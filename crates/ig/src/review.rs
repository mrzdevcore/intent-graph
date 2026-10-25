//! `ig_core::review` plus reading the git history it needs and printing the result.

pub use ig_core::review::*;

use std::path::Path;

/// The files each of the last `n` commits changed (`git log --relative`), newest first; empty
/// without git.
pub fn history(root: &Path, n: usize) -> Vec<Vec<String>> {
    let n = n.to_string();
    let Ok(log) = crate::git::git(
        root,
        &[
            "log",
            "--relative",
            "--name-only",
            "--format=%x00",
            "-n",
            &n,
        ],
    ) else {
        return vec![];
    };
    log.split('\0')
        .map(|commit| {
            commit
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(String::from)
                .collect()
        })
        .collect()
}

pub fn print(head: &[ReviewItem], rest: usize, all: bool, rest_items: &[ReviewItem]) {
    let show = |i: usize, it: &ReviewItem| {
        println!("{}. {} [{}]", i + 1, it.title, it.rule_id);
        println!("   {}", it.evidence);
        println!("   why: {}", it.why);
        println!("   fix: {}", it.fix);
    };
    if head.is_empty() {
        println!("ig review: no structural problems found");
        return;
    }
    for (i, it) in head.iter().enumerate() {
        show(i, it);
    }
    if all {
        for (i, it) in rest_items.iter().enumerate() {
            show(head.len() + i, it);
        }
    } else if rest > 0 {
        println!("\n{rest} more — ig review --all");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_from_git() {
        let dir = std::env::temp_dir().join(format!("ig-coupling-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src/a")).unwrap();
        std::fs::create_dir_all(dir.join("src/b")).unwrap();
        let git = |args: &[&str]| {
            let ok = std::process::Command::new("git")
                .arg("-C")
                .arg(&dir)
                .args(["-c", "user.email=t@t", "-c", "user.name=t"])
                .args(args)
                .output()
                .unwrap();
            assert!(
                ok.status.success(),
                "{}",
                String::from_utf8_lossy(&ok.stderr)
            );
        };
        git(&["init", "-q"]);
        for i in 0..5 {
            std::fs::write(dir.join("src/a/x.ts"), format!("// {i}")).unwrap();
            std::fs::write(dir.join("src/b/y.ts"), format!("// {i}")).unwrap();
            git(&["add", "-A"]);
            git(&["commit", "-qm", "c"]);
        }
        let commits: Vec<Vec<String>> = history(&dir, 100)
            .into_iter()
            .filter(|c| !c.is_empty())
            .collect();
        assert_eq!(commits, vec![vec!["src/a/x.ts", "src/b/y.ts"]; 5]);
        assert_eq!(history(&dir, 2).iter().filter(|c| !c.is_empty()).count(), 2);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(history(&dir, 100).is_empty());
    }
}
