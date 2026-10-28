//! `ig view` over the fixtures: the map it refreshes, its JSON and its page.

use super::{git_fixture, scratch};
use ig_app::map::view;
use ig_wire::Wired;

#[test]
fn view_json_shape() {
    let dir = git_fixture("shop");
    let w = Wired::new(&dir);
    let v = serde_json::to_value(view::run(&w.env()).unwrap()).unwrap();
    for key in ["boxes", "arrows", "marks", "placed", "notices"] {
        assert!(v.get(key).is_some(), "no `{key}` in {v}");
    }
    assert!(v.get("nodes").is_none());
    assert!(v["placed"]["frames"].is_object());
    let ids = |k: &str| -> Vec<String> {
        v[k].as_array()
            .unwrap()
            .iter()
            .map(|x| x["id"].as_str().unwrap().to_string())
            .collect()
    };
    assert!(ids("boxes").contains(&"api/domain-order".to_string()));
    // the code level: graph node ids
    assert!(ids("boxes").contains(&"apps/api/src/domain/order/order.ts#loadOrder".to_string()));
    assert!(ids("arrows").contains(&"api/domain-order->api/infra-prisma".to_string()));
    assert!(
        v["marks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["badge"] == "breaks: domain → infrastructure")
    );
    // the map was written, and a second run leaves it byte-identical
    let first = std::fs::read(dir.join(".ig/map.json")).unwrap();
    view::run(&w.env()).unwrap();
    assert_eq!(std::fs::read(dir.join(".ig/map.json")).unwrap(), first);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn view_writes_the_page() {
    let dir = scratch("page");
    let w = Wired::new(&dir);
    let path = crate::render::write(&w, &view::run(&w.env()).unwrap()).unwrap();
    assert!(
        std::fs::read_to_string(path)
            .unwrap()
            .contains("packages/core")
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// 13 packages (one named `system`) and 2 outside systems: the map is valid, accepted on the
/// next run and stable.
#[test]
fn many_containers_stay_loadable() {
    let mut files: Vec<(String, String)> = vec![
        (
            "package.json".into(),
            r#"{"name":"mono","private":true,"workspaces":["packages/*"]}"#.into(),
        ),
        (
            "ig.yaml".into(),
            "blueprint: { systems: { Stripe: [stripe], Pg: [pg] } }\n".into(),
        ),
    ];
    for i in 1..=13 {
        let name = if i == 1 {
            "@m/system".into()
        } else {
            format!("@m/p{i}")
        };
        files.push((
            format!("packages/p{i}/package.json"),
            format!(r#"{{"name":"{name}","version":"1.0.0"}}"#),
        ));
        files.push((
            format!("packages/p{i}/src/a/x.ts"),
            "export const x = 1;\n".into(),
        ));
    }
    let refs: Vec<(&str, &str)> = files
        .iter()
        .map(|(a, b)| (a.as_str(), b.as_str()))
        .collect();
    let dir = super::tmp_with(&refs);
    let w = Wired::new(&dir);
    let v = view::run(&w.env()).unwrap();
    assert!(v.notices.is_empty(), "{:?}", v.notices);
    let first = std::fs::read(dir.join(".ig/map.json")).unwrap();
    let again = view::run(&w.env()).unwrap();
    assert!(again.notices.is_empty(), "{:?}", again.notices);
    assert_eq!(std::fs::read(dir.join(".ig/map.json")).unwrap(), first);
    let ids: Vec<&str> = v
        .boxes
        .iter()
        .filter(|b| b.parent.as_deref() == Some("system"))
        .map(|b| b.id.as_str())
        .collect();
    assert_eq!(ids, ["packages", "pg", "stripe"]);
    let _ = std::fs::remove_dir_all(&dir);
}
