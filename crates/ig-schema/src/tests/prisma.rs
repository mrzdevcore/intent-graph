use super::*;

const SCHEMA: &str = r#"datasource db {
  provider = "postgresql" // the main one
  url      = env("DATABASE_URL")
}

/// A person.
model User {
  id        Int      @id @default(autoincrement())
  email     String   @unique @db.VarChar(40)
  name      String?  @map("full_name")
  tags      String[]
  posts     Post[]
  createdAt DateTime @default(now()) @map("created_at")

  @@map("users")
}

model Post {
  id       Int    @id
  authorId Int    @map("author_id")
  author   User   @relation(fields: [authorId], references: [id], onDelete: Cascade)
  status   Status @default(DRAFT)
}

model Tagging {
  postId Int
  tag    String
  @@id([postId, tag])
  @@unique([tag])
}

view PostCount {
  authorId Int @unique
  n        Int
}

enum Status {
  DRAFT
  LIVE
}
"#;

#[test]
fn prisma_models_relations_and_maps() {
    let (s, notices) = schema(&[("prisma/schema.prisma", SCHEMA)]);
    assert!(notices.is_empty(), "{notices:?}");
    assert_eq!(s.dialect.as_deref(), Some("postgresql"));
    assert_eq!(names(&s), ["Post", "PostCount", "Tagging", "users"]);
    let u = table(&s, "users");
    assert_eq!(
        (u.source.as_str(), u.range.start_line, u.range.end_line),
        ("prisma", 7, 16)
    );
    assert_eq!(cols(u), ["id", "email", "full_name", "tags", "created_at"]);
    let id = col(u, "id");
    assert!(id.pk && id.ty == "Int" && id.default.as_deref() == Some("autoincrement()"));
    assert!(col(u, "email").unique && !col(u, "email").nullable);
    assert!(col(u, "full_name").nullable);
    assert_eq!(col(u, "tags").ty, "String[]");
    let p = table(&s, "Post");
    assert_eq!(cols(p), ["id", "author_id", "status"]);
    assert_eq!(fk(col(p, "author_id")), Some(("users", Some("id"))));
    assert_eq!(col(p, "status").default.as_deref(), Some("DRAFT"));
    let t = table(&s, "Tagging");
    assert!(col(t, "postId").pk && col(t, "tag").pk && col(t, "tag").unique);
    assert_eq!(table(&s, "PostCount").kind, "view");
}

#[test]
fn a_prisma_project_skips_its_generated_migrations() {
    let (s, _) = schema(&[
        ("prisma/schema.prisma", SCHEMA),
        (
            "prisma/migrations/20240101_init/migration.sql",
            "CREATE TABLE \"users\" (\"id\" SERIAL, \"legacy\" text);",
        ),
        ("db/extra.sql", "CREATE TABLE audit (id int);"),
    ]);
    assert_eq!(
        cols(table(&s, "users")),
        ["id", "email", "full_name", "tags", "created_at"]
    );
    assert_eq!(table(&s, "users").source, "prisma");
    assert_eq!(table(&s, "audit").source, "sql");
    let (alone, _) = schema(&[(
        "prisma/migrations/1_init/migration.sql",
        "CREATE TABLE a (id int);",
    )]);
    assert_eq!(names(&alone), ["a"]);
}
