use super::*;
use crate::map::file::{Field, refresh};
use crate::map::{MapArrow, facts};
use ig_core::model::Finding;
use ig_core::testing::MemFiles;

/// ig's own map of the shop.
fn shop() -> MapFile {
    let (ws, g, cfg) = crate::map::shop::load();
    let b = facts(&ws, &g, &cfg);
    refresh(None, &b, arrows(&b, &g), &[]).0
}

#[test]
fn onion_outward_is_bad() {
    let m = shop();
    let bad: Vec<Mark> = rules(&m, &[])
        .into_iter()
        .filter(|k| k.state == MarkState::Bad)
        .collect();
    assert_eq!(bad.len(), 1, "{bad:#?}");
    assert_eq!(bad[0].target, "api/domain-order->api/infra-prisma");
    assert_eq!(
        bad[0].badge.as_deref(),
        Some("breaks: domain → infrastructure")
    );
    assert!(m.marks.contains(&bad[0]), "refresh stores the rules layer");
}

#[test]
fn finding_lands_on_its_arrow() {
    let m = shop();
    let f = Finding::new(
        "layers",
        Severity::Error,
        "ig",
        "apps/api/src/domain/order/order.ts",
        2,
        "domain may not import infra".into(),
    )
    .edge(
        "apps/api/src/domain/order/order.ts",
        "apps/api/src/infra/prisma/client.ts",
    );
    let got = rules(&m, &[f]);
    assert!(
        got.iter()
            .any(|k| k.target == "api/domain-order->api/infra-prisma"
                && k.badge.as_deref() == Some("layers")
                && k.state == MarkState::Bad),
        "{got:#?}"
    );
}

#[test]
fn loop_to_port_is_risk() {
    let m = shop();
    let got = risks(&m);
    let k = got
        .iter()
        .find(|k| k.badge.as_deref() == Some("risk: query in a loop"))
        .unwrap_or_else(|| panic!("{got:#?}"));
    assert_eq!(k.state, MarkState::Warn);
    assert_eq!(k.target, "api/application-checkout->api/infra-prisma");
    assert!(m.marks.contains(k), "refresh stores the risks layer");
}

#[test]
fn three_network_hops_is_risk() {
    let mut m = shop();
    let api = m.boxes.iter().find(|b| b.id == "api").unwrap().clone();
    for id in ["b", "c"] {
        let mut b = api.clone();
        b.id = id.into();
        m.boxes.push(b);
    }
    m.boxes.iter_mut().find(|b| b.id == "web").unwrap().kind = Field::ig(BoxKind::App);
    let net = |from: &str, to: &str| MapArrow {
        note: None,
        context: None,
        written: Default::default(),
        seen: vec![],
        id: format!("{from}->{to}"),
        parent: "system".into(),
        from: from.into(),
        to: to.into(),
        kind: ArrowKind::Calls,
        label: None,
        count: 1,
        kinds: vec![ArrowKind::Calls],
        in_loop: false,
        origin: Default::default(),
        parts: vec![],
    };
    // web -> api (already there) -> b -> c, and a cycle back that must not loop forever
    m.arrows
        .extend([net("api", "b"), net("b", "c"), net("c", "api")]);
    let got = risks(&m);
    assert!(
        got.iter()
            .any(|k| k.target == "web" && k.badge.as_deref() == Some("3 network hops")),
        "{got:#?}"
    );
}

#[test]
fn file_layer_marks() {
    let files = MemFiles::with(&[(
        ".ig/layers/health.json",
        r#"{"layer": "health", "marks": [
            {"target": "api", "state": "ok", "value": "99.9%"},
            {"target": "nope", "state": "down"}
        ]}"#,
    )]);
    let known: HashSet<&str> = ["api", "web"].into();
    let (marks, notices) = file_layers(&files, &known);
    assert_eq!(marks.len(), 1, "{marks:#?}");
    assert_eq!(
        (marks[0].layer.as_str(), marks[0].target.as_str()),
        ("health", "api")
    );
    assert_eq!(notices.len(), 1, "{notices:?}");
    assert!(notices[0].contains("nope"), "{notices:?}");
}
