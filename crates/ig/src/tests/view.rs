//! The viewer data and page over the fixtures.

use super::{env, scratch};
use ig_app::view_data;
use ig_wire::Wired;
use serde_json::json;

#[test]
fn system_nodes_carry_category_and_packages() {
    let dir = scratch("systems");
    let mut cfg = std::fs::read_to_string(dir.join("ig.yaml")).unwrap();
    cfg.push_str("blueprint:\n  systems:\n    PostgreSQL: [pg]\n");
    std::fs::write(dir.join("ig.yaml"), cfg).unwrap();
    std::fs::write(
        dir.join("packages/core/src/db.ts"),
        "import { Pool } from \"pg\";\nexport const pool = new Pool();\n",
    )
    .unwrap();
    let d = view_data::json(&env(&dir)).unwrap();
    let n = d["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["id"] == "system:PostgreSQL")
        .unwrap();
    assert_eq!(
        (n["category"].clone(), n["packages"].clone()),
        (json!("database"), json!(["pg"]))
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn view_writes_the_page_and_the_json_names_the_layers() {
    let dir = scratch("page");
    let path = crate::render::write(&Wired::new(&dir)).unwrap();
    assert!(
        std::fs::read_to_string(path)
            .unwrap()
            .contains("packages/core")
    );
    let d = view_data::json(&env(&dir)).unwrap();
    assert!(!d["layers"].as_array().unwrap().is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}
