//! Django models: each concrete `models.Model` subclass a table (`Meta.db_table`, else
//! `<app>_<model>`), its fields the columns, a `ForeignKey` the `<name>_id` column. Abstract
//! bases lend their fields; proxies and `ManyToManyField`s have no table or column of their own.

use crate::Found;
use crate::py::{self, Class, Line, assign, call};
use crate::text::{arg, kwarg, split_top, truthy, unquote};
use ig_core::model::{Column, ColumnRef, Range, Table};

/// A model class as read, before inheritance and foreign keys resolve.
struct Model {
    name: String,
    app: String,
    file: String,
    bases: Vec<String>,
    line: u32,
    end: u32,
    table: Option<String>,
    abstract_: bool,
    proxy: bool,
    fields: Vec<Field>,
}

struct Field {
    name: String,
    kind: String,
    args: String,
    line: u32,
}

const USER_BASES: &[&str] = &["AbstractUser", "AbstractBaseUser"];

/// The app a models file belongs to: the folder holding `models.py` or the `models/` package.
fn app(path: &str) -> String {
    let parts: Vec<&str> = path.split('/').collect();
    let dirs = &parts[..parts.len() - 1];
    let at = dirs
        .iter()
        .rposition(|d| *d == "models")
        .unwrap_or(dirs.len());
    at.checked_sub(1).map_or("", |i| dirs[i]).to_string()
}

fn base_name(b: &str) -> &str {
    b.rsplit('.').next().unwrap_or(b)
}

fn meta(lines: &[Line], all: &[Class], class: &Class, key: &str) -> Option<String> {
    let m = py::nested(all, class, "Meta")?;
    m.body.iter().find_map(|&i| match assign(&lines[i].text) {
        (k, Some(v)) if k == key => Some(v.to_string()),
        _ => None,
    })
}

fn models(path: &str, text: &str) -> Vec<Model> {
    let lines = py::lines(text);
    let all = py::classes(&lines);
    let app = app(path);
    (all.iter().filter(|c| c.indent == 0))
        .map(|c| {
            let fields = c.body.iter().filter_map(|&i| field(&lines[i])).collect();
            let label =
                meta(&lines, &all, c, "app_label").and_then(|v| unquote(&v).map(str::to_string));
            Model {
                name: c.name.clone(),
                app: label.unwrap_or_else(|| app.clone()),
                file: path.into(),
                bases: c.bases.iter().map(|b| base_name(b).to_string()).collect(),
                line: c.line,
                end: c.end,
                table: meta(&lines, &all, c, "db_table")
                    .and_then(|v| unquote(&v).map(str::to_string)),
                abstract_: truthy(meta(&lines, &all, c, "abstract").as_deref()) == Some(true),
                proxy: truthy(meta(&lines, &all, c, "proxy").as_deref()) == Some(true),
                fields,
            }
        })
        .collect()
}

fn field(l: &Line) -> Option<Field> {
    let (target, value) = assign(&l.text);
    let name = target.split(':').next()?.trim();
    let (kind, args) = call(value?)?;
    let is_field = kind.ends_with("Field") || kind.ends_with("ForeignKey");
    (is_field && name.chars().all(|c| c.is_alphanumeric() || c == '_')).then(|| Field {
        name: name.into(),
        kind: kind.into(),
        args: args.into(),
        line: l.line,
    })
}

/// The Django model tables in `files`.
pub fn read(files: &[(&str, &str)]) -> Found {
    let mut all: Vec<Model> = (files.iter())
        .filter(|(_, t)| t.contains("models.Model") || t.contains("django.db"))
        .flat_map(|(p, t)| models(p, t))
        .collect();
    let mut known: Vec<String> = vec![];
    loop {
        let before = known.len();
        for m in &all {
            let model = m
                .bases
                .iter()
                .any(|b| b == "Model" || USER_BASES.contains(&b.as_str()) || known.contains(b));
            if model && !known.contains(&m.name) {
                known.push(m.name.clone());
            }
        }
        if known.len() == before {
            break;
        }
    }
    all.retain(|m| known.contains(&m.name));
    let tables = (all.iter())
        .filter(|m| !m.abstract_ && !m.proxy)
        .map(|m| table(m, &all))
        .collect();
    Found {
        tables,
        dialect: None,
        notices: vec![],
    }
}

fn table_of(m: &Model) -> String {
    m.table.clone().unwrap_or_else(|| match m.app.as_str() {
        "" => m.name.to_lowercase(),
        app => format!("{app}_{}", m.name.to_lowercase()),
    })
}

