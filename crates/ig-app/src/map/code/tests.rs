use super::*;
use crate::map::facts;
use crate::map::file::refresh;

/// Every function, class and method of `g` has its own box; no box holds more than 12; a
/// group always opens onto something; ids are unique and arrows join known boxes.
fn check(code: &[MapBox], arrows: &[MapArrow], g: &Graph) {
    let ids: HashSet<&str> = code.iter().map(|b| b.id.as_str()).collect();
    assert_eq!(ids.len(), code.len(), "ids are unique");
    for n in g
        .nodes
        .iter()
        .filter(|n| matches!(n.kind.as_str(), "function" | "class"))
    {
        assert!(ids.contains(n.id.as_str()), "{} has no box", n.id);
    }
    let mut kids: BTreeMap<&str, usize> = BTreeMap::new();
    for b in code {
        *kids.entry(b.parent.as_deref().unwrap()).or_default() += 1;
    }
    assert!(kids.values().all(|&n| n <= MAX_CODE), "{kids:?}");
    for b in code.iter().filter(|b| {
        matches!(
            b.kind.value,
            BoxKind::Group | BoxKind::Folder | BoxKind::File
        )
    }) {
        assert!(
            kids.contains_key(b.id.as_str()),
            "{} is an empty group",
            b.id
        );
    }
    assert!(
        arrows
            .iter()
            .all(|a| ids.contains(a.from.as_str()) && ids.contains(a.to.as_str()))
    );
}

#[test]
fn every_function_has_a_box_however_many() {
    let (ws, mut g, cfg) = crate::map::shop::load();
    let b = facts(&ws, &g, &cfg);
    let m = refresh(None, &b, arrows(&b, &g), &[]).0;
    let code = code_boxes(&m, &g, &[]).boxes;
    // few functions: one box per node, id = the node id
    let order: Vec<&str> = code
        .iter()
        .filter(|b| b.parent.as_deref() == Some("api/domain-order"))
        .map(|b| b.id.as_str())
        .collect();
    assert_eq!(
        order,
        [
            "apps/api/src/domain/order/order.ts#Order",
            "apps/api/src/domain/order/order.ts#loadOrder"
        ]
    );
    let file = "apps/api/src/domain/payment/payment.ts";
    let total = g
        .nodes
        .iter()
        .find(|n| n.id == format!("{file}#total"))
        .unwrap()
        .clone();
    let words = [
        "get", "set", "load", "save", "make", "find", "list", "sync", "read", "send", "drop",
        "wrap", "fix", "run", "other",
    ];
    // 200 functions in one file: prefixes, more than 12 of them, share range boxes
    for i in 0..200 {
        let mut n = total.clone();
        n.name = format!("{}Thing{i}", words[i % words.len()]);
        n.id = format!("{file}#{}", n.name);
        g.nodes.push(n);
    }
    let Code {
        boxes: code,
        arrows,
        ..
    } = code_boxes(&m, &g, &[]);
    check(&code, &arrows, &g);
    let depth = |id: &str| {
        let mut d = 0;
        let mut p = Some(id);
        while let Some(x) = p.filter(|x| *x != "api/domain-payment") {
            d += 1;
            p = code
                .iter()
                .find(|b| b.id == x)
                .and_then(|b| b.parent.as_deref());
        }
        d
    };
    assert!(
        depth(&format!("{file}#getThing0")) >= 3,
        "range, prefix, function"
    );
    // one name, 40 times over: alphabetical ranges
    let mut g2 = g.clone();
    g2.nodes.retain(|n| !n.name.contains("Thing"));
    for i in 0..40 {
        let mut n = total.clone();
        n.id = format!("{file}#x{i}");
        n.name = "x".into();
        g2.nodes.push(n);
    }
    let Code {
        boxes: code2,
        arrows: arrows2,
        ..
    } = code_boxes(&m, &g2, &[]);
    check(&code2, &arrows2, &g2);
}

