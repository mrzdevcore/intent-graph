use super::*;
use crate::map::file::refresh;
use crate::map::{arrows, facts};

fn shop_map() -> MapFile {
    let (ws, g, cfg) = crate::map::shop::load();
    let b = facts(&ws, &g, &cfg);
    refresh(None, &b, arrows(&b, &g), &[]).0
}

#[test]
fn the_shop_is_written_one_file_per_container() {
    let files = write(&shop_map());
    assert_eq!(
        files.keys().collect::<Vec<_>>(),
        [
            "ig/api-client.kdl",
            "ig/api.kdl",
            "ig/system.kdl",
            "ig/web.kdl"
        ]
    );
    let sys = &files["ig/system.kdl"];
    assert!(sys.starts_with("// ig's intended architecture"));
    assert!(sys.contains("system \"system\""));
    assert!(sys.contains("store \"postgres\""));
    assert!(sys.contains("external \"stripe\""));
    let api = &files["ig/api.kdl"];
    assert!(api.contains("app \"api\" title=\"Api\""));
    assert!(api.contains("at \"apps/api/**\""));
    assert!(api.contains("data \"postgres\""));
    assert!(api.contains("network \"stripe\""));
    assert!(api.contains("usecase \"api/application-checkout\""));
    assert!(!api.contains("function"), "code level never written");
}

#[test]
fn writing_is_deterministic_and_every_file_parses() {
    let (a, b) = (write(&shop_map()), write(&shop_map()));
    assert_eq!(a, b);
    for t in a.values() {
        t.parse::<kdl::KdlDocument>().unwrap();
    }
}

fn names(doc: &kdl::KdlDocument, out: &mut Vec<String>) {
    for n in doc.nodes() {
        out.push(n.name().value().to_string());
        if let Some(c) = n.children() {
            names(c, out);
        }
    }
}

fn parse(t: &str) -> kdl::KdlDocument {
    t.parse().unwrap()
}

#[test]
fn no_code_level_kind_is_a_node_name() {
    let mut all = vec![];
    for t in write(&shop_map()).values() {
        names(&parse(t), &mut all);
    }
    for k in ["function", "class", "method", "file", "parts", "task"] {
        assert!(!all.iter().any(|n| n == k), "{k} written");
    }
    assert!(all.iter().any(|n| n == "usecase"));
}

#[test]
fn bundle_arrows_lose_their_derived_label() {
    let files = write(&shop_map());
    let web = &files["ig/web.kdl"];
    assert!(web.contains("bundle \"api-client\"\n") || web.contains("bundle \"api-client\" "));
    assert!(!web.contains("1 call"));
    assert!(web.contains("network \"api\" label=\"POST /checkout\""));
}

#[test]
fn fields_are_written_and_dropped_as_the_brief_says() {
    use crate::map::file::{Confidence, Field};
    let mut m = shop_map();
    for b in m.boxes.iter_mut().filter(|b| b.id == "api") {
        b.badges = Some(Field::person(vec!["core".into(), "paid".into()]));
        b.pattern = Some(Field::agent(
            Pattern::Layered {
                bands: vec!["ui".into(), "db".into()],
            },
            Some(Confidence::Ambiguous),
        ));
        b.package = Some("@shop/api".into());
        b.service_name = Some("svc-name".into());
        b.pinned = vec!["title".into()];
        b.needs_reading = true;
        b.stale = true;
    }
    let api = &write(&m)["ig/api.kdl"];
    assert!(api.contains("badges \"core\" \"paid\""), "{api}");
    assert!(api.contains("pattern \"layered\" \"ui\" \"db\""), "{api}");
    for dropped in [
        "pinned",
        "files",
        "package",
        "svc-name",
        "needsReading",
        "stale",
        "count",
        "source",
        "confidence",
        "ambiguous",
    ] {
        assert!(!api.contains(dropped), "{dropped} written");
    }
    let ids: Vec<_> = parse(api).nodes()[0]
        .children()
        .unwrap()
        .nodes()
        .iter()
        .filter_map(|n| n.entries()[0].value().as_string().map(String::from))
        .filter(|s| s.starts_with("api/"))
        .collect();
    let mut sorted = ids.clone();
    sorted.sort();
    assert!(ids.len() > 3);
    assert_eq!(ids, sorted);
}

