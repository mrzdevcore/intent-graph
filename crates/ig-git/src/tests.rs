use super::*;

/// A fresh repository at a unique temp dir, and a `git -c user… <args>` runner for it.
fn repo(name: &str) -> (PathBuf, impl Fn(&[&str]) -> String) {
    let dir = std::env::temp_dir().join(format!("ig-git-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let d = dir.clone();
    let g = move |args: &[&str]| {
        git(
            &d,
            &[&["-c", "user.email=t@t", "-c", "user.name=t"][..], args].concat(),
        )
        .unwrap()
    };
    g(&["init", "-q", "-b", "trunk"]);
    (dir, g)
}

#[test]
fn git_vcs_meets_the_contract() {
    let (dir, g) = repo("contract");
    std::fs::create_dir_all(dir.join("app")).unwrap();
    std::fs::write(dir.join("app/intents.md"), "# I\n").unwrap();
    g(&["add", "-A"]);
    g(&["commit", "-qm", "one"]);
    // An ig root below the top level: paths are relative to it.
    let v = GitVcs::open(&dir.join("app")).unwrap();
    ig_core::testing::vcs_contract(&v, "intents.md", "# I\n");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn checkout_without_the_ig_root() {
    let (dir, g) = repo("noroot");
    g(&["commit", "-q", "--allow-empty", "-m", "one"]);
    let one = g(&["rev-parse", "HEAD"]);
    std::fs::create_dir_all(dir.join("app")).unwrap();
    std::fs::write(dir.join("app/a.ts"), "").unwrap();
    g(&["add", "-A"]);
    g(&["commit", "-qm", "two"]);
    let v = GitVcs::open(&dir.join("app")).unwrap();
    let c = v.checkout(&one).unwrap();
    assert_eq!(c.root(), None);
    let c = v.checkout("HEAD").unwrap();
    let wt = c.root().unwrap().to_path_buf();
    assert!(wt.join("a.ts").is_file());
    drop(c);
    assert!(!wt.exists(), "removed on drop");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn changed_files_are_relative_and_unfiltered() {
    let (dir, g) = repo("changed");
    std::fs::create_dir_all(dir.join("app")).unwrap();
    std::fs::write(dir.join("app/gone.ts"), "").unwrap();
    std::fs::write(dir.join("app/kept.ts"), "").unwrap();
    std::fs::write(dir.join("outside.ts"), "").unwrap();
    g(&["add", "-A"]);
    g(&["commit", "-qm", "one"]);
    let base = g(&["rev-parse", "HEAD"]);
    std::fs::remove_file(dir.join("app/gone.ts")).unwrap();
    std::fs::write(dir.join("app/kept.ts"), "x").unwrap();
    std::fs::write(dir.join("app/new.ts"), "").unwrap();
    std::fs::write(dir.join("outside.ts"), "x").unwrap();
    let v = GitVcs::open(&dir.join("app")).unwrap();
    let mut notices = vec![];
    let mut files = v.changed_files(Some(&base), &mut notices);
    files.sort();
    assert_eq!(files, ["gone.ts", "kept.ts", "new.ts"]);
    assert!(notices.is_empty());
    assert_eq!(v.changed_files(None, &mut notices), ["new.ts"]);
    v.changed_files(Some("nope"), &mut notices);
    assert!(notices[0].starts_with("changed files for checkers: git diff"));
    assert!(!v.is_clean());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn base_resolution() {
    let (dir, g) = repo("base");
    let repo = GitVcs::open(&dir).unwrap();
    // an MR pipeline sets CI_MERGE_REQUEST_DIFF_BASE_SHA, which these repos never hold
    let own = |n: &[String]| -> Vec<String> {
        n.iter()
            .filter(|s| !s.starts_with("CI_MERGE_REQUEST_DIFF_BASE_SHA"))
            .cloned()
            .collect()
    };
    let mut notices = vec![];
    assert_eq!(repo.resolve_base(None, &mut notices).unwrap(), None);
    assert!(own(&notices)[0].contains("no commits"));
    g(&["commit", "-q", "--allow-empty", "-m", "one"]);
    let one = repo.head().unwrap();
    notices.clear();
    assert_eq!(
        repo.resolve_base(None, &mut notices).unwrap(),
        Some(one.clone())
    );
    assert!(own(&notices)[0].contains("comparing against HEAD"));
    g(&["branch", "main"]);
    g(&["commit", "-q", "--allow-empty", "-m", "two"]);
    notices.clear();
    assert_eq!(
        repo.resolve_base(None, &mut notices).unwrap(),
        Some(one.clone()),
        "merge-base with main"
    );
    assert!(own(&notices).is_empty());
    assert_eq!(
        repo.resolve_base(Some("HEAD"), &mut notices).unwrap(),
        repo.head()
    );
    assert!(repo.resolve_base(Some("nope"), &mut notices).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn history_from_git() {
    let (dir, g) = repo("history");
    std::fs::create_dir_all(dir.join("src/a")).unwrap();
    std::fs::create_dir_all(dir.join("src/b")).unwrap();
    for i in 0..5 {
        std::fs::write(dir.join("src/a/x.ts"), format!("// {i}")).unwrap();
        std::fs::write(dir.join("src/b/y.ts"), format!("// {i}")).unwrap();
        g(&["add", "-A"]);
        g(&["commit", "-qm", "c"]);
    }
    let v = GitVcs::open(&dir).unwrap();
    let commits: Vec<Vec<String>> = v
        .history(100)
        .into_iter()
        .filter(|c| !c.is_empty())
        .collect();
    assert_eq!(commits, vec![vec!["src/a/x.ts", "src/b/y.ts"]; 5]);
    assert_eq!(v.history(2).iter().filter(|c| !c.is_empty()).count(), 2);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(v.at(&dir).history(100).is_empty());
}

#[test]
fn show_dir_reads_the_files_of_a_folder_at_a_commit_in_one_batch() {
    let (dir, g) = repo("showdir");
    for (p, t) in [
        ("app/ig/system.kdl", "system\n"),
        ("app/ig/api.kdl", "api\n\nwith a blank line"),
        ("app/ig/deeper/x.kdl", "no\n"),
        ("app/other.kdl", "no\n"),
    ] {
        std::fs::create_dir_all(dir.join(p).parent().unwrap()).unwrap();
        std::fs::write(dir.join(p), t).unwrap();
    }
    g(&["add", "-A"]);
    g(&["commit", "-qm", "one"]);
    // staged after the commit: HEAD still has the old name
    g(&["mv", "app/ig/api.kdl", "app/ig/backend.kdl"]);
    let v = GitVcs::open(&dir.join("app")).unwrap();
    let head = v.head().unwrap();
    assert_eq!(
        v.show_dir(&head, "ig"),
        [
            (
                "ig/api.kdl".to_string(),
                "api\n\nwith a blank line".to_string()
            ),
            ("ig/system.kdl".to_string(), "system\n".to_string()),
        ]
    );
    assert!(v.show_dir(&head, "nope").is_empty());
    assert!(
        v.show_dir("0123456789abcdef0123456789abcdef01234567", "ig")
            .is_empty()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn changed_between_lists_paths_between_commits_and_the_working_tree() {
    let (dir, g) = repo("between");
    std::fs::create_dir_all(dir.join("app/src")).unwrap();
    std::fs::write(dir.join("app/src/a.ts"), "").unwrap();
    std::fs::write(dir.join("app/gone.ts"), "").unwrap();
    std::fs::write(dir.join("outside.ts"), "").unwrap();
    g(&["add", "-A"]);
    g(&["commit", "-qm", "one"]);
    let one = g(&["rev-parse", "HEAD"]);
    std::fs::write(dir.join("app/src/a.ts"), "x").unwrap();
    std::fs::remove_file(dir.join("app/gone.ts")).unwrap();
    std::fs::write(dir.join("outside.ts"), "x").unwrap();
    g(&["add", "-A"]);
    g(&["commit", "-qm", "two"]);
    let two = g(&["rev-parse", "HEAD"]);
    std::fs::write(dir.join("app/b.ts"), "").unwrap();
    std::fs::write(dir.join("app/src/a.ts"), "y").unwrap();
    let v = GitVcs::open(&dir.join("app")).unwrap();
    assert_eq!(
        v.changed_between(&one, Some(&two)).unwrap(),
        ["gone.ts", "src/a.ts"]
    );
    assert_eq!(v.changed_between(&two, Some(&two)).unwrap(), [""; 0]);
    assert_eq!(v.changed_between(&two, None).unwrap(), ["b.ts", "src/a.ts"]);
    assert_eq!(
        v.changed_between(&one, None).unwrap(),
        ["b.ts", "gone.ts", "src/a.ts"]
    );
    assert!(v.changed_between("nope", None).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

fn one_commit(name: &str) -> (PathBuf, impl Fn(&[&str]) -> String) {
    let (dir, g) = repo(name);
    std::fs::write(dir.join("a.ts"), "").unwrap();
    g(&["add", "-A"]);
    g(&["commit", "-qm", "one"]);
    (dir, g)
}

fn hook(dir: &Path, body: &str) {
    let h = dir.join(".git/hooks/post-checkout");
    std::fs::write(&h, format!("#!/bin/sh\n{body}\n")).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&h, std::fs::Permissions::from_mode(0o755)).unwrap();
}

#[test]
fn checkout_runs_no_hooks() {
    let (dir, _g) = one_commit("hooks");
    let marker = dir.join("marker");
    hook(&dir, &format!("touch '{}'", marker.display()));
    let v = GitVcs::open(&dir).unwrap();
    drop(v.checkout("HEAD").unwrap());
    assert!(!marker.exists(), "post-checkout ran");
    hook(&dir, "exit 1");
    assert!(v.checkout("HEAD").is_ok());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_failed_checkout_leaves_no_worktree() {
    let (dir, g) = one_commit("failed");
    std::fs::write(dir.join(".gitattributes"), "*.ts filter=broken\n").unwrap();
    g(&["add", "-A"]);
    g(&["commit", "-qm", "two"]);
    g(&["config", "filter.broken.smudge", "false"]);
    g(&["config", "filter.broken.required", "true"]);
    let v = GitVcs::open(&dir).unwrap();
    let Err(e) = v.checkout("HEAD") else {
        panic!("checkout succeeded")
    };
    assert!(!e.to_string().trim().is_empty());
    assert_eq!(g(&["worktree", "list"]).lines().count(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_killed_runs_worktree_is_pruned() {
    let (dir, g) = one_commit("stale");
    let mut child = Command::new("true").spawn().unwrap();
    let dead = child.id();
    child.wait().unwrap();
    let stale = std::env::temp_dir().join(format!("ig-wt-{dead}-0"));
    let other = std::env::temp_dir().join(format!("ig-wtx-{dead}-0"));
    for d in [&stale, &other] {
        let _ = std::fs::remove_dir_all(d);
        g(&[
            "worktree",
            "add",
            "--detach",
            "--quiet",
            &d.to_string_lossy(),
            "HEAD",
        ]);
    }
    let v = GitVcs::open(&dir).unwrap();
    let c = v.checkout("HEAD").unwrap();
    let list = g(&["worktree", "list"]);
    assert_eq!(list.lines().count(), 3, "{list}");
    assert!(!stale.exists());
    assert!(other.exists());
    drop(c);
    g(&["worktree", "remove", "--force", &other.to_string_lossy()]);
    let _ = std::fs::remove_dir_all(&dir);
}
