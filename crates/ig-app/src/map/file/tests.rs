use super::*;
use crate::map::{FactBox, facts};
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

/// `refresh`, checking the invariant every refresh keeps: its map is valid (else the next
/// `load` would drop the whole file, pins and all).
fn refreshed(old: Option<&MapFile>, facts: &[FactBox], arrows: Vec<MapArrow>) -> MapFile {
    let m = refresh(old, facts, arrows, &[]).0;
    assert_eq!(validate(&m), Ok(()));
    m
}

/// `path` added to the api package, as a source file and a module node.
fn add(ws: &mut ig_core::workspace::WorkspaceInfo, g: &mut ig_core::model::Graph, path: &str) {
    ws.files.push(SourceFile {
        path: path.into(),
        package: 0,
    });
    let mut n = g.nodes.iter().find(|n| n.is_module()).unwrap().clone();
    n.id = path.into();
    n.file = path.into();
    g.nodes.push(n);
}

/// The shop with 12 more api folders (so api has 12 components), and its map after the agent
/// took those 12 components as they are.
fn twelve() -> (
    ig_core::workspace::WorkspaceInfo,
    ig_core::model::Graph,
    ig_core::config::Config,
    MapFile,
) {
    let (mut ws, mut g, cfg) = crate::map::shop::load();
    for i in 0..12 {
        add(&mut ws, &mut g, &format!("apps/api/src/extra{i}/x.ts"));
    }
    let boxes = facts(&ws, &g, &cfg);
    let mut m = refreshed(None, &boxes, vec![]);
    let notes: Vec<serde_json::Value> = m
        .boxes
        .iter()
        .filter(|b| b.parent.as_deref() == Some("api"))
        .map(|b| serde_json::json!({"id": b.id, "madeOf": b.made_of}))
        .collect();
    assert_eq!(notes.len(), 12);
    annotate(&mut m, &serde_json::Value::from(notes).to_string()).unwrap();
    (ws, g, cfg, m)
}