#[test]
fn arrows_are_sorted_and_a_nested_id_maps_to_a_stem() {
    let mut m = shop_map();
    let mut c = m.boxes.iter().find(|b| b.id == "api").unwrap().clone();
    c.id = "api/inner".into();
    c.parent = Some("system".into());
    m.boxes.push(c);
    let mut k = m.boxes.iter().find(|b| b.id == "api/root").unwrap().clone();
    k.id = "api/inner/x".into();
    k.parent = Some("api/inner".into());
    m.boxes.push(k);
    let files = write(&m);
    assert!(files.contains_key("ig/api-inner.kdl"), "{:?}", files.keys());
    let api = parse(&files["ig/api.kdl"]);
    let arrows: Vec<_> = api.nodes()[0]
        .children()
        .unwrap()
        .nodes()
        .iter()
        .filter(|n| ["data", "network"].contains(&n.name().value()))
        .map(|n| n.name().value().to_string() + n.entries()[0].value().as_string().unwrap())
        .collect();
    assert_eq!(arrows, ["datapostgres", "networkstripe"]);
}

#[test]
fn a_second_root_never_overwrites_the_system_file() {
    let mut m = shop_map();
    let mut other = m.boxes.iter().find(|b| b.id == "postgres").unwrap().clone();
    other.id = "zed".into();
    other.parent = None;
    m.boxes.push(other);
    let files = write(&m);
    assert!(files["ig/system.kdl"].contains("system \"system\""));
    assert!(files["ig/zed.kdl"].contains("\"zed\""));
}

#[test]
fn awkward_strings_round_trip() {
    let mut m = shop_map();
    let (t, d) = ("a \"q\" \\ b\n\tc", "x\\\"\n\t\r y");
    for b in m.boxes.iter_mut().filter(|b| b.id == "api") {
        b.title.value = t.into();
        b.description = Some(crate::map::file::Field::ig(d.into()));
    }
    let doc = parse(&write(&m)["ig/api.kdl"]);
    let n = &doc.nodes()[0];
    assert_eq!(n.get("title").unwrap().as_string(), Some(t));
    assert_eq!(n.get("description").unwrap().as_string(), Some(d));
}

fn mem(w: &BTreeMap<String, String>) -> ig_core::testing::MemFiles {
    ig_core::testing::MemFiles::with(
        &w.iter()
            .map(|(p, t)| (p.as_str(), t.as_str()))
            .collect::<Vec<_>>(),
    )
}

#[test]
fn what_is_written_reads_back_the_same() {
    use crate::map::file::Field;
    let mut m = shop_map();
    for b in m.boxes.iter_mut().filter(|b| b.id == "api") {
        b.badges = Some(Field::person(vec!["core".into()]));
        b.description = Some(Field::person("Takes orders".into()));
        b.pattern = Some(Field::person(Pattern::Layered {
            bands: vec![
                "application".into(),
                "domain".into(),
                "infrastructure".into(),
            ],
        }));
    }
    let r = read(&mem(&write(&m))).unwrap().unwrap();
    let key = |b: &MapBox| {
        (
            b.id.clone(),
            b.parent.clone(),
            b.kind.value,
            b.title.value.clone(),
            b.made_of.clone(),
            b.description.as_ref().map(|f| f.value.clone()),
            b.tech.as_ref().map(|f| f.value.clone()),
            b.badges.as_ref().map(|f| f.value.clone()),
            b.pattern.as_ref().map(|f| f.value.clone()),
        )
    };
    let api = r.boxes.iter().find(|b| b.id == "api").unwrap();
    assert_eq!(api.description.as_ref().unwrap().value, "Takes orders");
    assert!(api.badges.is_some() && api.pattern.is_some());
    for b in &r.boxes {
        assert_eq!(b.pinned, crate::map::file::PINNABLE, "{}", b.id);
        assert_eq!(b.made_of_source, crate::map::file::Source::Person);
    }
    assert_eq!(
        r.boxes.iter().map(key).collect::<Vec<_>>(),
        m.boxes
            .iter()
            .filter(|b| !b.stale)
            .map(key)
            .collect::<Vec<_>>()
    );
    assert_eq!(
        r.arrows
            .iter()
            .map(|a| (&a.from, &a.to, a.kind))
            .collect::<Vec<_>>(),
        m.arrows
            .iter()
            .map(|a| (&a.from, &a.to, a.kind))
            .collect::<Vec<_>>()
    );
}

