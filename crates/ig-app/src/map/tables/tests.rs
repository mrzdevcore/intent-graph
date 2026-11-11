use super::*;
use crate::map::arrows::lift;
use crate::map::layout::layout_with;
use ig_core::model::{Column, ColumnRef, Range};
use serde_json::json;

fn store(id: &str, title: &str) -> MapBox {
    let f = |v: &str| json!({ "value": v, "source": "ig" });
    serde_json::from_value(json!({
        "id": id, "parent": "system", "level": "container", "madeOf": [], "files": 0,
        "kind": f("store"), "title": f(title),
    }))
    .unwrap()
}

fn map(boxes: Vec<MapBox>) -> MapFile {
    MapFile {
        version: 1,
        boxes,
        arrows: vec![],
        marks: vec![],
    }
}

fn col(name: &str, refs: Option<&str>) -> Column {
    Column {
        name: name.into(),
        ty: "uuid".into(),
        line: 1,
        pk: refs.is_none(),
        nullable: false,
        unique: false,
        default: None,
        references: refs.map(|t| ColumnRef {
            table: t.into(),
            column: None,
        }),
        check: None,
    }
}

fn table(name: &str, kind: &str, columns: Vec<Column>, reads: &[&str]) -> Table {
    Table {
        name: name.into(),
        kind: kind.into(),
        source: "sql".into(),
        file: "db/V1__init.sql".into(),
        range: Range {
            start_line: 1,
            end_line: 3,
        },
        columns,
        reads: reads.iter().map(|r| r.to_string()).collect(),
    }
}

fn pk(name: &str) -> Column {
    col(name, None)
}

fn fk(name: &str, to: &str) -> Column {
    col(name, Some(to))
}

fn plain(name: &str) -> Column {
    Column {
        pk: false,
        ..col(name, None)
    }
}

fn plain_table(name: &str, columns: Vec<Column>) -> Table {
    table(name, "table", columns, &[])
}

fn schema(tables: Vec<Table>) -> Schema {
    Schema {
        dialect: Some("postgresql".into()),
        tables,
    }
}

fn db() -> MapFile {
    map(vec![store("db", "Database")])
}

fn parent_of<'a>(t: &'a Tables, id: &str) -> &'a str {
    let b = t.boxes.iter().find(|b| b.id == id).unwrap();
    b.parent.as_deref().unwrap()
}

fn find<'a>(t: &'a Tables, id: &str) -> &'a MapArrow {
    (t.arrows.iter()).find(|a| a.id == id).unwrap_or_else(|| {
        panic!(
            "no arrow {id} in {:#?}",
            t.arrows.iter().map(|a| &a.id).collect::<Vec<_>>()
        )
    })
}

fn ends(from: End, to: End) -> Option<Ends> {
    Some(Ends { from, to })
}

mod keys;

fn notes() -> Schema {
    Schema {
        dialect: Some("postgresql".into()),
        tables: vec![
            table("batches", "table", vec![col("id_batch", None)], &[]),
            table(
                "batches_notes",
                "table",
                vec![
                    col("id_batch_note", None),
                    col("batch", Some("batches")),
                    col("previous_batch", Some("BATCHES")),
                    col("profile", Some("profiles")),
                ],
                &[],
            ),
            table("v_notes", "view", vec![], &["batches_notes", "missing"]),
        ],
    }
}

#[test]
fn tables_sit_in_the_database_of_their_dialect() {
    let m = map(vec![
        store("redis", "Redis"),
        store("cache", "Cache"),
        store("postgres", "PostgreSQL"),
    ]);
    let t = tables(&m, &notes());
    let ids: Vec<&str> = t.boxes.iter().map(|b| b.id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "postgres#(views)",
            "postgres#batches",
            "postgres#batches_notes",
            "postgres#v_notes"
        ]
    );
    let tables: Vec<&MapBox> = t
        .boxes
        .iter()
        .filter(|b| b.kind.value == BoxKind::Table)
        .collect();
    assert!(
        tables
            .iter()
            .all(|b| b.level == Level::Code && !b.needs_reading)
    );
    assert_eq!(parent_of(&t, "postgres#batches"), "postgres");
    assert_eq!(parent_of(&t, "postgres#v_notes"), "postgres#(views)");
    assert_eq!(parent_of(&t, "postgres#(views)"), "postgres");
    assert_eq!(t.tables["postgres#v_notes"].table.kind, "view");
}

