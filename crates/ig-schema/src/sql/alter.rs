//! `ALTER TABLE` and `DROP`: statements that change or remove what `CREATE` made.

use super::column::{Con, column, column_type, constraint};
use super::lex::{Cur, commas, source};
use super::{Db, col_at, find, same};
use ig_core::model::{Column, Range, Table};

impl Db<'_> {
    pub(super) fn drop(&mut self, mut c: Cur) {
        let line = c.t[0].line;
        let if_exists = c.eat_all(&["IF", "EXISTS"]);
        while let Some(name) = c.name() {
            match find(&self.tables, &name) {
                Some(i) => {
                    let gone = self.tables.remove(i).name;
                    self.unreference(&gone, None);
                    self.named.retain(|n| !same(&n.table, &gone));
                }
                None if !if_exists => {
                    self.notice(line, format!("DROP of unknown table {name}; skipped"))
                }
                None => {}
            }
            if !c.eat_p(",") {
                break;
            }
        }
    }

    /// Drops foreign keys to `table` (to its `column` only, when given).
    pub(super) fn unreference(&mut self, table: &str, column: Option<&str>) {
        for c in self.tables.iter_mut().flat_map(|t| t.columns.iter_mut()) {
            let hit = c.references.as_ref().is_some_and(|r| {
                same(&r.table, table)
                    && column.is_none_or(|col| r.column.as_deref().is_some_and(|rc| same(rc, col)))
            });
            if hit {
                c.references = None;
            }
        }
    }

    pub(super) fn alter(&mut self, mut c: Cur) {
        let line = c.t[0].line;
        let if_exists = c.eat_all(&["IF", "EXISTS"]);
        c.eat("ONLY");
        let Some(name) = c.name() else { return };
        c.eat_p("*");
        let Some(at) = find(&self.tables, &name) else {
            if !if_exists {
                self.notice(
                    line,
                    format!("ALTER TABLE on unknown table {name}; skipped"),
                );
            }
            return;
        };
        for action in commas(c.rest()) {
            let mut a = Cur::new(action);
            let line = action[0].line;
            if a.eat("ADD") {
                self.add(at, a);
            } else if a.eat("DROP") {
                self.drop_part(at, a, line);
            } else if a.eat("ALTER") {
                a.eat("COLUMN");
                self.alter_column(at, a, line);
            } else if a.eat("MODIFY") {
                a.eat("COLUMN");
                if let Some(col) = column(a.rest(), self.src, &mut self.hints) {
                    let name = col.name.clone();
                    self.replace_column(at, &name, col, line);
                }
            } else if a.eat("CHANGE") {
                a.eat("COLUMN");
                if let Some(old) = a.ident()
                    && let Some(col) = column(a.rest(), self.src, &mut self.hints)
                {
                    let new = col.name.clone();
                    self.rename_column(at, &old, &new, line);
                    self.replace_column(at, &new, col, line);
                }
            } else if a.eat("RENAME") {
                self.rename(at, a, line);
            }
        }
    }

    pub(super) fn add(&mut self, at: usize, mut a: Cur) {
        let rest = a.rest();
        let is_con = [
            "CONSTRAINT",
            "PRIMARY",
            "FOREIGN",
            "UNIQUE",
            "CHECK",
            "EXCLUDE",
            "INDEX",
            "KEY",
            "FULLTEXT",
            "SPATIAL",
        ]
        .iter()
        .any(|k| a.at(k));
        if is_con {
            if let Some(Some((name, con))) = constraint(rest, self.src) {
                let mut t = std::mem::replace(&mut self.tables[at], placeholder());
                self.constrain(&mut t, name, con);
                self.tables[at] = t;
            }
            return;
        }
        a.eat("COLUMN");
        let if_not = a.eat_all(&["IF", "NOT", "EXISTS"]);
        let defs = match a.group() {
            Some(g) => commas(g),
            None => vec![a.rest()],
        };
        for def in defs {
            let Some(col) = column(def, self.src, &mut self.hints) else {
                continue;
            };
            let t = &self.tables[at];
            match col_at(t, &col.name) {
                Some(_) if if_not => {}
                Some(i) => self.tables[at].columns[i] = col.clone(),
                None => self.tables[at].columns.push(col.clone()),
            }
            let name = self.tables[at].name.clone();
            self.column_names(&name, &col);
        }
    }

    pub(super) fn drop_part(&mut self, at: usize, mut a: Cur, line: u32) {
        let table = self.tables[at].name.clone();
        if a.eat("CONSTRAINT") || a.eat_all(&["FOREIGN", "KEY"]) {
            a.eat_all(&["IF", "EXISTS"]);
            if let Some(name) = a.ident() {
                self.drop_constraint(at, &name);
            }
            return;
        }
        if a.eat_all(&["PRIMARY", "KEY"]) {
            for c in &mut self.tables[at].columns {
                c.pk = false;
            }
            return;
        }
        if a.at("INDEX") || a.at("KEY") {
            return;
        }
        a.eat("COLUMN");
        let if_exists = a.eat_all(&["IF", "EXISTS"]);
        let Some(name) = a.ident() else { return };
        match col_at(&self.tables[at], &name) {
            Some(i) => {
                self.tables[at].columns.remove(i);
                self.unreference(&table, Some(&name));
                self.named.retain(|n| {
                    !(same(&n.table, &table) && con_cols(&n.con).iter().any(|c| same(c, &name)))
                });
            }
            None if !if_exists => self.notice(
                line,
                format!("DROP COLUMN of unknown column {table}.{name}; skipped"),
            ),
            None => {}
        }
    }

    pub(super) fn drop_constraint(&mut self, at: usize, name: &str) {
        let table = self.tables[at].name.clone();
        let Some(i) =
            (self.named.iter()).position(|n| same(&n.table, &table) && same(&n.name, name))
        else {
            return;
        };
        let n = self.named.remove(i);
        let t = &mut self.tables[at];
        for c in t
            .columns
            .iter_mut()
            .filter(|c| con_cols(&n.con).iter().any(|n| same(n, &c.name)))
        {
            match &n.con {
                Con::Pk(_) => c.pk = false,
                Con::Fk(..) => c.references = None,
                Con::Unique(_) => c.unique = false,
                Con::Check(_) => {}
            }
        }
        if let Con::Check(expr) = &n.con {
            for c in &mut t.columns {
                if c.check.as_deref() == Some(expr.as_str()) {
                    c.check = None;
                }
            }
        }
    }

    pub(super) fn alter_column(&mut self, at: usize, mut a: Cur, line: u32) {
        let Some(name) = a.ident() else { return };
        let table = self.tables[at].name.clone();
        let Some(i) = col_at(&self.tables[at], &name) else {
            self.notice(
                line,
                format!("ALTER COLUMN of unknown column {table}.{name}; skipped"),
            );
            return;
        };
        if a.eat("TYPE") || a.eat_all(&["SET", "DATA", "TYPE"]) {
            let ty = column_type(&mut a, &mut self.hints);
            self.tables[at].columns[i].ty = ty;
            return;
        }
        let c = &mut self.tables[at].columns[i];
        if a.eat_all(&["SET", "NOT", "NULL"]) {
            c.nullable = false;
        } else if a.eat_all(&["DROP", "NOT", "NULL"]) {
            c.nullable = true;
        } else if a.eat_all(&["SET", "DEFAULT"]) {
            c.default = Some(source(self.src, a.rest()));
        } else if a.eat_all(&["DROP", "DEFAULT"]) {
            c.default = None;
        }
    }

    pub(super) fn replace_column(&mut self, at: usize, name: &str, mut col: Column, line: u32) {
        let table = self.tables[at].name.clone();
        match col_at(&self.tables[at], name) {
            Some(i) => {
                col.line = self.tables[at].columns[i].line;
                self.tables[at].columns[i] = col;
            }
            None => self.notice(
                line,
                format!("MODIFY of unknown column {table}.{name}; skipped"),
            ),
        }
    }

    pub(super) fn rename(&mut self, at: usize, mut a: Cur, line: u32) {
        if a.eat("TO") || a.eat("AS") {
            if let Some(new) = a.name() {
                let old = std::mem::replace(&mut self.tables[at].name, new.clone());
                for r in self
                    .tables
                    .iter_mut()
                    .flat_map(|t| t.columns.iter_mut())
                    .filter_map(|c| c.references.as_mut())
                {
                    if same(&r.table, &old) {
                        r.table = new.clone();
                    }
                }
                for n in self.named.iter_mut().filter(|n| same(&n.table, &old)) {
                    n.table = new.clone();
                }
            }
            return;
        }
        if a.eat("CONSTRAINT") {
            let table = self.tables[at].name.clone();
            if let (Some(old), true, Some(new)) = (a.ident(), a.eat("TO"), a.ident()) {
                for n in self
                    .named
                    .iter_mut()
                    .filter(|n| same(&n.table, &table) && same(&n.name, &old))
                {
                    n.name = new.clone();
                }
            }
            return;
        }
        a.eat("COLUMN");
        if let (Some(old), true, Some(new)) = (a.ident(), a.eat("TO"), a.ident()) {
            self.rename_column(at, &old, &new, line);
        }
    }

    pub(super) fn rename_column(&mut self, at: usize, old: &str, new: &str, line: u32) {
        let table = self.tables[at].name.clone();
        let Some(i) = col_at(&self.tables[at], old) else {
            self.notice(
                line,
                format!("RENAME of unknown column {table}.{old}; skipped"),
            );
            return;
        };
        if !same(old, new) && col_at(&self.tables[at], new).is_some() {
            self.notice(
                line,
                format!("RENAME of {table}.{old} to {new}, a column it has; skipped"),
            );
            return;
        }
        self.tables[at].columns[i].name = new.into();
        for r in self
            .tables
            .iter_mut()
            .flat_map(|t| t.columns.iter_mut())
            .filter_map(|c| c.references.as_mut())
        {
            if same(&r.table, &table) && r.column.as_deref().is_some_and(|c| same(c, old)) {
                r.column = Some(new.into());
            }
        }
        for n in self.named.iter_mut().filter(|n| same(&n.table, &table)) {
            match &mut n.con {
                Con::Pk(cols) | Con::Unique(cols) | Con::Fk(cols, ..) => {
                    for c in cols.iter_mut().filter(|c| same(c, old)) {
                        *c = new.into();
                    }
                }
                Con::Check(_) => {}
            }
        }
    }
}

pub(super) fn con_cols(con: &Con) -> &[String] {
    match con {
        Con::Pk(c) | Con::Unique(c) | Con::Fk(c, ..) => c,
        Con::Check(_) => &[],
    }
}

pub(super) fn placeholder() -> Table {
    Table {
        name: String::new(),
        kind: String::new(),
        source: String::new(),
        file: String::new(),
        range: Range {
            start_line: 0,
            end_line: 0,
        },
        columns: vec![],
        reads: vec![],
    }
}
