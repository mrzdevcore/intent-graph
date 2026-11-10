use super::*;

mod js;

#[test]
fn django_models_tables_keys_and_inheritance() {
    let (s, notices) = schema(&[
        (
            "shop/models.py",
            "from django.conf import settings\nfrom django.db import models\n\n\
             class Stamped(models.Model):\n    created = models.DateTimeField(auto_now_add=True)\n\n    class Meta:\n        abstract = True\n\n\
             class Customer(Stamped):\n    \"\"\"A buyer. name = models.CharField()\"\"\"\n    name = models.CharField(max_length=40, unique=True)\n    \
             user = models.OneToOneField(settings.AUTH_USER_MODEL, on_delete=models.CASCADE)\n\n\
             class Order(models.Model):\n    code = models.CharField(\n        max_length=10,\n        primary_key=True,\n    )\n    \
             customer = models.ForeignKey(Customer, on_delete=models.CASCADE, null=True)\n    \
             parent = models.ForeignKey('self', null=True, on_delete=models.SET_NULL, db_column='parent_code')\n    \
             item = models.ForeignKey('catalog.Item', on_delete=models.PROTECT)\n    \
             tags = models.ManyToManyField('catalog.Tag')\n\n    class Meta:\n        db_table = 'orders'\n\n\
             class Rush(Order):\n    eta = models.DateField()\n\n\
             class Cheap(Order):\n    class Meta:\n        proxy = True\n",
        ),
        (
            "catalog/models/item.py",
            "from django.db import models\n\nclass Item(models.Model):\n    sku = models.CharField(max_length=8)\n",
        ),
        (
            "shop/migrations/0001_initial.py",
            "from django.db import migrations, models\nclass Migration(migrations.Migration):\n    pass\n",
        ),
    ]);
    assert!(notices.is_empty(), "{notices:?}");
    assert_eq!(
        names(&s),
        ["catalog_item", "orders", "shop_customer", "shop_rush"]
    );
    let c = table(&s, "shop_customer");
    assert_eq!(c.source, "django");
    assert_eq!(cols(c), ["id", "created", "name", "user_id"]);
    assert!(col(c, "id").pk && col(c, "id").ty == "AutoField");
    assert!(col(c, "name").unique && !col(c, "name").nullable && col(c, "name").ty == "CharField");
    assert_eq!(fk(col(c, "user_id")), Some(("auth_user", None)));
    let o = table(&s, "orders");
    assert_eq!(cols(o), ["code", "customer_id", "parent_code", "item_id"]);
    assert!(col(o, "code").pk);
    assert!(col(o, "customer_id").nullable);
    assert_eq!(
        fk(col(o, "customer_id")),
        Some(("shop_customer", Some("id")))
    );
    assert_eq!(col(o, "customer_id").ty, "ForeignKey");
    assert_eq!(fk(col(o, "parent_code")), Some(("orders", Some("code"))));
    assert_eq!(fk(col(o, "item_id")), Some(("catalog_item", Some("id"))));
    let r = table(&s, "shop_rush");
    assert_eq!(cols(r), ["order_ptr_id", "eta"]);
    assert_eq!(fk(col(r, "order_ptr_id")), Some(("orders", Some("code"))));
}