#[test]
fn a_foreign_key_is_one_arrow_per_table_and_a_view_reads() {
    let t = tables(&db(), &notes());
    let arrows: Vec<(&str, &str, ArrowKind, Option<&str>)> = (t.arrows.iter())
        .map(|a| (a.id.as_str(), a.parent.as_str(), a.kind, a.label.as_deref()))
        .collect();
    assert_eq!(
        arrows,
        [
            (
                "db#batches_notes->db#batches",
                "db",
                ArrowKind::Uses,
                Some("batch, previous_batch")
            ),
            ("db#v_notes->db#batches_notes", "db", ArrowKind::Reads, None),
        ]
    );
    let fk = &t.arrows[0];
    assert_eq!(fk.from_column.as_deref(), Some("batch"));
    assert_eq!(fk.to_column.as_deref(), Some("id_batch"));
    assert_eq!(fk.ends, ends(End::Many0, End::One));
    assert_eq!(t.arrows[1].ends, None);
}

#[test]
fn a_cache_or_bucket_never_holds_tables() {
    let t = tables(
        &map(vec![store("redis", "Redis"), store("s3", "AWS S3")]),
        &notes(),
    );
    assert!(t.boxes.is_empty());
    assert_eq!(
        t.notices,
        ["3 tables read, but no store on the map to draw them in"]
    );
    assert!(tables(&map(vec![]), &Schema::default()).notices.is_empty());
}

#[test]
fn another_sql_database_comes_before_a_drawn_store() {
    let m = map(vec![store("a-store", "Files"), store("mysql", "MySQL")]);
    let t = tables(&m, &notes());
    assert_eq!(t.boxes[0].parent.as_deref(), Some("mysql"));
}

#[test]
fn tables_group_by_their_prefix_and_a_table_named_like_it_joins() {
    let t = tables(
        &db(),
        &schema(vec![
            plain_table("batches", vec![pk("id")]),
            plain_table("batch_items", vec![pk("id")]),
            plain_table("batches_notes", vec![pk("id"), fk("batch", "batches")]),
            plain_table("BATCHES_cycles", vec![pk("id"), fk("batch", "batches")]),
            plain_table("users", vec![pk("id")]),
            plain_table("users_roles_x", vec![pk("id")]),
            plain_table("_prisma_migrations", vec![pk("id")]),
        ]),
    );
    assert_eq!(parent_of(&t, "db#batches"), "db#batches_*");
    assert_eq!(parent_of(&t, "db#BATCHES_cycles"), "db#batches_*");
    assert_eq!(parent_of(&t, "db#batch_items"), "db#(other)");
    assert_eq!(parent_of(&t, "db#users"), "db#(other)");
    assert_eq!(parent_of(&t, "db#_prisma_migrations"), "db#(other)");
    let group = t.boxes.iter().find(|b| b.id == "db#batches_*").unwrap();
    assert_eq!(
        (
            group.kind.value,
            group.title.value.as_str(),
            group.parent.as_deref(),
            group.level
        ),
        (BoxKind::Group, "batches", Some("db"), Level::Component)
    );
    assert_eq!(
        t.groups
            .iter()
            .map(|(g, k)| (g.as_str(), *k))
            .collect::<Vec<_>>(),
        [
            ("db#(other)", TableGroup::Other),
            ("db#batches_*", TableGroup::Domain)
        ]
    );
    assert_eq!(
        find(&t, "db#batches_notes->db#batches").parent,
        "db#batches_*"
    );
}

