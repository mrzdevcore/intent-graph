//! The base graph: built in a checkout of the base commit, cached in `.ig/cache/{sha}-{key}.json`.

use crate::Env;
use ig_core::config::Config;
use ig_core::model::Graph;
use ig_core::ports::Vcs;
use ig_core::workspace::WorkspaceInfo;

/// The cache key of the working tree (`ig_core::cache::cache_key` over the config and the
/// frontends' `cache_inputs`).
pub fn cache_key(env: &Env, config: &Config) -> String {
    let inputs: Vec<(String, String)> = env
        .frontends
        .iter()
        .flat_map(|fe| fe.cache_inputs(env.root, config))
        .map(|f| {
            let text = crate::read_text(env.files, &f);
            (f, text)
        })
        .collect();
    ig_core::cache::cache_key(&config.graph_key_input(), &inputs)
}

/// The working tree's packages, for `prepare_checkout` (just the root when discovery fails).
pub fn work_info(env: &Env, config: &Config) -> WorkspaceInfo {
    env.files
        .discover(config)
        .unwrap_or_else(|_| WorkspaceInfo {
            root: env.root.to_path_buf(),
            packages: vec![],
            files: vec![],
        })
}

/// The graph of commit `sha`, from the cache or built (and cached).
pub fn graph(env: &Env, vcs: &dyn Vcs, sha: &str, config: &Config) -> crate::Result<Graph> {
    let path = format!(".ig/cache/{sha}-{}.json", cache_key(env, config));
    if let Some(g) = crate::read_json::<Graph>(env.files, &path) {
        return Ok(g);
    }
    (env.progress)(&format!("base graph {}…", &sha[..sha.len().min(7)]));
    let g = graph_at(env, vcs, sha, config)?;
    env.files.write_atomic(&path, &serde_json::to_vec(&g)?)?;
    Ok(g)
}

/// Builds the graph of commit `sha` in a temporary checkout.
fn graph_at(env: &Env, vcs: &dyn Vcs, sha: &str, config: &Config) -> crate::Result<Graph> {
    let wt = vcs.checkout(sha)?;
    let Some(r) = wt.root() else {
        // The ig root did not exist at that commit.
        let mut g = Graph::empty();
        g.commit = Some(sha.to_string());
        return Ok(g);
    };
    let work = work_info(env, config);
    for fe in env.frontends {
        fe.prepare_checkout(&work, r, config, true)?;
    }
    let files = env.files.at(r);
    Ok(crate::graph(env, &*files, config, Some(sha.to_string()))?.graph)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{Fake, env};
    use ig_core::ports::Files;
    use ig_core::testing::{FakeRunner, FakeVcs, MemFiles};

    #[test]
    fn caches_by_sha_and_inputs() {
        // The checkout lives under `wt/`.
        let files = MemFiles::with(&[
            ("src/a.ts", ""),
            ("gen/decl.d.ts", "export {}\n"),
            ("wt/src/b.ts", ""),
        ]);
        let vcs = FakeVcs {
            commits: ["abc".to_string()].into(),
            checkout_root: Some("wt".into()),
            ..FakeVcs::default()
        };
        let runner = FakeRunner::default();
        let fe = Fake::with_inputs(&["gen/decl.d.ts"]);
        let e = env(&files, Some(&vcs), &runner, &fe);
        let config = Config::default();
        let key = cache_key(&e, &config);
        let g = graph(&e, &vcs, "abc", &config).unwrap();
        assert_eq!(g.commit.as_deref(), Some("abc"));
        assert!(g.nodes.iter().any(|n| n.id == "src/b.ts"), "{:?}", g.nodes);
        assert!(files.exists(&format!(".ig/cache/abc-{key}.json")));
        // A cached graph is read back, not rebuilt.
        files
            .write_atomic(
                &format!(".ig/cache/abc-{key}.json"),
                &serde_json::to_vec(&Graph::empty()).unwrap(),
            )
            .unwrap();
        assert!(graph(&e, &vcs, "abc", &config).unwrap().nodes.is_empty());
        // The key follows the inputs' contents.
        files
            .write_atomic("gen/decl.d.ts", b"export { a } from '../a'\n")
            .unwrap();
        assert_ne!(key, cache_key(&e, &config));
    }
}
