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
