use super::*;
use ig_core::ports::SchemaReader;

const PRISMA: &str = "datasource db {\n  provider = \"mysql\"\n}\nmodel Account {\n  id Int @id\n  @@map(\"accounts\")\n}\n";

#[test]
fn an_orm_declaration_wins_over_sql_and_sql_fills_the_rest() {
    let (s, notices) = schema(&[
        (
            "db/V1__init.sql",
            "CREATE TABLE ACCOUNTS (id int PRIMARY KEY, legacy text);\nCREATE TABLE audit (id serial, account int REFERENCES accounts);",
        ),
        ("prisma/schema.prisma", PRISMA),
    ]);
    assert!(notices.is_empty(), "{notices:?}");
    assert_eq!(s.dialect.as_deref(), Some("mysql"));
    assert_eq!(names(&s), ["accounts", "audit"]);
    assert_eq!(table(&s, "accounts").source, "prisma");
    let account = col(table(&s, "audit"), "account");
    assert_eq!(fk(account), Some(("accounts", Some("id"))));
}

#[test]
fn two_orms_that_disagree_leave_a_notice() {
    let (s, notices) = schema(&[
        ("prisma/schema.prisma", PRISMA),
        (
            "app/models.py",
            "class A(Base):\n    __tablename__ = \"accounts\"\n    id = Column(Integer, primary_key=True)\n    extra = Column(String)\n",
        ),
        (
            "app/same.py",
            "class B(Base):\n    __tablename__ = \"Accounts\"\n    id = Column(Integer, primary_key=True)\n",
        ),
    ]);
    assert_eq!(table(&s, "accounts").source, "prisma");
    assert_eq!(
        notices,
        [
            "app/models.py:1: table accounts is declared differently by prisma in prisma/schema.prisma; kept that one"
        ]
    );
}

#[test]
fn a_key_left_to_the_primary_key_takes_its_column_and_type() {
    let (s, _) = schema(&[(
        "app/models.py",
        "from django.db import models\nclass Org(models.Model):\n    code = models.CharField(primary_key=True, max_length=4)\n\
         class Team(models.Model):\n    org = models.ForeignKey(Org, on_delete=models.CASCADE)\n",
    )]);
    let org = col(table(&s, "app_team"), "org_id");
    assert_eq!(
        (fk(org), org.ty.as_str()),
        (Some(("app_org", Some("code"))), "ForeignKey")
    );
}

#[test]
fn the_port_reads_like_the_functions() {
    let files = vec![("a.sql".to_string(), "CREATE TABLE t (id int);".to_string())];
    assert_eq!(SchemaFiles.read(&files), read(&files));
    assert!(SchemaFiles.wants("db/V1__x.SQL") && SchemaFiles.wants("schema.prisma"));
    assert!(!SchemaFiles.wants("types.d.ts") && !SchemaFiles.wants("README.md"));
}