#[test]
fn a_broken_spec_names_file_and_line_and_reads_nothing() {
    use ig_core::testing::MemFiles;
    let files = MemFiles::with(&[(
        "ig/system.kdl",
        "system \"system\" {\n  app \"web\" {\n    calls \"nowhere\"\n  }\n}\n",
    )]);
    let e = read(&files).unwrap_err();
    assert!(
        e.iter()
            .any(|x| x.starts_with("ig/system.kdl:3:") && x.contains("nowhere")),
        "{e:?}"
    );
    let files = MemFiles::with(&[(
        "ig/system.kdl",
        "system \"system\" {\n  app \"web\" title=\n}\n",
    )]);
    let e = read(&files).unwrap_err();
    assert!(e[0].starts_with("ig/system.kdl:2:"), "{e:?}");
    assert_eq!(read(&MemFiles::with(&[])).unwrap(), None);
}

fn err_of(kdl: &str) -> Vec<String> {
    read(&ig_core::testing::MemFiles::with(&[("ig/system.kdl", kdl)])).unwrap_err()
}

fn assert_err(kdl: &str, prefix: &str, what: &str) {
    let e = err_of(kdl);
    assert!(
        e.iter().any(|x| x.starts_with(prefix) && x.contains(what)),
        "{e:?}"
    );
}

#[test]
fn every_kind_of_mistake_is_an_error_with_a_location() {
    let sys = |inner: &str| format!("system \"s\" title=\"S\" {{\n{inner}\n}}\n");
    assert_err(
        &sys("  gizmo \"a\" title=\"A\""),
        "ig/system.kdl:2:",
        "gizmo",
    );
    assert_err(
        &sys("  app \"a\" title=\"A\"\n  app \"a\" title=\"A\""),
        "ig/system.kdl:3:",
        "twice",
    );
    assert_err(
        &sys("  app \"a\" title=\"A\" color=\"red\""),
        "ig/system.kdl:2:",
        "color",
    );
    assert_err(&sys("  app \"a\""), "ig/system.kdl:2:", "title");
    assert_err(
        &sys("  app \"a\" title=\"A\" {\n    pattern \"blob\"\n  }"),
        "ig/system.kdl:3:",
        "pattern",
    );
    assert_err(
        &sys("  app \"a\" title=\"A\" {\n    at \"a/[\"\n  }"),
        "ig/system.kdl:3:",
        "bad glob",
    );
    assert_err("app \"a\" title=\"A\"\n", "ig/system.kdl:1:", "no system");
    let two = "system \"s\" title=\"S\"\nsystem \"t\" title=\"T\"\n";
    assert_err(two, "ig/system.kdl:2:", "second system");
}

#[test]
fn a_file_of_two_nodes_and_a_bad_file_beside_a_good_one_read_nothing() {
    use ig_core::testing::MemFiles;
    let good = "system \"s\" title=\"S\"\n";
    let files = MemFiles::with(&[
        ("ig/system.kdl", good),
        ("ig/x.kdl", "app \"a\" title=\"A\"\napp \"b\" title=\"B\"\n"),
    ]);
    let e = read(&files).unwrap_err();
    assert!(e.iter().any(|x| x.starts_with("ig/x.kdl:1:")), "{e:?}");
    let files = MemFiles::with(&[("ig/system.kdl", good), ("ig/x.kdl", "app \"a\" title=\n")]);
    assert!(read(&files).is_err());
}

#[test]
fn a_duplicate_id_with_an_arrow_to_it_is_an_error_not_a_hang() {
    let e = err_of(
        "system \"s\" title=\"S\" {\n  app \"x\" title=\"X\" {\n    app \"x\" title=\"X\"\n  }\n  calls \"x\"\n}\n",
    );
    assert!(
        e.iter()
            .any(|x| x.starts_with("ig/system.kdl:3:") && x.contains("twice")),
        "{e:?}"
    );
}