/// ig's own map of the shop, with the refund slot pinned by a person to `domain`.
fn shop() -> MapFile {
    let (ws, g, cfg) = crate::map::shop::load();
    let boxes = facts(&ws, &g, &cfg);
    let mut m = refreshed(None, &boxes, crate::map::arrows(&boxes, &g));
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
    let new = refreshed(Some(&old), &boxes, crate::map::arrows(&boxes, &g));

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
    let new = refreshed(Some(&old), &boxes, vec![]);
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
fn only_the_spec_lets_a_component_keep_another_containers_prefix() {
    let mut m = shop();
    let web = m.boxes.iter().find(|b| b.id == "web").unwrap().id.clone();
    get_mut(&mut m, "api/routers").parent = Some(web);
    get_mut(&mut m, "api/routers").slot = None; // web has no pattern
    let e = validate(&m).unwrap_err();
    assert!(
        e.iter().any(|e| e.contains("`api/routers` is not a slug")),
        "{e:?}"
    );
    assert_eq!(validate_spec(&m), Ok(()));
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

#[test]
fn refresh_keeps_agent_grouping() {
    let mut m = shop();
    get_mut(&mut m, "api/application-refund").pinned.clear();
    let kids: Vec<MapBox> = m
        .boxes
        .iter()
        .filter(|b| b.parent.as_deref() == Some("api"))
        .cloned()
        .collect();
    let merged = ["api/application-checkout", "api/application-refund"];
    let mut notes: Vec<serde_json::Value> = kids
        .iter()
        .filter(|b| !merged.contains(&b.id.as_str()))
        .map(|b| serde_json::json!({"id": b.id, "madeOf": b.made_of}))
        .collect();
    let orders: Vec<String> = kids
        .iter()
        .filter(|b| merged.contains(&b.id.as_str()))
        .flat_map(|b| b.made_of.clone())
        .collect();
    let want: Vec<String> = kids
        .iter()
        .filter(|b| merged.contains(&b.id.as_str()))
        .flat_map(|b| b.nodes.clone())
        .collect();
    assert!(!want.is_empty());
    notes.push(serde_json::json!({"id": "api/orders", "madeOf": orders, "title": "Orders"}));
    annotate(&mut m, &serde_json::Value::from(notes).to_string()).unwrap();
    // through the file, as the next run sees it
    let files = MemFiles::default();
    save(&files, &m).unwrap();
    let old = load(&files).0.unwrap();

    let (mut ws, mut g, cfg) = crate::map::shop::load();
    let billing = "apps/api/src/application/billing/pay.ts";
    ws.files.push(SourceFile {
        path: billing.into(),
        package: 0,
    });
    let mut module = g.nodes.iter().find(|n| n.is_module()).unwrap().clone();
    module.id = billing.into();
    module.file = billing.into();
    g.nodes.push(module);
    let boxes = facts(&ws, &g, &cfg);
    let new = refreshed(Some(&old), &boxes, crate::map::arrows(&boxes, &g));

    let orders = get(&new, "api/orders");
    assert_eq!(orders.made_of_source, Source::Agent);
    assert_eq!(orders.title.value, "Orders");
    let mut got = orders.nodes.clone();
    got.sort();
    let mut want = want;
    want.sort();
    assert_eq!(got, want);
    let unread = get(&new, "api/unread");
    assert_eq!(unread.nodes, [billing]);
    assert!(unread.needs_reading);
    assert!(new.boxes.iter().all(|b| b.id != "api/application-billing"));
    // each api file in exactly one component, and the globs say so too
    let comps: Vec<&MapBox> = new
        .boxes
        .iter()
        .filter(|b| b.parent.as_deref() == Some("api"))
        .collect();
    let api_files: Vec<&String> = boxes
        .iter()
        .filter(|b| b.parent.as_deref() == Some("api"))
        .flat_map(|b| &b.nodes)
        .collect();
    for f in api_files {
        let in_nodes = comps.iter().filter(|b| b.nodes.contains(f)).count();
        let in_globs = comps
            .iter()
            .filter(|b| globs(&b.made_of).unwrap().is_match(f))
            .count();
        assert_eq!((in_nodes, in_globs), (1, 1), "{f}");
    }
    assert_eq!(validate(&new), Ok(()));
}

#[test]
fn unread_never_breaks_validation() {
    let (mut ws, mut g, cfg, m) = twelve();
    add(&mut ws, &mut g, "apps/api/src/brandnew/y.ts");
    let boxes = facts(&ws, &g, &cfg);
    let new = refreshed(Some(&m), &boxes, vec![]);
    assert!(get(&new, "api/unread").needs_reading);
    let kids = new
        .boxes
        .iter()
        .filter(|b| b.parent.as_deref() == Some("api"))
        .count();
    assert_eq!(kids, 13);
    let files = MemFiles::default();
    save(&files, &new).unwrap();
    let (back, notices) = load(&files);
    assert!(notices.is_empty(), "{notices:?}");
    assert_eq!(back.unwrap().boxes.len(), new.boxes.len());
    // a 13th box of another kind is not an error either: the limit is a warning
    let mut more = new.clone();
    get_mut(&mut more, "api/unread").kind = Field::ig(BoxKind::Usecase);
    assert_eq!(validate(&more), Ok(()));
}

#[test]
fn stale_pinned_box_is_not_a_thirteenth_child() {
    let (ws, g, cfg, mut m) = twelve();
    let mut legacy = get(&m, "api/routers").clone();
    legacy.id = "api/legacy".into();
    legacy.made_of = vec!["apps/api/src/legacy/**".into()];
    legacy.pinned = vec!["title".into()];
    m.boxes.push(legacy);
    // also with ig's own grouping of api
    for b in m.boxes.iter_mut() {
        b.made_of_source = Source::Ig;
    }
    let boxes = facts(&ws, &g, &cfg);
    let new = refreshed(Some(&m), &boxes, vec![]);
    assert!(get(&new, "api/legacy").stale);
}

#[test]
fn pinned_slot_gives_way_when_the_pattern_no_longer_has_it() {
    let mut old = shop();
    get_mut(&mut old, "api").pattern = Some(Field::agent(Pattern::Hexagonal, None));
    let (ws, g, cfg) = crate::map::shop::load();
    let boxes = facts(&ws, &g, &cfg);
    let new = refreshed(Some(&old), &boxes, vec![]);
    let refund = get(&new, "api/application-refund");
    assert_eq!(refund.slot, None);
    assert!(!refund.pinned.contains(&"slot".to_string()));
    // a pin the pattern still allows is kept (onion: refund pinned to domain)
    let new = refreshed(Some(&shop()), &boxes, vec![]);
    assert_eq!(
        get(&new, "api/application-refund")
            .slot
            .as_ref()
            .unwrap()
            .value,
        "domain"
    );
}

#[test]
fn stale_component_outlives_its_container() {
    let mut old = shop();
    let mut gone = get(&old, "api").clone();
    gone.id = "gone".into();
    old.boxes.push(gone);
    let mut kid = get(&old, "api/application-refund").clone();
    kid.id = "gone/x".into();
    kid.parent = Some("gone".into());
    old.boxes.push(kid);
    let (ws, g, cfg) = crate::map::shop::load();
    let boxes = facts(&ws, &g, &cfg);
    let new = refreshed(Some(&old), &boxes, vec![]);
    // its container is kept too (stale), so every box still has its parent
    assert!(get(&new, "gone").stale);
    assert!(get(&new, "gone/x").stale);
}

#[test]
fn pinned_unread_takes_new_leftovers() {
    let (mut ws, mut g, cfg, m) = twelve();
    add(&mut ws, &mut g, "apps/api/src/brandnew/y.ts");
    let boxes = facts(&ws, &g, &cfg);
    let mut m = refreshed(Some(&m), &boxes, vec![]);
    let unread = get_mut(&mut m, "api/unread");
    unread.title = Field::person("To sort".into());
    unread.pinned = vec!["title".into()];
    add(&mut ws, &mut g, "apps/api/src/brandnew2/z.ts");
    let boxes = facts(&ws, &g, &cfg);
    let new = refreshed(Some(&m), &boxes, vec![]);
    let unread: Vec<&MapBox> = new.boxes.iter().filter(|b| b.id == "api/unread").collect();
    assert_eq!(unread.len(), 1);
    assert_eq!(unread[0].title.value, "To sort");
    assert_eq!(unread[0].files, 2);
    assert!(unread[0].needs_reading);
}

#[test]
fn unread_cannot_be_annotated() {
    let (mut ws, mut g, cfg, m) = twelve();
    add(&mut ws, &mut g, "apps/api/src/brandnew/y.ts");
    let boxes = facts(&ws, &g, &cfg);
    let mut m = refreshed(Some(&m), &boxes, vec![]);
    let before = m.clone();
    let e = annotate(&mut m, r#"[{"id": "api/unread", "title": "Misc"}]"#).unwrap_err();
    assert!(e.iter().any(|e| e.contains("regroup")), "{e:?}");
    assert_eq!(m, before);
    // an annotated unread box (by hand) still does not count as a 13th child
    get_mut(&mut m, "api/unread").needs_reading = false;
    assert_eq!(validate(&m), Ok(()));
}

#[test]
fn unknown_keys_in_arrows_and_patterns_are_rejected() {
    let (ws, g, cfg) = crate::map::shop::load();
    let boxes = facts(&ws, &g, &cfg);
    let m = refreshed(None, &boxes, crate::map::arrows(&boxes, &g));
    assert!(!m.arrows.is_empty());
    let mut v = serde_json::to_value(&m).unwrap();
    v["arrows"][0]["color"] = "red".into();
    let files = MemFiles::with(&[(MAP, v.to_string().as_str())]);
    assert!(load(&files).0.is_none());
    let mut v = serde_json::to_value(&m).unwrap();
    let api = v["boxes"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|b| b["id"] == "api")
        .unwrap();
    api["pattern"]["value"]["rings"] = 3.into();
    let files = MemFiles::with(&[(MAP, v.to_string().as_str())]);
    assert!(load(&files).0.is_none());
    let mut m = m;
    let e = annotate(
        &mut m,
        r#"[{"id": "api", "pattern": {"kind": "onion", "rings": 3}}]"#,
    )
    .unwrap_err();
    assert!(e.iter().any(|e| e.contains("rings")), "{e:?}");
}

#[test]
fn refresh_reports_pin_reset() {
    let mut old = shop();
    get_mut(&mut old, "api").pattern = Some(Field::agent(Pattern::Hexagonal, None));
    let (ws, g, cfg) = crate::map::shop::load();
    let boxes = facts(&ws, &g, &cfg);
    let (_, notices) = refresh(Some(&old), &boxes, vec![], &[]);
    assert_eq!(notices.len(), 1, "{notices:?}");
    assert!(
        notices[0].contains("api/application-refund") && notices[0].contains("domain"),
        "{notices:?}"
    );
    // ig's own (unpinned) slots that move say nothing
    let (_, notices) = refresh(Some(&shop()), &boxes, vec![], &[]);
    assert!(notices.is_empty(), "{notices:?}");
}

#[test]
fn validate_checks_parents_levels_cycles_and_version() {
    let ok = shop();
    let bad = |f: &dyn Fn(&mut MapFile)| {
        let mut m = ok.clone();
        f(&mut m);
        validate(&m).unwrap_err().join("; ")
    };
    // a pinned stale box that is its own parent
    let e = bad(&|m| {
        let mut z = get(m, "api/routers").clone();
        z.id = "zzz".into();
        z.parent = Some("zzz".into());
        z.pinned = vec!["title".into()];
        z.stale = true;
        m.boxes.push(z);
    });
    assert!(e.contains("`zzz`") && e.contains("reaches itself"), "{e}");
    let e = bad(&|m| get_mut(m, "api/routers").parent = Some("nope".into()));
    assert!(e.contains("parent `nope` is not in the map"), "{e}");
    // where a kind sits is a warning, not a validation error
    let mut moved = ok.clone();
    get_mut(&mut moved, "api").parent = Some("web".into());
    assert_eq!(validate(&moved), Ok(()));
    let e = bad(&|m| get_mut(m, "api/routers").level = Level::Code);
    assert!(e.contains("code boxes are not stored"), "{e}");
    let e = bad(&|m| get_mut(m, "system").parent = Some("api".into()));
    assert!(e.contains("`system`: the system has no parent"), "{e}");
    let e = bad(&|m| m.version = 2);
    assert!(e.contains("version 2"), "{e}");
}

#[test]
fn annotate_refits_ig_slots_to_a_new_pattern() {
    let (ws, g, cfg) = crate::map::shop::load();
    let boxes = facts(&ws, &g, &cfg);
    let mut m = refreshed(None, &boxes, vec![]);
    annotate(
        &mut m,
        r#"[{"id": "api", "pattern": {"kind": "hexagonal"}}]"#,
    )
    .unwrap();
    assert_eq!(get(&m, "api/routers").slot, None);
    // an agent's own slot that does not fit is still an error
    let e = annotate(
        &mut m,
        r#"[{"id": "api", "pattern": {"kind": "onion"}}, {"id": "api/routers", "slot": "core"}]"#,
    )
    .unwrap_err();
    assert!(e.join(";").contains("slot `core`"), "{e:?}");
}
