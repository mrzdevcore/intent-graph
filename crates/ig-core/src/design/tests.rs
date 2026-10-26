use super::*;

pub fn el(id: &str, kind: ElementKind, name: &str) -> Element {
    Element {
        id: id.into(),
        kind,
        name: name.into(),
        file: None,
        method: None,
        path: None,
        icon: None,
        layer: None,
        on: None,
        question: false,
        answer: None,
        frame: None,
        x: None,
        y: None,
        w: None,
        h: None,
    }
}

fn sample() -> Design {
    let mut d = empty("Export", Scope::Layer { name: "api".into() });
    let mut b = el("fn-b", ElementKind::Function, "b");
    (b.x, b.y) = (Some(10), Some(20));
    d.elements = vec![b, el("fn-a", ElementKind::Function, "a")];
    d.links = vec![
        Link {
            from: "fn-b".into(),
            to: "fn-a".into(),
            kind: LinkKind::Calls,
        },
        Link {
            from: "fn-a".into(),
            to: "src/a.ts#f".into(),
            kind: LinkKind::Calls,
        },
    ];
    d
}

#[test]
fn round_trip_is_canonical() {
    let t = to_text(&sample());
    assert_eq!(to_text(&parse(&t).unwrap()), t);
    assert!(t.ends_with("}\n"));
    assert!(!t.contains("\"w\""));
}

#[test]
fn elements_and_links_are_written_sorted_by_id() {
    let t = to_text(&sample());
    assert!(t.find("\"fn-a\"").unwrap() < t.find("\"fn-b\"").unwrap());
    let p = parse(&t).unwrap();
    assert_eq!(p.elements[0].id, "fn-a");
    assert_eq!(p.links[0].key(), "fn-a|calls|src/a.ts#f");
}

#[test]
fn newer_version_is_refused() {
    let e = parse(r#"{"version":2,"title":"t","scope":{"kind":"layer","name":"x"},"future":1}"#)
        .unwrap_err();
    assert_eq!(
        e.to_string(),
        "design.json is version 2; this ig reads version 1: update ig"
    );
}

#[test]
fn unknown_field_is_refused() {
    let t = r#"{"version":1,"title":"t","scope":{"kind":"layer","name":"x"},"extra":1}"#;
    assert!(matches!(parse(t), Err(DesignError::Json(_))));
}

fn with(f: impl FnOnce(&mut Design)) -> Result<Design, DesignError> {
    let mut d = sample();
    f(&mut d);
    parse(&serde_json::to_string(&d).unwrap())
}

fn invalid_msg(r: Result<Design, DesignError>) -> String {
    match r {
        Err(DesignError::Invalid(m)) => m,
        other => panic!("expected Invalid, got {other:?}"),
    }
}

#[test]
fn question_and_answer_only_on_notes() {
    let m = invalid_msg(with(|d| d.elements[1].question = true));
    assert_eq!(m, "fn-a: question and answer are for notes");
    assert!(
        with(|d| d.elements.push(Element {
            question: true,
            ..el("note-1", ElementKind::Note, "q")
        }))
        .is_ok()
    );
}

#[test]
fn bad_ids_and_dangling_links_are_invalid() {
    invalid_msg(with(|d| d.elements[0].id = "Fn-X".into()));
    invalid_msg(with(|d| d.elements[0].id = "fn-a".into()));
    let m = invalid_msg(with(|d| d.links[1].to = "fn-missing".into()));
    assert_eq!(m, "link fn-a|calls|fn-missing: no element fn-missing");
    assert!(with(|d| d.links[1].to = "src/a.ts#f".into()).is_ok());
}

#[test]
fn bridge_from_must_be_screen_slash_element() {
    let br = |from: &str, to: &str| {
        let (from, to) = (from.to_string(), to.to_string());
        with(move |d| d.bridges.push(Bridge { from, to }))
    };
    assert!(br("patient-list/btn-export", "fn-a").is_ok());
    invalid_msg(br("patient-list", "fn-a"));
    invalid_msg(br("patient-list/btn-export", "route-x"));
    assert!(br("patient-list/btn-export", "server/a.ts#f").is_ok());
}

#[test]
fn new_id_vectors() {
    use ElementKind::*;
    let with_ids = |ids: &[&str]| {
        let mut d = empty("t", Scope::Layer { name: "x".into() });
        d.elements = ids.iter().map(|i| el(i, Module, "m")).collect();
        d
    };
    let none = with_ids(&[]);
    assert_eq!(
        new_id(&none, Function, "Export patients"),
        "fn-export-patients"
    );
    assert_eq!(
        new_id(
            &with_ids(&["fn-export-patients"]),
            Function,
            "Export patients"
        ),
        "fn-export-patients-2"
    );
    assert_eq!(
        new_id(&none, Route, "GET /api/patients/export"),
        "route-get-api-patients-export"
    );
    assert_eq!(new_id(&none, System, "PostgreSQL"), "sys-postgresql");
    assert_eq!(new_id(&with_ids(&["note-1"]), Note, "anything"), "note-2");
    assert_eq!(new_id(&none, Module, "  !!  "), "mod-1");
}
