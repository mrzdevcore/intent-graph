use super::*;
use crate::testing::{Fake, env};
use ig_core::ports::Files;
use ig_core::testing::{FakeRunner, FakeVcs, MemFiles};

const PLAN: &str =
    "# P\n\n```ig-plan\nnodes:\n  - id: src/a.ts#f\n    status: new\n    exported: true\n```\n";

fn vcs() -> FakeVcs {
    FakeVcs {
        head: Some("h".into()),
        base: Some("h".into()),
        commits: ["h".to_string()].into(),
        checkout_root: Some(".wt".into()),
        ..FakeVcs::default()
    }
}

#[test]
fn finish_on_stale_state_points_to_abort() {
    let files = MemFiles::with(&[("plans/p.md", PLAN), ("src/a.ts", "")]);
    let (vcs, runner, fe) = (vcs(), FakeRunner::default(), Fake::default());
    let e = env(&files, Some(&vcs), &runner, &fe);
    start(&e, "plans/p.md", PLAN).unwrap();
    assert_eq!(load(&e).unwrap().base, "h");
    files.remove("plans/p.md").unwrap();
    let err = finish(&e, &mut String::new()).unwrap_err().to_string();
    assert_eq!(
        err,
        "stale apply state (plan plans/p.md is missing): run ig apply abort"
    );
    // A check ignores stale state, with a notice.
    let run = check::run(&e, &Opts::default()).unwrap();
    assert!(
        run.result.notices[0].starts_with("apply state is stale"),
        "{:?}",
        run.result.notices
    );
    assert_eq!(abort(&e).unwrap(), "plans/p.md");
    assert!(load(&e).is_none() && abort(&e).is_err());
}

#[test]
fn finish_clears_state_and_marks_the_plan_done() {
    let files = MemFiles::with(&[("plans/p.md", PLAN), ("src/a.ts", ""), (".wt/src/a.ts", "")]);
    let (vcs, runner, fe) = (vcs(), FakeRunner::default(), Fake::default());
    let e = env(&files, Some(&vcs), &runner, &fe);
    start(&e, "plans/p.md", PLAN).unwrap();
    files.write_atomic("src/a.ts", b"fn f\n").unwrap();
    let mut out = String::new();
    assert_eq!(finish(&e, &mut out).unwrap(), 1, "{out}");
    assert!(out.contains("ig/review-missing"), "{out}");
    assert_eq!(review_ingest(&e, "fine: []").unwrap(), 0);
    except(&e, "fp", "user said fine").unwrap();
    let mut out = String::new();
    assert_eq!(finish(&e, &mut out).unwrap(), 0, "{out}");
    assert_eq!(
        out,
        "exceptions added during this apply:\n- fp: user said fine\napply finished\n"
    );
    assert!(load(&e).is_none());
    assert!(
        files
            .read("plans/p.md")
            .unwrap()
            .starts_with("# P\nStatus: done\n")
    );
}

#[test]
fn tamper_is_reported_until_accepted() {
    let files = MemFiles::with(&[("plans/p.md", PLAN), ("ig.yaml", ""), (".wt/ig.yaml", "")]);
    let (vcs, runner, fe) = (vcs(), FakeRunner::default(), Fake::default());
    let e = env(&files, Some(&vcs), &runner, &fe);
    start(&e, "plans/p.md", PLAN).unwrap();
    files.write_atomic("ig.yaml", b"tests: []\n").unwrap();
    let tamper = |e: &Env| {
        check::run(e, &Opts::default())
            .unwrap()
            .result
            .findings
            .into_iter()
            .filter(|f| f.rule_id == "ig/tamper")
            .map(|f| f.file)
            .collect::<Vec<_>>()
    };
    assert_eq!(tamper(&e), ["ig.yaml"]);
    accept(&e).unwrap();
    assert!(tamper(&e).is_empty());
}
