//! `ig review`: structural problems of the whole graph, ranked, saved to `.ig/review.json`.

pub use ig_core::review::*;

use crate::Env;
use ig_core::workspace;

/// The ranked items: the top ones and the rest. Writes `.ig/review.json` (all of them).
pub fn run(env: &Env) -> anyhow::Result<(Vec<ReviewItem>, Vec<ReviewItem>)> {
    let cfg = crate::config::load(env.files)?;
    let repo_head = env.vcs.and_then(|r| r.head());
    let built = crate::graph(env, env.files, &cfg, repo_head)?;
    let tests = workspace::globset(&cfg.tests)?;
    let eps = workspace::globset(&cfg.entry_points)?;
    let ctx = Ctx {
        config: &cfg,
        tests: &tests,
        packages: &built.workspace.packages,
        entry_points: &eps,
    };
    let history = env
        .vcs
        .map(|v| v.history(cfg.review.history_commits))
        .unwrap_or_default();
    let items = review(&built.graph, &ctx, &history);
    let (head, rest) = rank(items, cfg.review.top);
    let all: Vec<&ReviewItem> = head.iter().chain(rest.iter()).collect();
    env.files
        .write_atomic(".ig/review.json", &serde_json::to_vec_pretty(&all)?)?;
    Ok((head, rest))
}

/// The plain-text list: the top items, then (with `all`) the rest, else a pointer to them.
pub fn text(head: &[ReviewItem], rest: &[ReviewItem], all: bool) -> String {
    let mut s = String::new();
    let mut show = |i: usize, it: &ReviewItem| {
        s.push_str(&format!("{}. {} [{}]\n", i + 1, it.title, it.rule_id));
        s.push_str(&format!("   {}\n", it.evidence));
        s.push_str(&format!("   why: {}\n", it.why));
        s.push_str(&format!("   fix: {}\n", it.fix));
    };
    if head.is_empty() {
        return "ig review: no structural problems found\n".into();
    }
    for (i, it) in head.iter().enumerate() {
        show(i, it);
    }
    if all {
        for (i, it) in rest.iter().enumerate() {
            show(head.len() + i, it);
        }
    } else if !rest.is_empty() {
        s.push_str(&format!("\n{} more — ig review --all\n", rest.len()));
    }
    s
}

/// `--agent`: one line per item.
pub fn agent_text(head: &[ReviewItem], rest: &[ReviewItem], all: bool) -> String {
    let shown: Vec<&ReviewItem> = head.iter().chain(rest.iter().filter(|_| all)).collect();
    let mut s = String::new();
    if shown.is_empty() {
        s.push_str("ig review: no structural problems found\n");
    }
    for it in shown {
        s.push_str(&format!(
            "- [{}] {}: {} Fix: {}\n",
            it.rule_id, it.title, it.evidence, it.fix
        ));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{Fake, env};
    use ig_core::ports::Files;
    use ig_core::testing::{FakeRunner, FakeVcs, MemFiles};

    #[test]
    fn history_feeds_the_review_and_the_result_is_saved() {
        // Two modules that always change together but never import each other: hidden coupling.
        let files = MemFiles::with(&[("x/a.ts", "fn a\n"), ("y/b.ts", "fn b\n")]);
        let pair = vec!["x/a.ts".to_string(), "y/b.ts".to_string()];
        let vcs = FakeVcs {
            head: Some("h".into()),
            history: vec![pair; 5],
            ..FakeVcs::default()
        };
        let (runner, fe) = (FakeRunner::default(), Fake::default());
        let e = env(&files, Some(&vcs), &runner, &fe);
        let (head, rest) = run(&e).unwrap();
        let none = run(&env(&files, None, &runner, &fe)).unwrap();
        let coupled =
            |items: &[ReviewItem]| items.iter().any(|i| i.rule_id == "struct/change-coupling");
        assert!(coupled(&head) || coupled(&rest), "{head:?}");
        assert!(!coupled(&none.0) && !coupled(&none.1));
        let saved: Vec<serde_json::Value> =
            serde_json::from_str(&files.read(".ig/review.json").unwrap()).unwrap();
        assert_eq!(saved.len(), none.0.len() + none.1.len());
        assert!(text(&head, &rest, true).starts_with("1. "));
    }
}
