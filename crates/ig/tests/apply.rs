#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

const BILLING: &str = r#"service "billing" title="Billing" {
    at "billing/**"
    calls "tiny" label="asks for prices"
}
"#;

fn repo(tag: &str, billing: &str) -> (PathBuf, PathBuf) {
    let base = std::env::temp_dir().join(format!("ig-apply-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let root = base.join("repo");
    for (p, s) in [
        ("src/a/a.ts", "export function f() {}\n"),
        (
            "ig/system.kdl",
            "system \"system\" title=\"tiny\" {\n    at \"**\"\n}\n",
        ),
        (
            "ig/tiny.kdl",
            "module \"tiny\" title=\"Tiny\" {\n    at \"src/**\"\n}\n",
        ),
        ("ig/billing.kdl", billing),
    ] {
        if s.is_empty() {
            continue;
        }
        let f = root.join(p);
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(f, s).unwrap();
    }
    std::fs::create_dir_all(base.join("bin")).unwrap();
    (root, base.join("bin"))
}

fn fake_claude(bin: &Path, builds: &str) {
    let d = bin.display();
    let script = format!(
        "#!/bin/sh\nfor a in \"$@\"; do printf '%s\\n' \"$a\"; done > {d}/argv\n\
         /bin/cat > {d}/prompt\n{builds}\necho 'Built it.'\n"
    );
    let p = bin.join("claude");
    std::fs::write(&p, script).unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
}

const BUILDS_BILLING: &str = "/bin/mkdir -p billing\n\
    printf 'import { f } from \"../src/a/a\";\\nexport const x = f();\\n' > billing/index.ts";

fn ig(root: &Path, path: &Path, args: &[&str]) -> (i32, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_ig"))
        .args(args)
        .arg("-C")
        .arg(root)
        .env("PATH", path)
        .output()
        .unwrap();
    (
        out.status.code().unwrap(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn nothing_to_build_starts_no_agent() {
    let (root, bin) = repo("nothing", "");
    fake_claude(&bin, "");
    let (code, out, err) = ig(&root, &bin, &["apply", "--yes"]);
    assert_eq!(code, 0, "{out}{err}");
    assert_eq!(out, "ig apply: nothing to build\n");
    assert!(!bin.join("argv").exists());
    let _ = std::fs::remove_dir_all(root.parent().unwrap());
}

#[test]
fn without_a_terminal_or_yes_it_lists_and_stops() {
    let (root, bin) = repo("ask", BILLING);
    fake_claude(&bin, "");
    let (code, out, err) = ig(&root, &bin, &["apply"]);
    assert_eq!(code, 2, "{out}{err}");
    assert_eq!(
        out,
        "ig apply: 2 to build\n  billing->tiny  to build: billing calls tiny\n  billing        to build: service billing (billing/**)\n"
    );
    assert!(err.contains("pass --yes"), "{err}");
    assert!(!bin.join("argv").exists());
    let _ = std::fs::remove_dir_all(root.parent().unwrap());
}

#[test]
fn yes_runs_the_agent_on_the_named_arrow_and_exits_1_while_it_is_left() {
    let (root, bin) = repo("arrow", BILLING);
    fake_claude(&bin, "");
    let (code, out, err) = ig(&root, &bin, &["apply", "--yes", "billing->tiny"]);
    assert_eq!(code, 1, "{out}{err}");
    assert!(out.contains("Built it.\n\nig plan: 2 to build"), "{out}");
    let prompt = std::fs::read_to_string(bin.join("prompt")).unwrap();
    assert!(
        prompt
            .contains("- `billing->tiny`: to build: billing calls tiny\n  label: asks for prices"),
        "{prompt}"
    );
    assert!(!prompt.contains("- `billing`:"), "{prompt}");
    let argv = std::fs::read_to_string(bin.join("argv")).unwrap();
    for want in [
        "--restricted\n",
        "--strict-mcp-config\n",
        "Edit(ig/**)\n",
        "Write(ig.yaml)\n",
        "Read(**/.env*)\n",
    ] {
        assert!(argv.contains(want), "{want} not in {argv}");
    }
    assert!(!argv.contains("Bash"), "{argv}");
    let _ = std::fs::remove_dir_all(root.parent().unwrap());
}

#[test]
fn exit_0_when_every_chosen_item_is_built() {
    let (root, bin) = repo("built", BILLING);
    fake_claude(&bin, BUILDS_BILLING);
    let (code, out, err) = ig(&root, &bin, &["apply", "--yes", "--allow-bash"]);
    assert_eq!(code, 0, "{out}{err}");
    assert!(
        out.contains("Built it.\n\nig plan: 0 to build, 0 drift"),
        "{out}"
    );
    let argv = std::fs::read_to_string(bin.join("argv")).unwrap();
    assert!(
        argv.contains("Edit,Write,mcp__ig__ig_plan,mcp__ig__ig_state,Bash\n"),
        "{argv}"
    );
    let _ = std::fs::remove_dir_all(root.parent().unwrap());
}

#[test]
fn an_unknown_id_lists_the_plan_s_ids() {
    let (root, bin) = repo("unknown", BILLING);
    fake_claude(&bin, "");
    let (code, out, err) = ig(&root, &bin, &["apply", "--yes", "web->api"]);
    assert_eq!(code, 2, "{out}{err}");
    assert!(
        err.contains("not in the plan: web->api (the plan's ids: billing->tiny, billing;"),
        "{err}"
    );
    assert!(!bin.join("argv").exists());
    let _ = std::fs::remove_dir_all(root.parent().unwrap());
}

#[test]
fn apply_refuses_other_agents_and_a_repo_without_ig() {
    let (root, bin) = repo("codex", BILLING);
    fake_claude(&bin, "");
    let (code, _, err) = ig(&root, &bin, &["apply", "--yes", "--agent", "codex"]);
    assert_eq!(code, 2, "{err}");
    assert!(err.contains("apply needs claude for now"), "{err}");
    std::fs::remove_dir_all(root.join("ig")).unwrap();
    let (code, _, err) = ig(&root, &bin, &["apply", "--yes"]);
    assert_eq!(code, 2, "{err}");
    assert!(err.contains("run `ig analyse`"), "{err}");
    assert!(!bin.join("argv").exists());
    let _ = std::fs::remove_dir_all(root.parent().unwrap());
}
