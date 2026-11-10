use super::*;
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
            "postgres#batches",
            "postgres#batches_notes",
            "postgres#v_notes"
        ]
    );
    assert!(
        t.boxes
            .iter()
            .all(|b| b.parent.as_deref() == Some("postgres")
                && b.kind.value == BoxKind::Table
                && b.level == Level::Code
                && !b.needs_reading)
    );
    assert_eq!(t.tables["postgres#v_notes"].kind, "view");
}

#[test]
fn a_foreign_key_is_one_arrow_per_table_and_a_view_reads() {
    let t = tables(&map(vec![store("db", "Database")]), &notes());
    let arrows: Vec<(&str, ArrowKind, Option<&str>)> = (t.arrows.iter())
        .map(|a| (a.id.as_str(), a.kind, a.label.as_deref()))
        .collect();
    assert_eq!(
        arrows,
        [
            (
                "db#batches_notes->db#batches",
                ArrowKind::Uses,
                Some("batch, previous_batch")
            ),
            ("db#v_notes->db#batches_notes", ArrowKind::Reads, None),
        ]
    );
    assert!(t.arrows.iter().all(|a| a.parent == "db"));
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