#[test]
fn sqlalchemy_declarative_mapped_and_core_tables() {
    let (s, notices) = schema(&[(
        "app/db.py",
        "from typing import Optional\nimport sqlalchemy as sa\nfrom sqlalchemy.orm import Mapped, mapped_column, relationship\n\n\
         class Stamp:\n    created_at = sa.Column(sa.DateTime, server_default=sa.func.now())\n\n\
         class Org(Base, Stamp):\n    __tablename__ = \"orgs\"\n    id = sa.Column(sa.Integer, primary_key=True)\n    \
         name = sa.Column(\"org_name\", sa.String(80), nullable=False, unique=True)\n\n\
         class User(Base):\n    __tablename__ = 'users'\n    id: Mapped[int] = mapped_column(primary_key=True)\n    \
         email: Mapped[str] = mapped_column(sa.String(255), unique=True)\n    nick: Mapped[Optional[str]]\n    \
         bio: Mapped[str | None] = mapped_column(sa.Text)\n    org_id: Mapped[int] = mapped_column(sa.ForeignKey(\"orgs.id\"))\n    \
         boss_id = mapped_column(sa.ForeignKey(Org.id), nullable=True)\n    org: Mapped[Org] = relationship(back_populates=\"users\")\n\n\
         tags = sa.Table(\n    \"user_tags\",\n    metadata,\n    sa.Column(\"user_id\", sa.ForeignKey(\"users.id\"), primary_key=True),\n    sa.Column(\"tag\", sa.String(20)),\n)\n",
    )]);
    assert!(notices.is_empty(), "{notices:?}");
    assert_eq!(names(&s), ["orgs", "user_tags", "users"]);
    let o = table(&s, "orgs");
    assert_eq!(cols(o), ["created_at", "id", "org_name"]);
    assert_eq!(
        col(o, "created_at").default.as_deref(),
        Some("sa.func.now()")
    );
    let name = col(o, "org_name");
    assert!(name.unique && !name.nullable && name.ty == "String(80)");
    let u = table(&s, "users");
    assert_eq!(cols(u), ["id", "email", "nick", "bio", "org_id", "boss_id"]);
    assert!(col(u, "id").pk && col(u, "id").ty == "int");
    assert!(!col(u, "email").nullable && col(u, "email").ty == "String(255)");
    assert!(col(u, "nick").nullable && col(u, "nick").ty == "str");
    assert!(col(u, "bio").nullable && col(u, "bio").ty == "Text");
    assert_eq!(fk(col(u, "org_id")), Some(("orgs", Some("id"))));
    assert_eq!(fk(col(u, "boss_id")), Some(("orgs", Some("id"))));
    assert_eq!(col(u, "boss_id").ty, "Integer");
    let t = table(&s, "user_tags");
    assert_eq!((t.range.start_line, t.range.end_line), (23, 28));
    assert!(col(t, "user_id").pk);
    assert_eq!(fk(col(t, "user_id")), Some(("users", Some("id"))));
}

#[test]
fn plain_code_has_no_tables() {
    let (s, notices) = schema(&[
        (
            "app/util.py",
            "class Table:\n    pass\n\ndef make(x):\n    return Column(x)\n",
        ),
        (
            "web/src/app.ts",
            "export const table = (x: string) => x;\nclass Entity {}\n",
        ),
        ("web/src/lib.js", "const users = [];\n"),
        ("README.md", "CREATE TABLE x (id int);"),
    ]);
    assert!(
        s.tables.is_empty() && s.dialect.is_none() && notices.is_empty(),
        "{s:?} {notices:?}"
    );
}

#[test]
fn odd_model_input_never_panics() {
    let odd = [
        (
            "a/models.py",
            "from django.db import models\nclass A(models.Model):\n    x = models.ForeignKey(\n",
        ),
        (
            "a/models.py",
            "from django.db import models\nclass A(A):\n    pass\nclass B(models.Model):\n    \"\"\"é\n",
        ),
        (
            "b.py",
            "__tablename__ = Table(\nclass X:\n    __tablename__ = 'x'\n    y: Mapped[\n",
        ),
        (
            "c.ts",
            "import 'drizzle-orm'; const t = pgTable(\"t\", { a: int(\"a\").references(() => ",
        ),
        (
            "d.ts",
            "import 'typeorm'; @Entity( class X { @Column({ name: } é: string",
        ),
        (
            "e.prisma",
            "model X {\n  a Int @relation(fields: [\n  b X @relation(fields: [a], references: [\n",
        ),
    ];
    for (path, text) in odd {
        let _ = schema(&[(path, text)]);
    }
}
