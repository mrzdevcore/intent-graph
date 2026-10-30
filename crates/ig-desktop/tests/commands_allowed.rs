//! Every command the app registers is one the webview may call: a command left out of
//! `build.rs` or `capabilities/` fails at run time with "not allowed", never at build time.

#[test]
fn every_registered_command_is_allowed() {
    let main = include_str!("../src/main.rs");
    let build = include_str!("../build.rs");
    let caps = include_str!("../capabilities/default.json");
    let list = main.split("generate_handler![").nth(1).unwrap();
    let list = list.split(']').next().unwrap();
    for cmd in list.split(',').map(|c| c.trim()).filter(|c| !c.is_empty()) {
        let name = cmd.rsplit("::").next().unwrap();
        assert!(build.contains(&format!("\"{name}\"")), "{name} missing from build.rs");
        let perm = format!("\"allow-{}\"", name.replace('_', "-"));
        assert!(caps.contains(&perm), "{perm} missing from capabilities/default.json");
    }
}
