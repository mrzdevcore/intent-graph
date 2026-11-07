use super::git_fixture;
use crate::cli::DiffArgs;
use serde_json::Value;
use std::path::Path;

fn git(dir: &Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args([
            "-c",
            "user.email=t@t",
            "-c",
            "user.name=t",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn add_folder(dir: &Path, name: &str) {
    let f = dir.join(format!("apps/api/src/domain/{name}/{name}.ts"));
    std::fs::create_dir_all(f.parent().unwrap()).unwrap();
    std::fs::write(f, format!("export const {name} = 1;\n")).unwrap();
}

fn two_commits() -> (std::path::PathBuf, String, String) {
    let dir = git_fixture("shop");
    let first = git(&dir, &["rev-parse", "HEAD"]);
    add_folder(&dir, "billing");
    git(&dir, &["add", "-A"]);
    git(&dir, &["commit", "-qm", "billing"]);
    let second = git(&dir, &["rev-parse", "HEAD"]);
    (dir, first, second)
}

fn args(base: &str, head: &str, json: bool) -> DiffArgs {
    DiffArgs {
        base: Some(base.into()),
        head: Some(head.into()),
        json,
        no_open: true,
    }
}

fn embedded(html: &str, id: &str) -> Value {
    let open = format!(r#"<script id="{id}" type="application/json">"#);
    let start = html.find(&open).unwrap() + open.len();
    let end = start + html[start..].find("</script>").unwrap();
    serde_json::from_str(&html[start..end]).unwrap()
}

fn box_ids(view: &Value) -> Vec<String> {
    view["boxes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["id"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn diff_writes_the_head_map_and_the_diff() {
    let (dir, first, second) = two_commits();
    assert_eq!(
        crate::diff::run(&dir, &args(&first, &second, false), true).unwrap(),
        0
    );
    let html = std::fs::read_to_string(dir.join(".ig/diff.html")).unwrap();
    let d = embedded(&html, "ig-diff");
    let added: Vec<&str> = d["boxes"]["added"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["id"].as_str().unwrap())
        .collect();
    let billing = added
        .iter()
        .find(|id| id.contains("billing"))
        .expect("billing added");
    assert!(box_ids(&embedded(&html, "ig-data")).contains(&billing.to_string()));
    assert_eq!(embedded(&html, "ig-show"), Value::Null);
}

#[test]
fn diff_json_writes_no_page() {
    let (dir, first, second) = two_commits();
    assert_eq!(
        crate::diff::run(&dir, &args(&first, &second, true), true).unwrap(),
        0
    );
    assert!(!dir.join(".ig/diff.html").exists());
}

#[test]
fn diff_with_a_head_commit_draws_that_commit() {
    let (dir, first, second) = two_commits();
    add_folder(&dir, "shipping");
    assert_eq!(
        crate::diff::run(&dir, &args(&first, &second, false), true).unwrap(),
        0
    );
    let html = std::fs::read_to_string(dir.join(".ig/diff.html")).unwrap();
    let ids = box_ids(&embedded(&html, "ig-data"));
    assert!(ids.iter().any(|id| id.contains("billing")), "{ids:?}");
    assert!(!ids.iter().any(|id| id.contains("shipping")), "{ids:?}");
}

#[test]
fn the_root_is_named_after_the_repo_not_its_checkout() {
    let dir = super::tmp_with(&[("src/a.ts", "export const a = 1;\n")]);
    let dir = dir.canonicalize().unwrap();
    git(&dir, &["init", "-q"]);
    git(&dir, &["add", "-A"]);
    git(&dir, &["commit", "-qm", "one"]);
    let first = git(&dir, &["rev-parse", "HEAD"]);
    std::fs::write(dir.join("src/a.ts"), "export const a = 2;\n").unwrap();
    git(&dir, &["commit", "-qam", "two"]);
    let second = git(&dir, &["rev-parse", "HEAD"]);
    assert_eq!(
        crate::diff::run(&dir, &args(&first, &second, false), true).unwrap(),
        0
    );
    let name = dir.file_name().unwrap().to_string_lossy().to_string();
    let html = std::fs::read_to_string(dir.join(".ig/diff.html")).unwrap();
    let view = embedded(&html, "ig-data");
    let root = view["boxes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["id"] == "system")
        .unwrap();
    assert_eq!(root["title"]["value"], name.as_str());
    let d = embedded(&html, "ig-diff");
    let titles: Vec<&str> = d["boxes"]["changed"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["title"].as_str().unwrap())
        .collect();
    assert!(
        !titles.iter().any(|t| t.starts_with("ig-wt-")),
        "{titles:?}"
    );
}
