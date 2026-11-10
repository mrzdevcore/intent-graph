use super::*;

#[test]
fn drizzle_tables_columns_and_references() {
    let (s, notices) = schema(&[
        (
            "src/db/orgs.ts",
            "import { pgTable, serial, text } from \"drizzle-orm/pg-core\";\n\
             export const timestamps = { createdAt: timestamp(\"created_at\").defaultNow().notNull() };\n\
             export const orgs = pgTable(\"orgs\", {\n  id: serial(\"id\").primaryKey(),\n  name: text(\"name\"),\n  ...timestamps,\n});\n",
        ),
        (
            "src/db/users.ts",
            "import { pgTable, integer, varchar, primaryKey } from 'drizzle-orm/pg-core';\n\
             // const ghost = pgTable(\"ghost\", {});\n\
             export const users = pgTable(\"users\", {\n  id: serial(\"id\").primaryKey(),\n  \
             email: varchar(\"email\", { length: 256 }).notNull().unique(),\n  \
             orgId: integer(\"org_id\").references(() => orgs.id, { onDelete: \"cascade\" }),\n  \
             roles: text().array().default(sql`'{}'`),\n});\n\
             export const members = pgTable(\"members\", {\n  userId: integer(\"user_id\").notNull(),\n  orgId: integer(\"org_id\").notNull(),\n}, (t) => ({\n  \
             pk: primaryKey({ columns: [t.userId, t.orgId] }),\n  \
             fk: foreignKey({ columns: [t.userId], foreignColumns: [users.id] }),\n}));\n\
             export const active = pgView(\"active_users\").as((qb) => qb.select().from(users).innerJoin(orgs, eq(users.orgId, orgs.id)));\n",
        ),
    ]);
    assert!(notices.is_empty(), "{notices:?}");
    assert_eq!(s.dialect.as_deref(), Some("postgresql"));
    assert_eq!(names(&s), ["active_users", "members", "orgs", "users"]);
    let o = table(&s, "orgs");
    assert_eq!(cols(o), ["id", "name", "created_at"]);
    assert!(col(o, "id").pk && col(o, "id").ty == "serial" && col(o, "name").nullable);
    assert_eq!(
        (
            col(o, "created_at").line,
            col(o, "created_at").default.as_deref()
        ),
        (6, Some("now()"))
    );
    let u = table(&s, "users");
    assert_eq!(
        (u.source.as_str(), u.range.start_line, u.range.end_line),
        ("drizzle", 3, 8)
    );
    assert_eq!(cols(u), ["id", "email", "org_id", "roles"]);
    let email = col(u, "email");
    assert!(!email.nullable && email.unique && email.ty == "varchar" && email.line == 5);
    assert_eq!(fk(col(u, "org_id")), Some(("orgs", Some("id"))));
    assert_eq!(col(u, "roles").ty, "text[]");
    let m = table(&s, "members");
    assert!(col(m, "user_id").pk && col(m, "org_id").pk);
    assert_eq!(fk(col(m, "user_id")), Some(("users", Some("id"))));
    let v = table(&s, "active_users");
    assert_eq!(
        (v.kind.as_str(), v.reads.clone()),
        ("view", vec!["users".to_string(), "orgs".into()])
    );
}

#[test]
fn drizzle_table_creators_and_other_dialects() {
    let (s, _) = schema(&[(
        "db.ts",
        "import { mysqlTableCreator, int } from \"drizzle-orm/mysql-core\";\n\
         const createTable = mysqlTableCreator((name) => `app_${name}`);\n\
         export const posts = createTable(\"post\", { id: int(\"id\").primaryKey().autoincrement() });\n",
    )]);
    assert_eq!(s.dialect.as_deref(), Some("mysql"));
    assert_eq!(cols(table(&s, "app_post")), ["id"]);
}

#[test]
fn typeorm_entities_relations_and_views() {
    let (s, notices) = schema(&[
        (
            "src/entity/base.ts",
            "import { PrimaryGeneratedColumn } from \"typeorm\";\nexport abstract class Base {\n  @PrimaryGeneratedColumn(\"uuid\")\n  id: string;\n\n  @CreateDateColumn({ name: \"created_at\" })\n  createdAt: Date;\n}\n",
        ),
        (
            "src/entity/user.ts",
            "import { Entity, Column, ManyToOne, JoinColumn } from \"typeorm\";\n\n\
             @Entity({ name: \"users\" })\nexport class User extends Base {\n  \
             @Column({ type: \"varchar\", length: 40, unique: true })\n  email: string;\n\n  \
             // @Column() ghost: string;\n  @Column({ nullable: true })\n  nick?: string | null;\n\n  \
             @ManyToOne(() => Org, (o) => o.users, { nullable: false })\n  @JoinColumn({ name: \"org_id\" })\n  org: Org;\n\n  \
             @OneToMany(() => Post, (p) => p.author)\n  posts: Post[];\n\n  \
             greet(): string {\n    return `hi ${this.nick}`;\n  }\n}\n",
        ),
        (
            "src/entity/org.ts",
            "@Entity()\nexport class OrgUnit {\n  @PrimaryColumn()\n  code: string;\n}\n\n\
             @Entity(\"orgs\")\nexport class Org {\n  @PrimaryGeneratedColumn()\n  id: number;\n\n  \
             @ManyToOne(() => OrgUnit)\n  unit: OrgUnit;\n\n  @Column() unitId: string;\n}\n\n\
             @ViewEntity({ name: \"org_sizes\", expression: `SELECT o.id, count(*) AS n FROM orgs o JOIN users u ON u.org_id = o.id GROUP BY o.id` })\n\
             export class OrgSize {\n  @ViewColumn() id: number;\n  @ViewColumn() n: number;\n}\n",
        ),
    ]);
    assert!(notices.is_empty(), "{notices:?}");
    assert_eq!(names(&s), ["org_sizes", "org_unit", "orgs", "users"]);
    let u = table(&s, "users");
    assert_eq!((u.source.as_str(), u.range.start_line), ("typeorm", 3));
    assert_eq!(cols(u), ["id", "created_at", "email", "nick", "org_id"]);
    assert!(col(u, "id").pk && col(u, "id").ty == "uuid");
    let email = col(u, "email");
    assert!(email.unique && !email.nullable && email.ty == "varchar(40)");
    assert!(col(u, "nick").nullable && col(u, "nick").ty == "string");
    let org = col(u, "org_id");
    assert!(!org.nullable && org.ty == "number");
    assert_eq!(fk(org), Some(("orgs", Some("id"))));
    let o = table(&s, "orgs");
    assert_eq!(cols(o), ["id", "unitId"]);
    assert_eq!(fk(col(o, "unitId")), Some(("org_unit", Some("code"))));
    let v = table(&s, "org_sizes");
    assert_eq!(
        (v.kind.as_str(), v.reads.clone()),
        ("view", vec!["orgs".to_string(), "users".into()])
    );
    assert_eq!(cols(v), ["id", "n"]);
}
