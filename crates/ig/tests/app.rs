//! `ig init` and `ig analyse` on an empty repo, and `ig app` opening the desktop app.

use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

fn dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("ig-app-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d.canonicalize().unwrap()
}

#[test]
fn analyse_on_an_empty_repo_writes_an_empty_system() {
    let d = dir("init");
    for args in [&["init"][..], &["analyse", "--no-agent"]] {
        let o = Command::new(env!("CARGO_BIN_EXE_ig"))
            .args(["--root"])
            .arg(&d)
            .args(args)
            .output()
            .unwrap();
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    }
    let sys = std::fs::read_to_string(d.join("ig/system.kdl")).unwrap();
    assert!(sys.contains("system \"system\""), "{sys}");
    // a second init leaves ig/ alone
    std::fs::write(d.join("ig/system.kdl"), "// mine\n").unwrap();
    let o = Command::new(env!("CARGO_BIN_EXE_ig"))
        .arg("--root")
        .arg(&d)
        .arg("init")
        .output()
        .unwrap();
    assert!(o.status.success());
    assert_eq!(
        std::fs::read_to_string(d.join("ig/system.kdl")).unwrap(),
        "// mine\n"
    );
}

#[cfg(unix)]
#[test]
fn a_directory_or_app_opens_the_app_on_it() {
    use std::os::unix::fs::PermissionsExt;
    let d = dir("open");
    let out = d.join("opened");
    let fake = d.join("fake-app");
    std::fs::write(
        &fake,
        format!("#!/bin/sh\necho \"$1\" >> '{}'\n", out.display()),
    )
    .unwrap();
    std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
    let project = d.join("shop");
    std::fs::create_dir(&project).unwrap();
    for args in [vec!["app"], vec!["app", "."]] {
        let o = Command::new(env!("CARGO_BIN_EXE_ig"))
            .current_dir(&project)
            .env("IG_APP", &fake)
            .args(&args)
            .output()
            .unwrap();
        assert!(
            o.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&o.stderr)
        );
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    let lines = loop {
        let s = std::fs::read_to_string(&out).unwrap_or_default();
        if s.lines().count() == 2 || Instant::now() > deadline {
            break s;
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let want = format!("{}\n", project.display());
    assert_eq!(lines, want.repeat(2));
    let o = Command::new(env!("CARGO_BIN_EXE_ig"))
        .arg("nope-not-a-dir")
        .output()
        .unwrap();
    assert!(!o.status.success());
}
