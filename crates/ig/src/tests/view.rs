//! `ig view` over the fixtures: the map it refreshes, its JSON and its page.

use super::{git_fixture, scratch};
use ig_app::map::view;
use ig_wire::Wired;

/// The files of `dir/ig/`, by name.
fn spec(dir: &std::path::Path) -> std::collections::BTreeMap<String, String> {
    std::fs::read_dir(dir.join("ig"))
        .unwrap()
        .map(|e| {
            let e = e.unwrap();
            let text = std::fs::read_to_string(e.path()).unwrap();
            (e.file_name().to_string_lossy().into_owned(), text)
        })
        .collect()
}

#[test]
fn view_json_shape() {
    let dir = git_fixture("shop");
    let w = Wired::new(&dir);
    let mut drawn = view::run(&w.env()).unwrap();
    let v = serde_json::to_value(&drawn).unwrap();
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
    // ig/ was written, and later runs leave it and the JSON byte-identical
    let first = spec(&dir);
    assert!(first.contains_key("system.kdl"), "{first:?}");
    let json = |v: view::View| serde_json::to_string_pretty(&v).unwrap();
    let once = view::run(&w.env()).unwrap();
    assert!(once.notices.is_empty(), "{:?}", once.notices);
    let once = json(once);
    assert_eq!(json(view::run(&w.env()).unwrap()), once);
    assert_eq!(spec(&dir), first);
    // the first run drew what later runs draw
    drawn.notices.retain(|n| !n.starts_with("wrote ig/"));
    assert_eq!(json(drawn), once);
    assert!(!dir.join(".ig/map.json").exists());
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
    assert_eq!(
        v.notices,
        ["wrote ig/: the intended architecture, from the code; edit it, commit it"]
    );
    let first = spec(&dir);
    let again = view::run(&w.env()).unwrap();
    assert!(again.notices.is_empty(), "{:?}", again.notices);
    assert_eq!(spec(&dir), first);
    let ids: Vec<&str> = v
        .boxes
        .iter()
        .filter(|b| b.parent.as_deref() == Some("system"))
        .map(|b| b.id.as_str())
        .collect();
    assert_eq!(ids, ["packages", "pg", "stripe"]);
    let _ = std::fs::remove_dir_all(&dir);
}
