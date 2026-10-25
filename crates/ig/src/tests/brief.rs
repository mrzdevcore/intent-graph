//! `ig brief` on the monorepo fixture.

use super::{check_run, env};
use ig_app::brief::brief;
use ig_app::check::Opts;

#[test]
fn brief_on_fixture() {
    let src = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/monorepo");
    let dir = std::env::temp_dir().join(format!("ig-brief-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        std::process::Command::new("cp")
            .arg("-R")
            .arg(&src)
            .arg(&dir)
            .status()
            .unwrap()
            .success()
    );
    // Outside git everything is added.
    let run = check_run(&dir, &Opts::default()).unwrap();
    let md = brief(&env(&dir), &run, &Opts::default(), true);
    assert!(md.contains("## Architecture"));
    assert!(md.contains("+ GET /users (route)"));
    assert!(md.contains(
        "**+ add** (new) · `packages/core/src/math.ts:2`\n\n```diff\n+export function add("
    ));
    // A class whose methods are shown on their own is not repeated.
    assert!(!md.contains("**+ Repo** (new)"));
    assert!(md.contains("```mermaid\nsequenceDiagram"));
    assert!(md.contains("New links between layers:") && md.contains("core → ui (1)"));
    let text = brief(&env(&dir), &run, &Opts::default(), false);
    assert!(text.starts_with("CHANGE BRIEF"));
    assert!(!text.contains("EXCEPTIONS"));
    std::fs::create_dir_all(dir.join(".ig")).unwrap();
    std::fs::write(
        dir.join(".ig/baseline.json"),
        r#"["plain", {"fingerprint": "abc", "reason": "user: legacy import"}]"#,
    )
    .unwrap();
    let md = brief(&env(&dir), &run, &Opts::default(), true);
    assert!(
        md.contains("## Exceptions\n\n- abc: user: legacy import\n"),
        "{md}"
    );
    let text = brief(&env(&dir), &run, &Opts::default(), false);
    assert!(
        text.contains("EXCEPTIONS\n\n- abc: user: legacy import\n"),
        "{text}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
