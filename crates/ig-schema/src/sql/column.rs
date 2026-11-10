//! Column definitions and table constraints, as `CREATE TABLE` and `ALTER TABLE` write them.

use super::Hints;
use super::lex::{Cur, K, Tok, commas, source, type_text};
use ig_core::model::{Column, ColumnRef};

/// Words that end a column's type and start its constraints.
pub(super) const COLUMN_STOP: &[&str] = &[
    "CONSTRAINT",
    "PRIMARY",
    "NOT",
    "NULL",
    "UNIQUE",
    "DEFAULT",
    "REFERENCES",
    "CHECK",
    "COLLATE",
    "GENERATED",
    "AUTO_INCREMENT",
    "AUTOINCREMENT",
    "COMMENT",
    "ON",
    "CHARSET",
    "AS",
    "USING",
    "IDENTITY",
    "FIRST",
    "AFTER",
    "STORED",
    "VIRTUAL",
    "INVISIBLE",
    "VISIBLE",
];

pub(super) fn stops(c: &Cur) -> bool {
    let Some(t) = c.peek() else { return true };
    COLUMN_STOP.iter().any(|k| t.is(k))
        || (t.is("CHARACTER") && c.t.get(c.i + 1).is_some_and(|n| n.is("SET")))
}

/// The type at the cursor, up to the first constraint.
pub(super) fn column_type(c: &mut Cur, hints: &mut Hints) -> String {
    let from = c.i;
    while !c.done() && !stops(c) {
        c.skip();
    }
    let ty = type_text(&c.t[from..c.i]);
    hints.of_type(&ty);
    ty
}

/// A table constraint, its column lists resolved later against the table.
pub(super) enum Con {
    Pk(Vec<String>),
    Fk(Vec<String>, String, Vec<String>),
    Unique(Vec<String>),
    Check(String),
}

/// A constraint as named (or as Postgres would name it), to be dropped by that name.
pub(super) struct Named {
    pub(super) table: String,
    pub(super) name: String,
    pub(super) con: Con,
}

/// A column list: `(a, b)`, lengths and sort orders dropped.
pub(super) fn names(toks: &[Tok]) -> Vec<String> {
    commas(toks)
        .into_iter()
        .filter_map(|p| p.first().filter(|t| t.ident()).map(|t| t.text.clone()))
        .collect()
}

/// `REFERENCES t [(c, ...)]` at the cursor, past the word.
pub(super) fn references(c: &mut Cur) -> Option<(String, Vec<String>)> {
    let table = c.name()?;
    let cols = c.group().map(names).unwrap_or_default();
    Some((table, cols))
}

/// Moves past `ON DELETE|UPDATE <action>` and `MATCH x`, `DEFERRABLE`, `INITIALLY x`.
pub(super) fn ref_actions(c: &mut Cur) {
    loop {
        if c.eat("ON") {
            let _ = c.eat("DELETE") || c.eat("UPDATE");
            let _ = c.eat_all(&["SET", "NULL"])
                || c.eat_all(&["SET", "DEFAULT"])
                || c.eat_all(&["NO", "ACTION"])
                || c.eat("CASCADE")
                || c.eat("RESTRICT")
                || c.eat("CURRENT_TIMESTAMP");
            c.group();
        } else if c.eat("MATCH") || c.eat("INITIALLY") {
            c.i += 1;
        } else if !(c.eat("DEFERRABLE") || c.eat_all(&["NOT", "DEFERRABLE"])) {
            return;
        }
    }
}

/// A column definition: name, type and constraints.
pub(super) fn column(toks: &[Tok], src: &str, hints: &mut Hints) -> Option<Column> {
    let mut c = Cur::new(toks);
    let line = c.peek()?.line;
    let name = c.ident()?;
    let mut col = Column {
        name,
        ty: column_type(&mut c, hints),
        line,
        pk: false,
        nullable: true,
        unique: false,
        default: None,
        references: None,
        check: None,
    };
    while !c.done() {
        if c.eat("CONSTRAINT") {
            c.ident();
        } else if c.eat("PRIMARY") {
            c.eat("KEY");
            col.pk = true;
            col.nullable = false;
        } else if c.eat_all(&["NOT", "NULL"]) {
            col.nullable = false;
        } else if c.eat("NULL") {
            col.nullable = !col.pk;
        } else if c.eat("UNIQUE") {
            c.eat("KEY");
            col.unique = true;
        } else if c.eat("DEFAULT") {
            let from = c.i;
            c.skip();
            while !c.done() && !stops(&c) {
                c.skip();
            }
            col.default = Some(source(src, &toks[from..c.i]));
        } else if c.eat("REFERENCES") {
            if let Some((table, cols)) = references(&mut c) {
                col.references = Some(ColumnRef {
                    table,
                    column: cols.into_iter().next(),
                });
            }
            ref_actions(&mut c);
        } else if c.eat("CHECK") {
            let g = c.group().unwrap_or_default();
            col.check = Some(and(col.check.take(), source(src, g)));
        } else if c.eat("COLLATE")
            || c.eat("COMMENT")
            || c.eat("CHARSET")
            || c.eat_all(&["CHARACTER", "SET"])
        {
            c.name();
        } else if c.eat("GENERATED") {
            while !c.done() && !c.eat("AS") {
                c.i += 1;
            }
            c.eat("IDENTITY");
            c.group();
        } else if c.eat("AUTO_INCREMENT") {
            hints.mysql += 1;
        } else if c.eat("AUTOINCREMENT") {
            hints.sqlite += 1;
        } else {
            c.skip();
        }
    }
    Some(col)
}

pub(super) fn and(a: Option<String>, b: String) -> String {
    match a {
        Some(a) => format!("{a} AND {b}"),
        None => b,
    }
}

/// A table constraint, or None when `toks` is a column (or an index, which is `Some(None)`).
pub(super) fn constraint(toks: &[Tok], src: &str) -> Option<Option<(Option<String>, Con)>> {
    let mut c = Cur::new(toks);
    let name = c.eat("CONSTRAINT").then(|| c.ident()).flatten();
    let con = if c.eat_all(&["PRIMARY", "KEY"]) {
        Con::Pk(c.group().map(names).unwrap_or_default())
    } else if c.eat_all(&["FOREIGN", "KEY"]) {
        if !c.at_p("(") {
            c.ident();
        }
        let cols = c.group().map(names).unwrap_or_default();
        c.eat("REFERENCES");
        let (table, refs) = references(&mut c)?;
        Con::Fk(cols, table, refs)
    } else if c.eat("UNIQUE") {
        let _ = c.eat("KEY") || c.eat("INDEX");
        if !c.at_p("(") {
            c.ident();
        }
        Con::Unique(c.group().map(names).unwrap_or_default())
    } else if c.eat("CHECK") {
        Con::Check(source(src, c.group().unwrap_or_default()))
    } else if name.is_some() || c.at("EXCLUDE") || index(toks) {
        return Some(None);
    } else {
        return None;
    };
    Some(Some((name, con)))
}

/// A MySQL `KEY name (a, b)` or `INDEX (a)` element: a column list with no numbers.
pub(super) fn index(toks: &[Tok]) -> bool {
    let first = &toks[0];
    let kw = ["KEY", "INDEX", "FULLTEXT", "SPATIAL"]
        .iter()
        .any(|k| first.is(k));
    let group = toks.iter().position(|t| t.p("("));
    kw && group.is_some_and(|g| g <= 3 && !toks[g..].iter().any(|t| t.k == K::Num))
}
