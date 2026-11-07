//! The two sides of a diff: the base built in a checkout, the head in a checkout or from the
//! working tree, and the files git says changed between them.

use super::{Diff, Side, SpecSide, diff};
use crate::Env;
use crate::map::file::MapFile;
use crate::map::view::{self, View};
use ig_core::config::Config;
use ig_core::ports::{Checkout, PortError};
use ig_core::workspace::WorkspaceInfo;
use std::path::Path;

pub type ViewAt<'a> = &'a dyn Fn(Option<&Path>) -> crate::Result<View>;

pub fn run(
    env: &Env,
    base: &str,
    head: Option<&str>,
    at: &dyn Fn(&Path, bool, &Config) -> crate::Result<Side>,
) -> crate::Result<Diff> {
    run_with_view(env, base, head, at, None).map(|(d, _)| d)
}

/// The diff and, with `view_at`, the head's map: `view_at(None)` draws the working tree.
pub fn run_with_view(
    env: &Env,
    base: &str,
    head: Option<&str>,
    at: &dyn Fn(&Path, bool, &Config) -> crate::Result<Side>,
    view_at: Option<ViewAt>,
) -> crate::Result<(Diff, Option<View>)> {
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
    let own_head = (head_sha.as_deref())
        .filter(|h| *h != base_sha)
        .map(checkout)
        .transpose()?;
    let head_co = head_sha
        .as_ref()
        .map(|_| own_head.as_ref().unwrap_or(&base_co));
    let config = match head_co.map(Checkout::root) {
        None => crate::config::load(env.files)?,
        Some(Some(root)) => crate::config::load(&*env.files.at(root))?,
        Some(None) => Config::default(),
    };
    let work = work_info(env, &config);
    for co in std::iter::once(&base_co).chain(&own_head) {
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
    let mut h = side(head_co, true)?;
    if let SpecSide::Broken(e) = &h.spec {
        return Err(anyhow::anyhow!("ig/ does not read: {e}").into());
    }
    if !matches!((&b.spec, &h.spec), (SpecSide::Read(_), SpecSide::Read(_))) {
        // a side without an `ig/` that reads is already the map of the code alone
        if matches!(b.spec, SpecSide::Read(_)) {
            b = side(Some(&base_co), false)?;
        }
        if matches!(h.spec, SpecSide::Read(_)) {
            h = side(head_co, false)?;
        }
    }
    let changed = vcs.changed_between(&base_sha, head_sha.as_deref())?;
    let view = match (view_at, head_co.map(Checkout::root)) {
        (None, _) | (_, Some(None)) => None,
        (Some(f), None) => Some(f(None)?),
        (Some(f), Some(Some(root))) => Some(f(Some(root))?),
    };
    Ok((diff(&b, &h, &changed, &base_sha, head_sha.as_deref()), view))
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

fn work_info(env: &Env, config: &Config) -> WorkspaceInfo {
    crate::build::discover(env.frontends, env.files, config).unwrap_or_else(|_| WorkspaceInfo {
        root: env.root.to_path_buf(),
        packages: vec![],
        files: vec![],
    })
}
