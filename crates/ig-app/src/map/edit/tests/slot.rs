//! A move out of a pattern's block clears the slot the new parent does not offer.

use super::*;

const SYS2: &str = r#"// ig's intended architecture: edit freely; ig changes it only when you or an agent ask (ig accept, ig fmt, edits in the app, ig_edit).
system "shop" title="Shop" {
    at "**"
    app "billing" title="Billing" {
        at "apps/billing/**"
    }
}
"#;

const ONION: &str = r#"// ig's intended architecture: edit freely; ig changes it only when you or an agent ask (ig accept, ig fmt, edits in the app, ig_edit).
app "api" title="Api" {
    at "apps/api/**"
    pattern "onion"
    usecase "api/application-refund" title="Refund" slot="application" {
        at "apps/api/src/application/refund/**"
    }
    usecase "api/orders" title="Orders" slot="domain"
}
"#;

#[test]
fn a_move_clears_a_slot_the_new_parent_does_not_offer_and_its_inverse_restores_it() {
    let f = MemFiles::with(&[("ig/system.kdl", SYS2), ("ig/api.kdl", ONION)]);
    let before = texts(&f);
    let mv = Edit::Move {
        id: "api/application-refund".into(),
        parent: "billing".into(),
        before: None,
        slot: None,
    };
    let inv = apply(&f, &mv).unwrap();
    let billing = &texts(&f)["ig/billing.kdl"];
    assert!(
        billing.contains("usecase \"api/application-refund\" title=\"Refund\" {"),
        "{billing}"
    );
    let m = spec::read(&f).unwrap().unwrap();
    let b = m.boxes.iter().find(|b| b.id == "api/application-refund");
    assert_eq!(b.unwrap().slot, None);
    assert_eq!(
        inv,
        Edit::Move {
            id: "api/application-refund".into(),
            parent: "api".into(),
            before: Some("api/orders".into()),
            slot: Some("application".into()),
        }
    );
    let again = apply(&f, &inv).unwrap();
    assert_eq!(texts(&f), before);
    assert_eq!(again, mv);
}
