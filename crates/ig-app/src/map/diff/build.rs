//! The two sides of a diff: the base built in a checkout, the head in a checkout or from the
//! working tree, and the files git says changed between them.

use super::{Diff, Side, SpecSide, diff};
use crate::Env;
use crate::map::file::MapFile;
use crate::map::view;
use ig_core::config::Config;
use ig_core::ports::{Checkout, PortError};
use ig_core::workspace::WorkspaceInfo;
use std::path::Path;

/// The diff of `base` against `head` (the working tree when None), both built with the head's
/// ig.yaml. `at(root, use_spec, config)` builds the side of a checkout's ig root.
pub fn run(
    env: &Env,
    base: &str,
    head: Option<&str>,
    at: &dyn Fn(&Path, bool, &Config) -> crate::Result<Side>,
) -> crate::Result<Diff> {
    let vcs = env
        .vcs
        .ok_or_else(|| PortError::Vcs("not a git repository".into()))?;
    let verify =
        |rev: &str| (vcs.verify(rev)).ok_or_else(|| PortError::Vcs(format!("{rev}: not a commit")));
    let base_sha = verify(base)?;
    let head_sha = head.map(verify).transpose()?;
    let checkout = |sha: &str| {
        (env.progress)(&format!("checking out {}…", &sha[..sha.len().min(7)]));
        vcs.checkout(sha)
    };
    let base_co = checkout(&base_sha)?;
    let head_co = head_sha.as_deref().map(checkout).transpose()?;
    let config = match head_co.as_ref().map(Checkout::root) {
        None => crate::config::load(env.files)?,
        Some(Some(root)) => crate::config::load(&*env.files.at(root))?,
        Some(None) => Config::default(),
    };
    let work = work_info(env, &config);
    for co in std::iter::once(&base_co).chain(&head_co) {
        if let Some(root) = co.root() {
            for fe in env.frontends {
                fe.prepare_checkout(&work, root, &config, true)?;
            }
        }
    }
    let side = |co: Option<&Checkout>, use_spec: bool| match co.map(Checkout::root) {
        None => view::architecture(env, use_spec, Some(&config)),
        Some(None) => Ok(empty()),
        Some(Some(root)) => at(root, use_spec, &config),
    };
    let mut b = side(Some(&base_co), true)?;
    let mut h = side(head_co.as_ref(), true)?;
    if let SpecSide::Broken(e) = &h.spec {
        return Err(anyhow::anyhow!("ig/ does not read: {e}").into());
    }
    if !matches!((&b.spec, &h.spec), (SpecSide::Read(_), SpecSide::Read(_))) {
        // a side without an `ig/` that reads is already the map of the code alone
        if matches!(b.spec, SpecSide::Read(_)) {
            b = side(Some(&base_co), false)?;
        }
        if matches!(h.spec, SpecSide::Read(_)) {
            h = side(head_co.as_ref(), false)?;
        }
    }
    let changed = vcs.changed_between(&base_sha, head_sha.as_deref())?;
    Ok(diff(&b, &h, &changed, &base_sha, head_sha.as_deref()))
}

fn empty() -> Side {
    Side {
        arch: MapFile {
            version: 1,
            boxes: vec![],
            arrows: vec![],
            marks: vec![],
        },
        spec: SpecSide::Missing,
    }
}

/// The working tree's packages, for `prepare_checkout` (just the root when discovery fails).
fn work_info(env: &Env, config: &Config) -> WorkspaceInfo {
    crate::build::discover(env.frontends, env.files, config).unwrap_or_else(|_| WorkspaceInfo {
        root: env.root.to_path_buf(),
        packages: vec![],
        files: vec![],
    })
}
