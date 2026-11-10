use super::*;

#[test]
fn a_create_table_reads_types_constraints_and_table_keys() {
    let (s, notices) = schema(&[(
        "db/schema.sql",
        "-- users; and more\n\
         CREATE TABLE IF NOT EXISTS public.users (\n\
           id bigserial PRIMARY KEY,\n\
           email varchar(40) NOT NULL UNIQUE,\n\
           price numeric(10, 2) DEFAULT 0.00 CHECK (price >= 0),\n\
           tags varchar[],\n\
           seen timestamp with time zone NULL,\n\
           org_id int REFERENCES orgs ON DELETE SET NULL\n\
         );\n\
         CREATE TABLE memberships (\n\
           user_id int NOT NULL,\n\
           org_id int NOT NULL,\n\
           CONSTRAINT m_pk PRIMARY KEY (user_id, org_id),\n\
           FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE,\n\
           UNIQUE (org_id)\n\
         );\n\
         CREATE TABLE orgs (id uuid PRIMARY KEY DEFAULT gen_random_uuid(), name text);\n",
    )]);
    assert!(notices.is_empty(), "{notices:?}");
    assert_eq!(s.dialect.as_deref(), Some("postgresql"));
    assert_eq!(names(&s), ["memberships", "orgs", "users"]);
    let users = table(&s, "users");
    assert_eq!(
        (
            users.file.as_str(),
            users.range.start_line,
            users.range.end_line
        ),
        ("db/schema.sql", 2, 9)
    );
    assert_eq!(
        cols(users),
        ["id", "email", "price", "tags", "seen", "org_id"]
    );
    let id = col(users, "id");
    assert!(id.pk && !id.nullable && id.line == 3 && id.ty == "bigserial");
    let email = col(users, "email");
    assert!(email.unique && !email.nullable && email.ty == "varchar(40)");
    let price = col(users, "price");
    assert_eq!(
        (
            price.ty.as_str(),
            price.default.as_deref(),
            price.check.as_deref()
        ),
        ("numeric(10,2)", Some("0.00"), Some("price >= 0"))
    );
    assert_eq!(col(users, "tags").ty, "varchar[]");
    assert_eq!(col(users, "seen").ty, "timestamp with time zone");
    assert_eq!(fk(col(users, "org_id")), Some(("orgs", Some("id"))));
    let m = table(&s, "memberships");
    assert!(col(m, "user_id").pk && col(m, "org_id").pk && col(m, "org_id").unique);
    assert_eq!(fk(col(m, "user_id")), Some(("users", Some("id"))));
    assert_eq!(
        col(table(&s, "orgs"), "id").default.as_deref(),
        Some("gen_random_uuid()")
    );
}

#[test]
fn composite_foreign_keys_map_column_by_column() {
    let (s, _) = schema(&[(
        "a.sql",
        "CREATE TABLE p (a int, b int, PRIMARY KEY (a, b));\n\
         CREATE TABLE c (x int, y int, CONSTRAINT f FOREIGN KEY (x, y) REFERENCES p (a, b));",
    )]);
    let c = table(&s, "c");
    assert_eq!(fk(col(c, "x")), Some(("p", Some("a"))));
    assert_eq!(fk(col(c, "y")), Some(("p", Some("b"))));
}

#[test]
fn comments_strings_and_dollar_bodies_do_not_split_statements() {
    let (s, notices) = schema(&[(
        "a.sql",
        "/* CREATE TABLE ghost (x int); */\n\
         CREATE FUNCTION f() RETURNS trigger AS $body$ BEGIN DROP TABLE t; RETURN NEW; END; $body$ LANGUAGE plpgsql;\n\
         CREATE TABLE t (note text DEFAULT 'a;b', \"Quoted Col\" int);\n\
         INSERT INTO t VALUES ('x; DROP TABLE t;');\n\
         CREATE INDEX t_note ON t (note);\n",
    )]);
    assert!(notices.is_empty(), "{notices:?}");
    assert_eq!(names(&s), ["t"]);
    let t = table(&s, "t");
    assert_eq!(cols(t), ["note", "Quoted Col"]);
    assert_eq!(col(t, "note").default.as_deref(), Some("'a;b'"));
}

