use super::*;
use crate::map::facts;
use ig_core::testing::MemFiles;
use ig_core::workspace::SourceFile;

fn get<'a>(m: &'a MapFile, id: &str) -> &'a MapBox {
    m.boxes
        .iter()
        .find(|b| b.id == id)
        .unwrap_or_else(|| panic!("no box {id}"))
}

fn get_mut<'a>(m: &'a mut MapFile, id: &str) -> &'a mut MapBox {
    m.boxes.iter_mut().find(|b| b.id == id).unwrap()
}

/// ig's own map of the shop, with the refund slot pinned by a person to `domain`.
fn shop() -> MapFile {
    let (ws, g, cfg) = crate::map::shop::load();
    let boxes = facts(&ws, &g, &cfg);
    let mut m = refresh(None, &boxes, crate::map::arrows(&boxes, &g));
    let refund = get_mut(&mut m, "api/application-refund");
    refund.slot = Some(Field::person("domain".to_string()));
    refund.pinned = vec!["slot".into()];
    m
}

#[test]
fn shop_map_is_valid() {
    let m = shop();
    assert_eq!(validate(&m), Ok(()));
    assert!(get(&m, "api/routers").needs_reading);
    assert_eq!(get(&m, "api/routers").kind.source, Source::Ig);
}

#[test]
fn refresh_keeps_annotations() {
    let mut old = shop();
    let checkout = get_mut(&mut old, "api/application-checkout");
    checkout.description = Some(Field::agent("Takes the order.".into(), None));
    checkout.needs_reading = false;
    let mut legacy = get(&old, "api/application-checkout").clone();
    legacy.id = "api/application-legacy".into();
    legacy.made_of = vec!["apps/api/src/application/legacy/**".into()];
    legacy.title = Field::person("Legacy".into());
    legacy.pinned = vec!["title".into()];
    old.boxes.push(legacy);

    let (mut ws, g, cfg) = crate::map::shop::load();
    ws.files.push(SourceFile {
        path: "apps/api/src/application/billing/pay.ts".into(),
        package: 0,
    });
    let boxes = facts(&ws, &g, &cfg);
    let new = refresh(Some(&old), &boxes, crate::map::arrows(&boxes, &g));

    let checkout = get(&new, "api/application-checkout");
    assert_eq!(
        checkout.description.as_ref().map(|f| f.value.as_str()),
        Some("Takes the order.")
    );
    assert!(!checkout.needs_reading);
    let refund = get(&new, "api/application-refund");
    assert_eq!(refund.slot.as_ref().unwrap().value, "domain");
    assert!(get(&new, "api/application-legacy").stale);
    assert!(get(&new, "api/application-billing").needs_reading);
    assert!(!get(&new, "api/application-checkout").stale);
    let ids: Vec<&str> = new.boxes.iter().map(|b| b.id.as_str()).collect();
    let mut sorted = ids.clone();
    sorted.sort();
    assert_eq!(ids, sorted);
}

#[test]
fn refresh_drops_gone_boxes_without_pins() {
    let mut old = shop();
    let mut gone = get(&old, "api/routers").clone();
    gone.id = "api/gone".into();
    old.boxes.push(gone);
    let (ws, g, cfg) = crate::map::shop::load();
    let boxes = facts(&ws, &g, &cfg);
    let new = refresh(Some(&old), &boxes, vec![]);
    assert!(new.boxes.iter().all(|b| b.id != "api/gone"));
}

#[test]
fn annotate_respects_pins() {
    let mut m = shop();
    let n = annotate(
        &mut m,
        r#"[{"id": "api/application-refund", "slot": "application", "description": "Refunds an order."}]"#,
    )
    .unwrap();
    assert_eq!(n, 1);
    let refund = get(&m, "api/application-refund");
    assert_eq!(refund.slot.as_ref().unwrap().value, "domain");
    assert_eq!(refund.slot.as_ref().unwrap().source, Source::Person);
    assert_eq!(refund.description.as_ref().unwrap().source, Source::Agent);
    assert!(!refund.needs_reading);
}

#[test]
fn annotate_keeps_confidence() {
    let mut m = shop();
    annotate(
        &mut m,
        r#"[{"id": "api/routers", "tech": "tRPC", "confidence": "ambiguous"},
            {"id": "api/domain-order", "title": "Order"}]"#,
    )
    .unwrap();
    let tech = get(&m, "api/routers").tech.clone().unwrap();
    assert_eq!(tech.confidence, Some(Confidence::Ambiguous));
    let title = &get(&m, "api/domain-order").title;
    assert_eq!(title.source, Source::Agent);
    assert_eq!(title.confidence, Some(Confidence::Inferred));
    assert_eq!(get(&m, "api").title.confidence, None);
}