#[test]
fn folders_files_and_methods_nest() {
    let (ws, mut g, cfg) = crate::map::shop::load();
    let b = facts(&ws, &g, &cfg);
    let m = refresh(None, &b, arrows(&b, &g), &[]).0;
    let total = g
        .nodes
        .iter()
        .find(|n| n.id.ends_with("payment.ts#total"))
        .unwrap()
        .clone();
    // 3 subfolders of payment/ with 5 files of 2 functions, and a class of 3 methods
    for d in 0..3 {
        for f in 0..5 {
            let file = format!("apps/api/src/domain/payment/d{d}/f{f}.ts");
            let mut module = g.nodes.iter().find(|n| n.is_module()).unwrap().clone();
            module.id = file.clone();
            module.file = file.clone();
            g.nodes.push(module);
            for k in 0..2 {
                let mut n = total.clone();
                n.name = format!("fn{k}");
                n.id = format!("{file}#{}", n.name);
                n.file = file.clone();
                n.parent = Some(file.clone());
                g.nodes.push(n);
            }
        }
    }
    let host = "apps/api/src/domain/payment/d0/f0.ts";
    let mut class = total.clone();
    (class.id, class.name, class.kind) = (format!("{host}#Box"), "Box".into(), "class".into());
    (class.file, class.parent) = (host.into(), Some(host.into()));
    for k in 0..3 {
        let mut n = total.clone();
        (n.id, n.name, n.kind) = (format!("{host}#Box.m{k}"), format!("m{k}"), "method".into());
        (n.file, n.parent) = (host.into(), Some(class.id.clone()));
        g.nodes.push(n);
    }
    g.nodes.push(class);
    let mut m = m;
    let c = m
        .boxes
        .iter_mut()
        .find(|b| b.id == "api/domain-payment")
        .unwrap();
    c.nodes
        .extend((0..3).flat_map(|d| {
            (0..5).map(move |f| format!("apps/api/src/domain/payment/d{d}/f{f}.ts"))
        }));
    let Code {
        boxes: code,
        arrows,
        ..
    } = code_boxes(&m, &g, &[]);
    check(&code, &arrows, &g);
    let kind_of = |id: &str| code.iter().find(|b| b.id == id).map(|b| b.kind.value);
    assert_eq!(
        kind_of("apps/api/src/domain/payment/d0/"),
        Some(BoxKind::Folder)
    );
    // d0 holds 11 items: they sit in the folder itself, with no file level
    assert_eq!(kind_of(host), None);
    let class = code.iter().find(|b| b.id == format!("{host}#Box")).unwrap();
    assert_eq!(
        class.parent.as_deref(),
        Some("apps/api/src/domain/payment/d0/")
    );
    let methods: Vec<&MapBox> = code
        .iter()
        .filter(|b| b.parent.as_deref() == Some(&format!("{host}#Box")))
        .collect();
    assert_eq!(methods.len(), 3);
    assert!(methods.iter().all(|b| b.kind.value == BoxKind::Method));
}

