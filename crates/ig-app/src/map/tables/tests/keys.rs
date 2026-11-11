use super::*;

#[test]
fn ends_come_from_nullable_unique_and_the_primary_key() {
    let nullable = Column {
        nullable: true,
        ..fk("reviewer", "users")
    };
    let unique = Column {
        unique: true,
        ..fk("passport", "passports")
    };
    let t = tables(
        &db(),
        &schema(vec![
            plain_table("users", vec![pk("id")]),
            plain_table("passports", vec![pk("id")]),
            plain_table(
                "posts",
                vec![pk("id"), fk("author", "users"), nullable.clone()],
            ),
            plain_table("people", vec![pk("id"), unique]),
            plain_table(
                "profiles",
                vec![Column {
                    pk: true,
                    ..fk("user_id", "users")
                }],
            ),
            plain_table("reviews", vec![pk("id"), nullable]),
        ]),
    );
    assert_eq!(
        find(&t, "db#posts->db#users").ends,
        None,
        "author and reviewer disagree"
    );
    assert_eq!(
        find(&t, "db#posts->db#users").from_column.as_deref(),
        Some("author")
    );
    assert_eq!(
        find(&t, "db#reviews->db#users").ends,
        ends(End::Many0, End::ZeroOne)
    );
    assert_eq!(
        find(&t, "db#people->db#passports").ends,
        ends(End::ZeroOne, End::One)
    );
    assert_eq!(
        find(&t, "db#profiles->db#users").ends,
        ends(End::ZeroOne, End::One)
    );
}

#[test]
fn a_reader_that_does_not_say_nullability_gives_no_ends() {
    let mut s = schema(vec![
        plain_table("users", vec![pk("id")]),
        plain_table("posts", vec![pk("id"), fk("author", "users")]),
    ]);
    for t in &mut s.tables {
        t.source = "someorm".into();
    }
    let t = tables(&db(), &s);
    let a = find(&t, "db#posts->db#users");
    assert_eq!((a.ends, a.from_column.as_deref()), (None, Some("author")));
}

#[test]
fn the_referenced_column_is_as_written_else_the_one_primary_key() {
    let to_code = Column {
        references: Some(ColumnRef {
            table: "countries".into(),
            column: Some("code".into()),
        }),
        ..plain("country")
    };
    let t = tables(
        &db(),
        &schema(vec![
            plain_table("countries", vec![pk("id"), plain("code")]),
            plain_table("pairs", vec![pk("a"), pk("b")]),
            plain_table("users", vec![pk("id"), to_code, fk("pair", "pairs")]),
        ]),
    );
    assert_eq!(
        find(&t, "db#users->db#countries").to_column.as_deref(),
        Some("code")
    );
    assert_eq!(find(&t, "db#users->db#pairs").to_column, None);
}

#[test]
fn a_join_table_is_also_one_many_to_many_arrow() {
    let key = |n: &str, to: &str| Column {
        pk: true,
        ..fk(n, to)
    };
    let t = tables(
        &db(),
        &schema(vec![
            plain_table("users", vec![pk("id")]),
            plain_table("roles", vec![pk("role_id")]),
            plain_table(
                "user_roles",
                vec![
                    key("user_id", "users"),
                    key("role_id", "roles"),
                    plain("granted_by"),
                    plain("created_at"),
                    plain("deleted"),
                ],
            ),
            plain_table(
                "user_groups",
                vec![
                    key("user_id", "users"),
                    key("role_id", "roles"),
                    plain("a"),
                    plain("b"),
                    plain("c"),
                ],
            ),
            plain_table("user_self", vec![key("a", "users"), key("b", "users")]),
        ]),
    );
    let v = find(&t, "db#users->db#roles via user_roles");
    assert_eq!(v.via.as_deref(), Some("user_roles"));
    assert_eq!(v.label.as_deref(), Some("user_roles"));
    assert_eq!(v.ends, ends(End::Many0, End::Many0));
    assert_eq!(
        (v.from_column.as_deref(), v.to_column.as_deref()),
        (Some("id"), Some("role_id"))
    );
    find(&t, "db#user_roles->db#users");
    let vias: Vec<&str> = t.arrows.iter().filter_map(|a| a.via.as_deref()).collect();
    assert_eq!(vias, ["user_roles"]);
}

#[test]
fn a_table_pointed_at_by_ten_tables_is_a_hub() {
    let mut ts = vec![
        plain_table("users", vec![pk("id")]),
        plain_table("teams", vec![pk("id")]),
        plain_table("roles", vec![pk("id")]),
        plain_table(
            "role_users",
            vec![
                Column {
                    pk: true,
                    ..fk("role", "roles")
                },
                Column {
                    pk: true,
                    ..fk("user", "users")
                },
            ],
        ),
    ];
    for i in 0..10 {
        ts.push(plain_table(
            &format!("t{i}"),
            vec![
                pk("id"),
                fk("by", "users"),
                fk("team", if i < 9 { "teams" } else { "t0" }),
            ],
        ));
    }
    let t = tables(&db(), &schema(ts));
    let to_users: Vec<&MapArrow> = t.arrows.iter().filter(|a| a.to == "db#users").collect();
    assert_eq!(to_users.len(), 12);
    assert!(find(&t, "db#roles->db#users via role_users").pinned);
    assert!(to_users.iter().all(|a| a.pinned));
    assert!(
        t.arrows
            .iter()
            .filter(|a| a.to != "db#users")
            .all(|a| !a.pinned)
    );
}

#[test]
fn join_and_translation_tables_do_not_count_towards_a_hub() {
    let mut ts = vec![
        plain_table("teams", vec![pk("id")]),
        plain_table("users", vec![pk("id")]),
        plain_table(
            "team_users",
            vec![
                Column {
                    pk: true,
                    ..fk("team", "teams")
                },
                Column {
                    pk: true,
                    ..fk("user", "users")
                },
            ],
        ),
        plain_table(
            "teams_translations",
            vec![pk("id"), fk("team", "teams"), plain("name")],
        ),
    ];
    for i in 0..8 {
        ts.push(plain_table(
            &format!("t{i}"),
            vec![pk("id"), fk("team", "teams")],
        ));
    }
    let t = tables(&db(), &schema(ts));
    assert_eq!(t.arrows.iter().filter(|a| a.to == "db#teams").count(), 10);
    assert!(t.arrows.iter().all(|a| !a.pinned));
}
