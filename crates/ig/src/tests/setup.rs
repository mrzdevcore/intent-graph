//! Setup facts on the monorepo fixture.

use super::env;
use ig_app::setup;

#[test]
fn facts_on_fixture() {
    let src = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/monorepo");
    let f = setup::facts(&env(&src)).unwrap();
    assert!(f["config"]["error"].is_null());
    assert_eq!(f["packages"].as_array().unwrap().len(), 3);
    assert!(
        f["externalPackages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["package"] == "express")
    );
    assert!(
        f["archViolations"]
            .as_object()
            .unwrap()
            .keys()
            .any(|k| k.contains("core must not depend on ui"))
    );
    let r = setup::report(&f);
    assert!(r.contains("## External packages"));
    assert!(crate::skills::agent("cursor").is_err());
}