#[test]
fn with_no_domain_the_tables_sit_in_the_store_itself() {
    let t = tables(
        &db(),
        &schema(vec![
            plain_table("users", vec![pk("id")]),
            plain_table("posts", vec![pk("id"), fk("author", "users")]),
            plain_table("tags", vec![pk("id")]),
            table("v_posts", "view", vec![], &["posts"]),
        ]),
    );
    assert_eq!(t.groups.keys().collect::<Vec<_>>(), ["db#(views)"]);
    for id in ["db#users", "db#posts", "db#tags"] {
        assert_eq!(parent_of(&t, id), "db");
    }
    assert_eq!(parent_of(&t, "db#v_posts"), "db#(views)");
    assert_eq!(find(&t, "db#posts->db#users").parent, "db");
    let mut t = t;
    let mut boxes = vec![store("db", "Database")];
    boxes.append(&mut t.boxes);
    let parents: HashMap<&str, &str> = (boxes.iter())
        .filter_map(|b| Some((b.id.as_str(), b.parent.as_deref()?)))
        .collect();
    let arrows = lift(&parents, t.arrows.clone());
    let read = arrows.iter().find(|a| a.kind == ArrowKind::Reads).unwrap();
    assert_eq!(
        (read.from.as_str(), read.to.as_str()),
        ("db#(views)", "db#posts")
    );
    let rows = (t.tables.iter())
        .map(|(id, v)| (id.clone(), v.table.columns.len() as u32))
        .collect();
    let (placed, unrouted) = layout_with(&boxes, &arrows, &rows);
    assert!(unrouted.is_empty(), "{unrouted:?}");
    assert!(
        boxes
            .iter()
            .filter(|b| b.id != "db")
            .all(|b| placed.frames.contains_key(&b.id))
    );
}

#[test]
fn backups_and_temp_tables_are_set_aside_and_do_not_make_a_domain() {
    let t = tables(
        &db(),
        &schema(vec![
            plain_table("orders", vec![pk("id")]),
            plain_table("orders_backup", vec![pk("id")]),
            plain_table("orders_backup_2023", vec![pk("id")]),
            plain_table("orders_temp", vec![pk("id")]),
            plain_table("orders_lines", vec![pk("id")]),
        ]),
    );
    assert_eq!(parent_of(&t, "db#orders_backup_2023"), "db#(leftovers)");
    assert_eq!(parent_of(&t, "db#orders_temp"), "db#(leftovers)");
    assert_eq!(parent_of(&t, "db#orders"), "db");
    assert_eq!(t.groups["db#(leftovers)"], TableGroup::Leftovers);
}

#[test]
fn translations_fold_into_their_table_and_sit_apart() {
    let t = tables(
        &db(),
        &schema(vec![
            plain_table("products", vec![pk("id"), plain("sku")]),
            plain_table(
                "products_translations",
                vec![
                    pk("id"),
                    fk("product_id", "products"),
                    plain("locale"),
                    plain("name"),
                    plain("description"),
                    plain("created_at"),
                ],
            ),
            plain_table("orphan_translations", vec![pk("id"), plain("name")]),
            plain_table("kinds", vec![pk("id")]),
            plain_table(
                "kind_translations",
                vec![pk("id"), fk("kind", "kinds"), plain("label")],
            ),
            plain_table(
                "kinds_translations",
                vec![pk("id"), fk("kind", "kinds"), plain("label"), plain("hint")],
            ),
            plain_table("languages", vec![pk("id")]),
            plain_table("tags", vec![pk("id")]),
            plain_table(
                "tags_translations",
                vec![
                    Column {
                        pk: true,
                        ..fk("tag", "tags")
                    },
                    Column {
                        pk: true,
                        ..fk("language", "languages")
                    },
                    plain("label"),
                ],
            ),
        ]),
    );
    assert_eq!(
        t.tables["db#products"].translations,
        ["name", "description"]
    );
    assert_eq!(
        parent_of(&t, "db#products_translations"),
        "db#(translations)"
    );
    assert_eq!(t.groups["db#(translations)"], TableGroup::Translations);
    assert_eq!(parent_of(&t, "db#orphan_translations"), "db");
    assert!(t.tables["db#orphan_translations"].translations.is_empty());
    let a = find(&t, "db#products_translations->db#products");
    assert_eq!(a.parent, "db");
    assert_eq!(t.tables["db#tags"].translations, ["label"]);
    assert_eq!(t.tables["db#kinds"].translations, ["label", "hint"]);
    assert!(
        t.arrows.iter().all(|a| a.via.is_none()),
        "a translation is no join table"
    );
}