#[test]
fn migrations_apply_in_natural_order_so_the_last_state_wins() {
    let (s, notices) = schema(&[
        (
            "db/migration/V10__rename.sql",
            "ALTER TABLE accounts DROP COLUMN legacy, RENAME COLUMN nick TO handle;\n\
             ALTER TABLE accounts RENAME TO members;\n",
        ),
        (
            "db/migration/V2__more.sql",
            "ALTER TABLE accounts ADD COLUMN IF NOT EXISTS nick varchar(20), ADD legacy int;\n\
             ALTER TABLE accounts ALTER COLUMN name TYPE varchar(80), ALTER COLUMN name SET NOT NULL;\n\
             CREATE TABLE posts (id serial PRIMARY KEY, author int REFERENCES accounts(id));\n",
        ),
        (
            "db/migration/V1__init.sql",
            "CREATE TABLE accounts (id serial PRIMARY KEY, name text);\n",
        ),
        ("db/migration/U2__more.sql", "DROP TABLE accounts;"),
        ("db/migration/V2__more.down.sql", "DROP TABLE accounts;"),
    ]);
    assert!(notices.is_empty(), "{notices:?}");
    assert_eq!(names(&s), ["members", "posts"]);
    let m = table(&s, "members");
    assert_eq!(cols(m), ["id", "name", "handle"]);
    assert_eq!(m.file, "db/migration/V1__init.sql");
    let name = col(m, "name");
    assert!(name.ty == "varchar(80)" && !name.nullable && name.line == 1);
    assert_eq!(col(m, "handle").line, 1);
    assert_eq!(
        fk(col(table(&s, "posts"), "author")),
        Some(("members", Some("id")))
    );
}

#[test]
fn natural_order_and_down_migrations() {
    let mut p = vec![
        "m/V10__a.sql",
        "m/V2__b.sql",
        "m/R__views.sql",
        "m/V1_1__c.sql",
        "m/V1__d.sql",
    ];
    p.sort_by(|a, b| crate::sql::natural(a, b));
    assert_eq!(
        p,
        [
            "m/V1__d.sql",
            "m/V1_1__c.sql",
            "m/V2__b.sql",
            "m/V10__a.sql",
            "m/R__views.sql"
        ]
    );
    let mut q = vec![
        "m/010_x.sql",
        "m/9_y.sql",
        "m/20240102_z.sql",
        "m/20231231_w.sql",
    ];
    q.sort_by(|a, b| crate::sql::natural(a, b));
    assert_eq!(
        q,
        [
            "m/9_y.sql",
            "m/010_x.sql",
            "m/20231231_w.sql",
            "m/20240102_z.sql"
        ]
    );
    for down in [
        "a/1_x.down.sql",
        "a/1_x_down.sql",
        "a/2020_x/down.sql",
        "a/U3__x.sql",
    ] {
        assert!(crate::sql::is_down(down), "{down}");
    }
    assert!(
        !crate::sql::is_down("a/2020_x/up.sql") && !crate::sql::is_down("a/V3__undo_nothing.sql")
    );
}

#[test]
fn an_alter_on_a_missing_table_is_a_notice_not_a_panic() {
    let (s, notices) = schema(&[(
        "db/V3__x.sql",
        "CREATE TABLE a (id int);\n\nALTER TABLE foo ADD COLUMN x int;\nALTER TABLE IF EXISTS bar ADD y int;\n\
         ALTER TABLE a ALTER COLUMN nope SET NOT NULL;\nALTER TABLE",
    )]);
    assert_eq!(names(&s), ["a"]);
    assert_eq!(
        notices,
        [
            "db/V3__x.sql:3: ALTER TABLE on unknown table foo; skipped",
            "db/V3__x.sql:5: ALTER COLUMN of unknown column a.nope; skipped",
        ]
    );
}

#[test]
fn constraints_added_and_dropped_by_name() {
    let (s, _) = schema(&[(
        "a.sql",
        "CREATE TABLE p (id int);\nCREATE TABLE c (id int, p_id int REFERENCES p(id), code text);\n\
         ALTER TABLE ONLY p ADD CONSTRAINT p_pkey PRIMARY KEY (id);\n\
         ALTER TABLE c DROP CONSTRAINT c_p_id_fkey, ADD CONSTRAINT c_code UNIQUE (code), ADD CHECK (length(code) > 2);\n\
         ALTER TABLE c ALTER COLUMN code SET DEFAULT 'x', ALTER COLUMN code DROP DEFAULT;",
    )]);
    assert!(col(table(&s, "p"), "id").pk);
    let c = table(&s, "c");
    assert_eq!(fk(col(c, "p_id")), None);
    let code = col(c, "code");
    assert!(code.unique && code.default.is_none());
    assert_eq!(code.check.as_deref(), Some("length(code) > 2"));
}

