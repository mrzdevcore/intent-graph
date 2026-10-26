//! Use cases over the real adapters (temporary trees and git repos), and their helpers.

mod apply;
mod base;
mod brief;
mod graph;
mod plan;
mod setup;
mod view;

use ig_app::build::Built;
use ig_app::check::{self, Opts, Run};
use ig_core::config::Config;
use ig_wire::Wired;
use std::path::{Path, PathBuf};

pub(crate) const PLAN: &str = "# Export\n\n## Steps\n1. core: add export\n\n```ig-plan\nnodes:\n  - id: packages/core/src/export.ts#exportRows\n    status: new\n    exported: true\n  - id: packages/core/src/repo.ts#Repo.find\n    status: changed\nedges:\n  - from: packages/core/src/export.ts#exportRows\n    to: packages/core/src/repo.ts#Repo.find\n```\n";

/// An `Env` for `dir`; its adapters are leaked (tests only).
pub(crate) fn env(dir: &Path) -> ig_app::Env<'static> {
    Box::leak(Box::new(Wired::new(dir))).env()
}

/// The graph of the tree at `root`.
pub(crate) fn build(root: &Path, config: &Config, commit: Option<String>) -> anyhow::Result<Built> {
    let w = Wired::new(root);
    let env = w.env();
    Ok(ig_app::graph(&env, env.files, config, commit)?)
}

pub(crate) fn load(root: &Path) -> anyhow::Result<Config> {
    Ok(ig_app::config::load(&ig_fs::FsFiles::new(root))?)
}

pub(crate) fn check_run(root: &Path, opts: &Opts) -> anyhow::Result<Run> {
    Ok(check::run(&Wired::new(root).env(), opts)?)
}

/// `ig plan check` on the plan at `path`.
pub(crate) fn plan_check(root: &Path, path: &Path) -> anyhow::Result<serde_json::Value> {
    let (rel, text) = ig_wire::read_plan(root, path)?;
    Ok(ig_app::plan::check(&Wired::new(root).env(), &rel, &text)?)
}

pub(crate) fn fixture_dir(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

static TMP: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// A fresh temporary directory holding `files`.
pub(crate) fn tmp_with(files: &[(&str, &str)]) -> PathBuf {
    let n = TMP.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("ig-auto-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    for (p, s) in files {
        let f = dir.join(p);
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(f, s).unwrap();
    }
    dir
}

/// A scratch copy of the monorepo fixture, outside git.
pub(crate) fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ig-view-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        std::process::Command::new("cp")
            .arg("-R")
            .arg(fixture_dir("monorepo"))
            .arg(&dir)
            .status()
            .unwrap()
            .success()
    );
    dir.canonicalize().unwrap()
}

/// A copy of fixture `name` committed to a fresh git repo.
pub(crate) fn git_fixture(name: &str) -> PathBuf {
    static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("ig-apply-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let sh = |cmd: &mut std::process::Command| assert!(cmd.status().unwrap().success());
    sh(std::process::Command::new("cp")
        .arg("-R")
        .arg(fixture_dir(name))
        .arg(&dir));
    for args in [
        &["init", "-q"][..],
        &["add", "-A"],
        &[
            "-c",
            "commit.gpgsign=false",
            "-c",
            "user.email=t@t",
            "-c",
            "user.name=t",
            "commit",
            "-qm",
            "base",
        ],
    ] {
        sh(std::process::Command::new("git")
            .arg("-C")
            .arg(&dir)
            .args(args));
    }
    dir.canonicalize().unwrap() // like main: plan paths are made root-relative
}

pub(crate) fn write(dir: &Path, rel: &str, text: &str) {
    let p = dir.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, text).unwrap();
}
