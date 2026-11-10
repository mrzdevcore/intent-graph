use super::*;
use ig_core::model::{ColumnRef, Range};

fn col(name: &str, ty: &str) -> Column {
    Column {
        name: name.into(),
        ty: ty.into(),
        line: 1,
        pk: false,
        nullable: true,
        unique: false,
        default: None,
        references: None,
        check: None,
    }
}

fn table(name: &str, kind: &str, columns: Vec<Column>) -> Table {
    Table {
        name: name.into(),
        kind: kind.into(),
        source: "sql".into(),
        file: "db.sql".into(),
        range: Range {
            start_line: 1,
            end_line: 1,
        },
        columns,
        reads: vec![],
    }
}

fn schema(tables: Vec<Table>) -> Schema {
    Schema {
        dialect: None,
        tables,
    }
}

fn notes(impact: Column, more: Option<Column>) -> Table {
    let mut cols = vec![col("id_batch_note", "uuid"), impact];
    cols.extend(more);
    table("batches_notes", "table", cols)
}

#[test]
fn columns_added_removed_and_changed() {
    let mut fk = col("impact", "varchar");
    fk.references = Some(ColumnRef {
        table: "categories".into(),
        column: None,
    });
    let base = schema(vec![
        notes(fk, Some(col("gap", "int4"))),
        table("gone", "table", vec![]),
    ]);
    let head = schema(vec![
        notes(col("impact", "varchar"), Some(col("impacts", "varchar[]"))),
        table("v_impacts", "view", vec![]),
    ]);
    let d = tables(&base, &head);
    let names = |r: &[TableRef]| r.iter().map(|t| t.name.clone()).collect::<Vec<_>>();
    assert_eq!(names(&d.added), ["v_impacts"]);
    assert_eq!(names(&d.removed), ["gone"]);
    let [c] = d.changed.as_slice() else {
        panic!("{:?}", d.changed)
    };
    assert_eq!(c.added[0].name, "impacts");
    assert_eq!(c.removed[0].name, "gap");
    assert_eq!(c.changed[0].name, "impact");
    assert_eq!(c.changed[0].what, ["foreign key"]);
    assert_eq!(
        lines(&d),
        [
            "view  +  v_impacts",
            "table −  gone",
            "table ~  batches_notes: + impacts varchar[], ~ impact (foreign key), − gap",
        ]
    );
}

#[test]
fn the_same_schema_or_case_alone_is_no_change() {
    let a = schema(vec![notes(col("impact", "varchar"), None)]);
    let mut b = a.clone();
    b.tables[0].name = "BATCHES_NOTES".into();
    b.tables[0].columns[1].ty = "VARCHAR".into();
    assert!(tables(&a, &b).is_empty());
    assert!(tables(&Schema::default(), &Schema::default()).is_empty());
}