#[test]
fn mysql_and_sqlite_syntax_are_tolerated() {
    let (my, _) = schema(&[(
        "my.sql",
        "CREATE TABLE `users` (\n  `id` int unsigned NOT NULL AUTO_INCREMENT,\n  `email` varchar(255) CHARACTER SET utf8mb4 NOT NULL,\n\
         `org` int DEFAULT NULL,\n  PRIMARY KEY (`id`),\n  UNIQUE KEY `uq_email` (`email`),\n  KEY `idx_org` (`org`),\n\
         CONSTRAINT `fk_org` FOREIGN KEY (`org`) REFERENCES `orgs` (`id`)\n) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;",
    )]);
    assert_eq!(my.dialect.as_deref(), Some("mysql"));
    let u = table(&my, "users");
    assert_eq!(cols(u), ["id", "email", "org"]);
    assert!(col(u, "id").pk && col(u, "email").unique && col(u, "email").ty == "varchar(255)");
    assert_eq!(col(u, "id").ty, "int unsigned");
    assert_eq!(fk(col(u, "org")), Some(("orgs", Some("id"))));
    let (lite, _) = schema(&[(
        "lite.sql",
        "CREATE TABLE t (id INTEGER PRIMARY KEY AUTOINCREMENT, n TEXT);",
    )]);
    assert_eq!(lite.dialect.as_deref(), Some("sqlite"));
    assert_eq!(col(table(&lite, "t"), "id").ty, "integer");
}

#[test]
fn views_read_their_sources_and_plain_columns() {
    let (s, _) = schema(&[(
        "v.sql",
        "CREATE TABLE a (id int PRIMARY KEY, name text);\nCREATE TABLE b (a_id int, n int);\n\
         CREATE OR REPLACE VIEW v AS\nWITH x AS (SELECT * FROM b)\nSELECT a.id, a.name AS label, count(*) total, extract(year FROM now()) AS y, n::text AS nt\n\
         FROM a JOIN x ON x.a_id = a.id LEFT JOIN public.c cc ON true;\n\
         CREATE MATERIALIZED VIEW m AS SELECT * FROM a;\nCREATE VIEW gone AS SELECT 1 AS one;\nDROP VIEW gone;",
    )]);
    assert_eq!(names(&s), ["a", "b", "m", "v"]);
    let v = table(&s, "v");
    assert_eq!(v.kind, "view");
    assert_eq!(v.reads, ["b", "a", "c"]);
    assert_eq!(cols(v), ["id", "label", "total", "y", "nt"]);
    assert_eq!(
        (
            col(v, "id").ty.as_str(),
            col(v, "label").ty.as_str(),
            col(v, "total").ty.as_str(),
            col(v, "nt").ty.as_str()
        ),
        ("int", "text", "", "text")
    );
    assert_eq!(cols(table(&s, "m")), ["id", "name"]);
}

