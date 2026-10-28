use super::*;
use crate::map::file::refresh;
use crate::map::{arrows, facts, shop};

/// The shop's fresh map and its evidence (the prisma client imports `@prisma/client`).
fn shop_map() -> (MapFile, Evidence) {
    let (ws, g, cfg) = shop::load();
    let boxes = facts(&ws, &g, &cfg);
    let (m, _) = refresh(None, &boxes, arrows(&boxes, &g), &[]);
    let imports = BTreeSet::from([(
        "apps/api/src/infra/prisma/client.ts".to_string(),
        "@prisma/client".to_string(),
    )]);
    (m, Evidence::new(&boxes, &imports, &cfg))
}

fn get<'a>(m: &'a MapFile, id: &str) -> &'a MapBox {
    m.boxes.iter().find(|b| b.id == id).unwrap()
}

/// `read` with an agent answering `replies` in turn; the prompts it got.
fn read_with(
    m: &mut MapFile,
    ev: &Evidence,
    replies: &[&str],
) -> (Result<usize, String>, Vec<String>) {
    let mut prompts = vec![];
    let mut run = |_: Agent, p: &str| {
        prompts.push(p.to_string());
        Ok(replies[(prompts.len() - 1).min(replies.len() - 1)].to_string())
    };
    let r = read(Agent::Claude, "{{boxes}}", m, ev, true, &mut run, &|_| {});
    (r, prompts)
}

#[test]
fn pick_order() {
    let none = Config::default();
    let found = |n: &str| n == "gemini" || n == "codex";
    assert_eq!(Agent::pick(&none, None, &found), Some(Agent::Gemini));
    let codex = Config::parse("agent: codex\n").unwrap();
    assert_eq!(Agent::pick(&codex, None, &found), Some(Agent::Codex));
    assert_eq!(
        Agent::pick(&codex, Some(Agent::Claude), &found),
        Some(Agent::Claude)
    );
    assert_eq!(Agent::pick(&none, None, &|_| false), None);
}

#[test]
fn compiler_upgrades_confidence() {
    let (mut m, ev) = shop_map();
    let reply = r#"<ig-annotations>[
        {"id": "postgres", "tech": "via Prisma", "confidence": "inferred"},
        {"id": "stripe", "tech": "Payments over HTTPS", "confidence": "inferred"},
        {"id": "api/domain-order", "kind": "domain", "title": "Order"}
    ]</ig-annotations>"#;
    let (r, _) = read_with(&mut m, &ev, &[reply]);
    assert_eq!(r, Ok(3));
    let conf = |id: &str| get(&m, id).tech.as_ref().unwrap().confidence;
    assert_eq!(conf("postgres"), Some(Confidence::Extracted));
    assert_eq!(conf("stripe"), Some(Confidence::Inferred));
    let order = get(&m, "api/domain-order");
    assert_eq!(order.kind.confidence, Some(Confidence::Extracted));
    assert_eq!(order.title.confidence, Some(Confidence::Inferred));
}

#[test]
fn the_prompt_holds_the_facts_and_a_retry_the_errors() {
    let (mut m, ev) = shop_map();
    let before = m.clone();
    let (r, prompts) = read_with(
        &mut m,
        &ev,
        &[
            "no tags here",
            "<ig-annotations>[{\"id\": \"nope\"}]</ig-annotations>",
        ],
    );
    assert!(r.unwrap_err().contains("unknown box `nope`"));
    assert_eq!(m, before, "the map is untouched");
    assert_eq!(prompts.len(), 2);
    assert!(
        prompts[0].contains("\"api/infra-prisma\""),
        "{}",
        prompts[0]
    );
    assert!(
        prompts[0].contains("@prisma/client"),
        "packages: {}",
        prompts[0]
    );
    assert!(
        prompts[0].contains("apps/api/src/infra/prisma/client.ts"),
        "sample"
    );
    assert!(prompts[1].contains("no <ig-annotations>"), "{}", prompts[1]);
}

#[test]
fn extract_takes_the_last_block() {
    let r = "Answer between <ig-annotations> and </ig-annotations>:\n<ig-annotations>\n[]\n</ig-annotations>";
    assert_eq!(extract(r), Some("[]"));
    assert_eq!(extract("prose"), None);
}

#[test]
fn offer_reads_only_on_consent() {
    let (mut m, ev) = shop_map();
    let cfg = Config::default();
    let mut run = |_: Agent, _: &str| -> Result<String, String> { unreachable!("never run") };
    let mut a = Ask {
        all: false,
        flag: None,
        template: "",
        found: &|_| true,
        confirm: None,
        run: &mut run,
    };
    let n = m.boxes.iter().filter(|b| b.needs_reading).count();
    let hint =
        format!("{n} boxes are ig's guess; run `ig view --read` to let an AI agent read them.");
    // no terminal, no --read: never read, a hint
    assert_eq!(
        offer(&mut a, &mut m, &ev, &cfg, &|_| {}),
        Some(hint.clone())
    );
    // asked, and the person says no
    let mut asked = false;
    let mut no = |_: Agent| {
        asked = true;
        false
    };
    a.confirm = Some(&mut no);
    assert_eq!(offer(&mut a, &mut m, &ev, &cfg, &|_| {}), None);
    // no agent to ask about: the hint, not the no-agent line
    a.found = &|_| false;
    assert_eq!(offer(&mut a, &mut m, &ev, &cfg, &|_| {}), Some(hint));
    // --read with no agent: the no-agent line
    a.all = true;
    assert_eq!(
        offer(&mut a, &mut m, &ev, &cfg, &|_| {}).as_deref(),
        Some(NO_AGENT)
    );
    // nothing needs reading: nothing to offer
    a.all = false;
    m.boxes.iter_mut().for_each(|b| b.needs_reading = false);
    a.found = &|_| panic!("nothing needs reading");
    assert_eq!(offer(&mut a, &mut m, &ev, &cfg, &|_| {}), None);
    assert!(asked);
}
