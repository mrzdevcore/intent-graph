use super::*;

#[test]
fn a_card_has_a_row_per_key_one_for_the_other_columns_and_one_for_its_translations() {
    let t = tables(
        &db(),
        &schema(vec![
            plain_table("tags", vec![pk("id")]),
            plain_table(
                "products",
                vec![pk("id"), fk("tag", "tags"), plain("price")],
            ),
            plain_table(
                "products_translations",
                vec![pk("id"), fk("product", "products"), plain("name")],
            ),
        ]),
    );
    assert_eq!(t.tables["db#tags"].card_rows(), 1);
    assert_eq!(t.tables["db#products"].card_rows(), 2 + 1 + 1);
    let v = serde_json::to_value(&t.tables["db#products"]).unwrap();
    assert_eq!(v["nullability"], true);
}

#[test]
fn a_domain_box_takes_the_shape_of_its_tables_so_opened_they_fill_it() {
    let ts = (0..8)
        .map(|i| {
            plain_table(
                &format!("shop_t{i}"),
                vec![pk("id"), plain("a"), plain("b")],
            )
        })
        .chain([plain_table("misc_a", vec![pk("id")])])
        .collect();
    let (_, placed) = laid_out(ts);
    let (f, (iw, ih)) = (&placed.frames["db#shop_*"], placed.inner["db#shop_*"]);
    assert!(((f.h - 30.0) / f.w - ih / iw).abs() < 0.01);
    assert!(f.w >= 248.0 && f.h >= 152.0);
}