#[test]
fn batches_notes_and_its_impacts_view() {
    let (s, notices) = schema(&[
        (
            "migrations/V1__batches.sql",
            "CREATE TABLE batches (id_batch uuid PRIMARY KEY DEFAULT gen_random_uuid(), code varchar(40) NOT NULL);\n\
             CREATE TABLE parameters_batch_note_categories (\n  id_batch_note_category varchar(40) PRIMARY KEY,\n  label text\n);\n",
        ),
        (
            "migrations/V2__batches_notes.sql",
            "CREATE TABLE batches_notes (\n\
               id_batch_note uuid NOT NULL DEFAULT gen_random_uuid(),\n\
               batch uuid NOT NULL,\n\
               impact varchar(40),\n\
               gap_minutes float4,\n\
               created_at timestamptz NOT NULL DEFAULT now(),\n\
               CONSTRAINT batches_notes_pkey PRIMARY KEY (id_batch_note),\n\
               CONSTRAINT batches_notes_batch_fkey FOREIGN KEY (batch) REFERENCES batches(id_batch) ON DELETE CASCADE,\n\
               CONSTRAINT batches_notes_gap_check CHECK (gap_minutes >= 0::double precision)\n\
             );\n",
        ),
        (
            "migrations/V3__impacts.sql",
            "ALTER TABLE batches_notes ADD COLUMN impacts varchar[] DEFAULT '{}'::character varying[];\n\
             ALTER TABLE batches_notes ADD CONSTRAINT batches_notes_impact_fkey FOREIGN KEY (impact)\n\
               REFERENCES parameters_batch_note_categories(id_batch_note_category);\n\
             CREATE TABLE translations (key text, lang text, value text);\n\
             CREATE OR REPLACE VIEW v_batches_notes_impacts AS\n\
             SELECT bn.id_batch_note, bn.batch, unnest(bn.impacts) AS impact, t.value AS impact_label\n\
             FROM batches_notes bn\n\
             LEFT JOIN translations t ON t.key = bn.impact AND t.lang = 'fr';\n",
        ),
    ]);
    assert!(notices.is_empty(), "{notices:?}");
    assert_eq!(s.dialect.as_deref(), Some("postgresql"));
    let n = table(&s, "batches_notes");
    assert_eq!(n.file, "migrations/V2__batches_notes.sql");
    assert_eq!(
        cols(n),
        [
            "id_batch_note",
            "batch",
            "impact",
            "gap_minutes",
            "created_at",
            "impacts"
        ]
    );
    let id = col(n, "id_batch_note");
    assert!(id.pk && id.ty == "uuid" && id.default.as_deref() == Some("gen_random_uuid()"));
    assert_eq!(fk(col(n, "batch")), Some(("batches", Some("id_batch"))));
    assert_eq!(
        fk(col(n, "impact")),
        Some((
            "parameters_batch_note_categories",
            Some("id_batch_note_category")
        ))
    );
    let gap = col(n, "gap_minutes");
    assert_eq!(
        (gap.ty.as_str(), gap.check.as_deref()),
        ("float4", Some("gap_minutes >= 0::double precision"))
    );
    let impacts = col(n, "impacts");
    assert_eq!((impacts.ty.as_str(), impacts.line), ("varchar[]", 1));
    let v = table(&s, "v_batches_notes_impacts");
    assert_eq!(
        (v.kind.as_str(), v.reads.clone()),
        (
            "view",
            vec!["batches_notes".to_string(), "translations".into()]
        )
    );
    assert_eq!(
        cols(v),
        ["id_batch_note", "batch", "impact", "impact_label"]
    );
    assert_eq!(col(v, "batch").ty, "uuid");
    assert_eq!(col(v, "impact_label").ty, "text");
}

#[test]
fn odd_input_never_panics() {
    for text in [
        "CREATE",
        "CREATE TABLE",
        "CREATE TABLE (",
        "ALTER TABLE x RENAME",
        "'unterminated",
        "$$ never",
        "CREATE VIEW v AS",
        "DROP TABLE",
        "CREATE TABLE é (ü int, \"x",
        "CREATE TABLE t (a int REFERENCES)",
        "\u{feff}CREATE TABLE t(a int);",
    ] {
        let _ = schema(&[("x.sql", text)]);
    }
}

#[test]
fn a_rename_onto_a_column_the_table_has_is_skipped() {
    let sql = "CREATE TABLE t (id int PRIMARY KEY, location uuid, old_location uuid);\n\
               ALTER TABLE t RENAME COLUMN location TO old_location;";
    let (s, notices) = read(&[("m/1.sql".into(), sql.into())]);
    let names: Vec<&str> = s.tables[0]
        .columns
        .iter()
        .map(|c| c.name.as_str())
        .collect();
    assert_eq!(names, ["id", "location", "old_location"]);
    assert_eq!(
        notices,
        ["m/1.sql:2: RENAME of t.location to old_location, a column it has; skipped"]
    );
}

#[test]
fn migrations_in_a_down_folder_are_down_migrations() {
    assert!(crate::sql::is_down(
        "db/migrations/down/2023.11.10T11.33.47.fix.sql"
    ));
    assert!(crate::sql::is_down("db/Rollback/V2__x.sql"));
    assert!(!crate::sql::is_down(
        "db/migrations/2023.11.10T11.33.47.fix.sql"
    ));
    assert!(!crate::sql::is_down("db/downtime/V2__x.sql"));
}