/// The table a `ForeignKey`'s target names: a class, `"app.Model"`, `"self"`, the user model.
fn target(expr: &str, m: &Model, all: &[Model]) -> String {
    let e = unquote(expr).unwrap_or(expr.trim());
    if e == "self" {
        return table_of(m);
    }
    if e.contains("AUTH_USER_MODEL") || e.starts_with("get_user_model") {
        let user = all
            .iter()
            .find(|u| u.bases.iter().any(|b| USER_BASES.contains(&b.as_str())));
        return user.map_or("auth_user".into(), table_of);
    }
    let (app, name) = match e.rsplit_once('.') {
        Some((a, n)) if unquote(expr).is_some() => (Some(a), n),
        Some((_, n)) => (None, n),
        None => (None, e),
    };
    let hit = |same_app: bool| {
        all.iter().find(|x| {
            x.name == name
                && match app {
                    Some(a) => x.app.eq_ignore_ascii_case(a),
                    None => !same_app || x.app == m.app,
                }
        })
    };
    match (hit(true).or_else(|| hit(false)), app) {
        (Some(x), _) => table_of(x),
        (None, Some(a)) => format!("{}_{}", a.to_lowercase(), name.to_lowercase()),
        (None, None) => name.to_string(),
    }
}

/// `m`'s fields with those of its abstract bases first, and a parent link for a concrete base.
fn inherited<'a>(
    m: &'a Model,
    all: &'a [Model],
    depth: usize,
) -> (Vec<&'a Field>, Option<&'a Model>) {
    let mut fields: Vec<&Field> = vec![];
    let mut parent = None;
    for b in &m.bases {
        let Some(base) = all.iter().find(|x| &x.name == b) else {
            continue;
        };
        if !base.abstract_ {
            parent = parent.or(Some(base));
        } else if depth < 8 {
            fields.extend(inherited(base, all, depth + 1).0);
        }
    }
    for f in &m.fields {
        fields.retain(|x| x.name != f.name);
        fields.push(f);
    }
    (fields, parent)
}

fn table(m: &Model, all: &[Model]) -> Table {
    let (fields, parent) = inherited(m, all, 0);
    let mut columns: Vec<Column> = fields
        .iter()
        .filter(|f| f.kind != "ManyToManyField")
        .map(|f| column(f, m, all))
        .collect();
    if let Some(p) = parent.filter(|_| !columns.iter().any(|c| c.pk)) {
        let mut link = plain(
            &format!("{}_ptr_id", p.name.to_lowercase()),
            "OneToOneField",
            m.line,
        );
        link.pk = true;
        link.references = Some(ColumnRef {
            table: table_of(p),
            column: None,
        });
        columns.insert(0, link);
    }
    if !columns.iter().any(|c| c.pk) {
        let mut id = plain("id", "AutoField", m.line);
        id.pk = true;
        columns.insert(0, id);
    }
    Table {
        name: table_of(m),
        kind: "table".into(),
        source: "django".into(),
        file: m.file.clone(),
        range: Range {
            start_line: m.line,
            end_line: m.end,
        },
        columns,
        reads: vec![],
    }
}

fn plain(name: &str, ty: &str, line: u32) -> Column {
    Column {
        name: name.into(),
        ty: ty.into(),
        line,
        pk: false,
        nullable: false,
        unique: false,
        default: None,
        references: None,
        check: None,
    }
}

fn column(f: &Field, m: &Model, all: &[Model]) -> Column {
    let args = split_top(&f.args, b',');
    let get = |k: &str| arg(&args, k, kwarg);
    let flag = |k: &str| truthy(get(k)) == Some(true);
    let key = f.kind.ends_with("ForeignKey") || f.kind == "OneToOneField";
    let db_column = get("db_column").and_then(unquote).map(str::to_string);
    let name = db_column.unwrap_or_else(|| match key {
        true => format!("{}_id", f.name),
        false => f.name.clone(),
    });
    let mut c = plain(&name, &f.kind, f.line);
    c.pk = flag("primary_key");
    c.nullable = flag("null") && !c.pk;
    c.unique = flag("unique") || f.kind == "OneToOneField";
    c.default = get("default").map(str::to_string);
    if key {
        let to = (args
            .iter()
            .find(|(_, a)| kwarg(a).is_none())
            .map(|(_, a)| *a))
        .or(get("to"));
        c.references = to.map(|t| ColumnRef {
            table: target(t, m, all),
            column: get("to_field").and_then(unquote).map(str::to_string),
        });
    }
    c
}
