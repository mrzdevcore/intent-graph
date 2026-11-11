use super::*;

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
            plain_table("orders_backup_2023_01_01", vec![pk("id")]),
            plain_table("user_backup_codes", vec![pk("id")]),
            plain_table("orders_lines_backup_refacto", vec![pk("id")]),
            plain_table("user_temperature", vec![pk("id")]),
        ]),
    );
    assert_eq!(parent_of(&t, "db#orders_backup"), "db#(leftovers)");
    assert_eq!(
        parent_of(&t, "db#orders_backup_2023_01_01"),
        "db#(leftovers)"
    );
    assert_eq!(
        parent_of(&t, "db#orders_lines_backup_refacto"),
        "db#(leftovers)"
    );
    assert_eq!(parent_of(&t, "db#user_backup_codes"), "db");
    assert_eq!(parent_of(&t, "db#user_temperature"), "db");
    assert_eq!(parent_of(&t, "db#orders_backup_2023"), "db#(leftovers)");
    assert_eq!(parent_of(&t, "db#orders_temp"), "db#(leftovers)");
    assert_eq!(parent_of(&t, "db#orders"), "db");
    assert_eq!(t.groups["db#(leftovers)"], TableGroup::Leftovers);
}

#[test]
fn a_table_named_like_a_backup_of_another_is_one_only_when_it_copies_its_columns() {
    let user = || vec![pk("id"), plain("email"), plain("name")];
    let t = tables(
        &db(),
        &schema(vec![
            plain_table("user", user()),
            plain_table(
                "user_backup_codes",
                vec![pk("id"), fk("user_id", "user"), plain("code")],
            ),
            plain_table("user_backup_old", user()),
            plain_table(
                "batches_notes",
                vec![pk("id"), plain("text"), plain("batch")],
            ),
            plain_table(
                "batches_notes_backup_refacto",
                vec![pk("id"), plain("text"), plain("at")],
            ),
        ]),
    );
    assert_eq!(parent_of(&t, "db#user_backup_codes"), "db");
    assert_eq!(parent_of(&t, "db#user_backup_old"), "db#(leftovers)");
    assert_eq!(
        parent_of(&t, "db#batches_notes_backup_refacto"),
        "db#(leftovers)"
    );
}