#[test]
fn annotate_regroups_components() {
    let mut m = shop();
    let comps: Vec<String> = m
        .boxes
        .iter()
        .filter(|b| b.parent.as_deref() == Some("api"))
        .flat_map(|b| b.made_of.clone())
        .collect();
    let json = serde_json::json!([{"id": "api/everything", "madeOf": comps, "kind": "group"}]);
    let n = annotate(&mut m, &json.to_string());
    // the pinned refund box cannot be regrouped away
    assert!(n.unwrap_err().iter().any(|e| e.contains("pinned")));
    get_mut(&mut m, "api/application-refund").pinned.clear();
    annotate(&mut m, &json.to_string()).unwrap();
    let kids: Vec<&str> = m
        .boxes
        .iter()
        .filter(|b| b.parent.as_deref() == Some("api"))
        .map(|b| b.id.as_str())
        .collect();
    assert_eq!(kids, ["api/everything"]);
    assert!(get(&m, "api/everything").files > 0);
    // one file in two components
    let twice = serde_json::json!([
        {"id": "api/a", "madeOf": ["apps/api/src/**"]},
        {"id": "api/b", "madeOf": ["apps/api/src/domain/**"]}
    ]);
    let e = annotate(&mut m, &twice.to_string()).unwrap_err();
    assert!(e.iter().any(|e| e.contains("2 components")), "{e:?}");
}

#[test]
fn rejects_hostile_annotations() {
    let long = "x".repeat(201);
    let many: Vec<serde_json::Value> = (0..13)
        .map(|i| serde_json::json!({"id": format!("api/g{i}"), "madeOf": [format!("apps/api/src/none{i}/**")]}))
        .collect();
    let cases = [
        (
            r#"[{"id": "api/routers", "madeOf": ["../etc/**"]}]"#.to_string(),
            "..",
        ),
        (r#"[{"id": "Bad Id", "title": "x"}]"#.to_string(), "Bad Id"),
        (
            format!(r#"[{{"id": "api/routers", "description": "{long}"}}]"#),
            "description",
        ),
        (
            r#"[{"id": "api/routers", "kind": "spaceship"}]"#.to_string(),
            "spaceship",
        ),
        (
            r#"[{"id": "api/routers", "color": "red"}]"#.to_string(),
            "color",
        ),
        (serde_json::Value::from(many).to_string(), "13 children"),
    ];
    let mut m = shop();
    get_mut(&mut m, "api/application-refund").pinned.clear();
    for (json, want) in cases {
        let before = m.clone();
        let e = annotate(&mut m, &json).unwrap_err();
        assert!(e.iter().any(|e| e.contains(want)), "{want}: {e:?}");
        assert_eq!(m, before);
    }
}

#[test]
fn validate_checks_slots_and_globs() {
    let mut m = shop();
    get_mut(&mut m, "api/routers").slot = Some(Field::person("ui".into()));
    get_mut(&mut m, "api/domain-order").made_of = vec!["/etc/**".into()];
    let e = validate(&m).unwrap_err();
    assert!(e.iter().any(|e| e.contains("ui")), "{e:?}");
    assert!(e.iter().any(|e| e.contains("/etc/**")), "{e:?}");
}

#[test]
fn save_is_stable() {
    let files = MemFiles::default();
    let mut m = shop();
    m.boxes.reverse();
    save(&files, &m).unwrap();
    let one = files.read_bytes(MAP).unwrap();
    save(&files, &m).unwrap();
    assert_eq!(files.read_bytes(MAP).unwrap(), one);
    let (back, notices) = load(&files);
    assert!(notices.is_empty(), "{notices:?}");
    let back = back.unwrap();
    let ids: Vec<&str> = back.boxes.iter().map(|b| b.id.as_str()).collect();
    let mut sorted = ids.clone();
    sorted.sort();
    assert_eq!(ids, sorted);
    let text = String::from_utf8(one).unwrap();
    assert!(text.ends_with("}\n"));
    // keys sorted: "files" before "id" in each box
    assert!(text.find("\"files\"").unwrap() < text.find("\"id\": \"api\"").unwrap());
}

#[test]
fn load_rejects_bad_files() {
    let files = MemFiles::with(&[(MAP, "{not json")]);
    let (m, notices) = load(&files);
    assert!(m.is_none());
    assert_eq!(notices.len(), 1);
    let files = MemFiles::with(&[(
        MAP,
        r#"{"version": 1, "boxes": [], "arrows": [], "marks": [], "extra": 1}"#,
    )]);
    assert!(load(&files).0.is_none());
    let big = format!("{{\"pad\": \"{}\"}}", "x".repeat(2 * 1024 * 1024));
    let files = MemFiles::with(&[(MAP, big.as_str())]);
    let (m, notices) = load(&files);
    assert!(m.is_none() && notices[0].contains("2 MB"), "{notices:?}");
    assert_eq!(load(&MemFiles::default()), (None, vec![]));
}