#[test]
fn a_component_opens_onto_its_parts_and_functions() {
    let (ws, g, cfg) = crate::map::shop::load();
    let b = facts(&ws, &g, &cfg);
    let m = refresh(None, &b, arrows(&b, &g), &[]).0;
    let file = "apps/api/src/application/checkout/checkout.ts";
    let module = g.nodes.iter().find(|n| n.id == file).unwrap();
    let fns: Vec<&Node> = g
        .nodes
        .iter()
        .filter(|n| n.parent.as_deref() == Some(file))
        .collect();
    let part = |name: &str, kind: &str| Part {
        name: name.into(),
        kind: kind.into(),
        line: 1,
        detail: None,
    };
    let a = Anatomy {
        framework: "vue".into(),
        name: "Checkout".into(),
        file: file.into(),
        range: module.range,
        inputs: vec![part("orderId", "prop")],
        // a computed value is listed as state, not as a function
        state: vec![part(&fns[0].name, "computed")],
        uses: vec![],
        functions: vec![],
        renders: vec![part("Row", "tag")],
        template: Some(Range {
            start_line: 1,
            end_line: 2,
        }),
    };
    let code = code_boxes(&m, &g, &[a]);
    let kid = |p: &str| -> Vec<(&str, BoxKind)> {
        let mut v: Vec<_> = (code.boxes.iter())
            .filter(|b| b.parent.as_deref() == Some(p))
            .map(|b| (b.id.as_str(), b.kind.value))
            .collect();
        v.sort();
        v
    };
    let c = code.boxes.iter().find(|b| b.id == file).unwrap();
    assert_eq!(
        (c.kind.value, c.title.value.as_str()),
        (BoxKind::Component, "Checkout")
    );
    assert_eq!(c.logo.as_ref().map(|l| l.value.as_str()), Some("vue"));
    let mut want = vec![
        (format!("{file}/inputs"), BoxKind::Parts),
        (format!("{file}/renders"), BoxKind::Parts),
        (format!("{file}/state"), BoxKind::Parts),
    ];
    want.extend(fns[1..].iter().map(|n| (n.id.clone(), BoxKind::Function)));
    want.sort();
    let got: Vec<(String, BoxKind)> = kid(file).into_iter().map(|(i, k)| (i.into(), k)).collect();
    assert_eq!(got, want);
    let parts: BTreeMap<_, _> = code.parts.iter().cloned().collect();
    assert_eq!(parts[&format!("{file}/inputs")][0].name, "orderId");
    let template = code
        .spans
        .iter()
        .find(|s| s.0 == format!("{file}/renders"))
        .unwrap();
    assert_eq!((template.2.start_line, template.2.end_line), (1, 2));
    // the module itself is in the component box: its imports end there
    assert!(c.nodes.iter().any(|n| n == file), "{:?}", c.nodes);
}

#[test]
fn folders_deep_down_show_their_pattern() {
    let (ws, mut g, cfg) = crate::map::shop::load();
    let b = facts(&ws, &g, &cfg);
    let mut m = refresh(None, &b, arrows(&b, &g), &[]).0;
    let total = g
        .nodes
        .iter()
        .find(|n| n.id.ends_with("payment.ts#total"))
        .unwrap()
        .clone();
    // payment/ as a clean architecture: 4 rings of 4 files, and a folder no ring names
    let dirs = [
        "application",
        "domain",
        "infrastructure",
        "use-cases",
        "misc",
    ];
    let mut files = vec![];
    for d in dirs {
        for f in 0..4 {
            let file = format!("apps/api/src/domain/payment/{d}/f{f}.ts");
            let mut module = g.nodes.iter().find(|n| n.is_module()).unwrap().clone();
            (module.id, module.file) = (file.clone(), file.clone());
            g.nodes.push(module);
            let mut n = total.clone();
            (n.id, n.file, n.parent) = (format!("{file}#fn"), file.clone(), Some(file.clone()));
            g.nodes.push(n);
            files.push(file);
        }
    }
    let c = m
        .boxes
        .iter_mut()
        .find(|b| b.id == "api/domain-payment")
        .unwrap();
    c.nodes.extend(files);
    let code = code_boxes(&m, &g, &[]);
    assert_eq!(
        code.patterns,
        [("api/domain-payment".to_string(), Pattern::Onion)]
    );
    let slot_of = |d: &str| {
        let id = format!("apps/api/src/domain/payment/{d}/");
        let b = code.boxes.iter().find(|b| b.id == id).unwrap();
        b.slot.as_ref().map(|s| s.value.clone())
    };
    let slots: Vec<Option<String>> = dirs.iter().map(|d| slot_of(d)).collect();
    let want = [
        "application",
        "domain",
        "infrastructure",
        "application",
        "infrastructure",
    ];
    assert_eq!(slots, want.map(|s| Some(s.to_string())));
    // the pattern goes on the box holding the folders, through `patterns`, not on its code
    assert!(code.boxes.iter().all(|b| b.pattern.is_none()));
}
