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
    let mut notices = vec![];
    assert_eq!(repo.resolve_base(None, &mut notices).unwrap(), None);
    assert!(notices[0].contains("no commits"));
    g(&["commit", "-q", "--allow-empty", "-m", "one"]);
    let one = repo.head().unwrap();
    notices.clear();
    assert_eq!(
        repo.resolve_base(None, &mut notices).unwrap(),
        Some(one.clone())
    );
    assert!(notices[0].contains("comparing against HEAD"));
    g(&["branch", "main"]);
    g(&["commit", "-q", "--allow-empty", "-m", "two"]);
    notices.clear();
    assert_eq!(
        repo.resolve_base(None, &mut notices).unwrap(),
        Some(one.clone()),
        "merge-base with main"
    );
    assert!(notices.is_empty());
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