#[test]
fn a_self_reference_or_a_missing_table_draws_no_wire() {
    let t = tables(
        &db(),
        &schema(vec![
            plain_table(
                "nodes",
                vec![pk("id"), fk("parent", "NODES"), fk("owner", "missing")],
            ),
            plain_table(
                "pairs",
                vec![
                    Column {
                        pk: true,
                        ..fk("a", "missing")
                    },
                    Column {
                        pk: true,
                        ..fk("b", "nodes")
                    },
                ],
            ),
        ]),
    );
    assert!(t.arrows.iter().all(|a| a.from != a.to));
    assert_eq!(
        t.arrows.iter().map(|a| a.id.as_str()).collect::<Vec<_>>(),
        ["db#pairs->db#nodes"]
    );
}

#[test]
fn arrows_between_domains_lift_to_their_groups_and_every_table_has_a_place() {
    let mut ts = vec![];
    for n in ["a_one", "a_two", "a_three", "b_one", "b_two", "b_three"] {
        ts.push(plain_table(n, vec![pk("id")]));
    }
    ts[0].columns.push(fk("b", "b_one"));
    ts[1].columns.push(fk("b", "b_two"));
    ts[2].columns.push(fk("a", "a_one"));
    let mut t = tables(&db(), &schema(ts));
    let mut boxes = vec![store("db", "Database")];
    boxes.append(&mut t.boxes);
    let parents: HashMap<&str, &str> = (boxes.iter())
        .filter_map(|b| Some((b.id.as_str(), b.parent.as_deref()?)))
        .collect();
    let arrows = lift(&parents, t.arrows.clone());
    let a = arrows.iter().find(|a| a.from == "db#a_*").unwrap();
    assert_eq!((a.to.as_str(), a.parts.len()), ("db#b_*", 2));
    assert!(a.from_column.is_none() && a.ends.is_none());
    assert_eq!(a.parts[0].ends, ends(End::Many0, End::One));
    assert!(
        arrows
            .iter()
            .any(|a| a.id == "db#a_three->db#a_one" && a.parent == "db#a_*")
    );
    let rows = (t.tables.iter())
        .map(|(id, v)| (id.clone(), v.table.columns.len() as u32))
        .collect();
    let (placed, unrouted) = layout_with(&boxes, &arrows, &rows);
    assert!(unrouted.is_empty(), "{unrouted:?}");
    assert!(
        boxes
            .iter()
            .filter(|b| b.id != "db")
            .all(|b| placed.frames.contains_key(&b.id))
    );
}

#[test]
fn the_view_names_its_fields_as_the_viewer_reads_them() {
    let t = tables(
        &db(),
        &schema(vec![
            plain_table("products", vec![pk("id")]),
            plain_table(
                "products_translations",
                vec![pk("id"), fk("product", "products"), plain("name")],
            ),
        ]),
    );
    let a = serde_json::to_value(find(&t, "db#products_translations->db#products")).unwrap();
    assert_eq!(a["fromColumn"], "product");
    assert_eq!(a["toColumn"], "id");
    assert_eq!(a["ends"], json!({ "from": "many0", "to": "one" }));
    assert!(a.get("pinned").is_none() && a.get("via").is_none());
    let p = serde_json::to_value(&t.tables["db#products"]).unwrap();
    assert_eq!(
        (p["name"].clone(), p["translations"].clone()),
        (json!("products"), json!(["name"]))
    );
    assert_eq!(
        serde_json::to_value(&t.groups).unwrap()["db#(translations)"],
        "translations"
    );
}
