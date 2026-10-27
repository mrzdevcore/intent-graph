//! Setup facts: what an agent needs to write ig.yaml, as JSON and as a Markdown report.

use crate::Env;
use crate::build::Built;
use ig_core::config::Config;
use ig_core::rules::{self, Layers};
use ig_core::workspace::globset;
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap, HashSet};

const NODE_BUILTINS: &[&str] = &[
    "assert",
    "async_hooks",
    "buffer",
    "child_process",
    "cluster",
    "crypto",
    "dns",
    "events",
    "fs",
    "http",
    "http2",
    "https",
    "module",
    "net",
    "os",
    "path",
    "perf_hooks",
    "process",
    "querystring",
    "readline",
    "stream",
    "string_decoder",
    "timers",
    "tls",
    "tty",
    "url",
    "util",
    "v8",
    "vm",
    "worker_threads",
    "zlib",
];

const PAGE_DIRS: &[&str] = &["pages", "views", "routes", "screens"];

fn top_dirs(files: impl Iterator<Item = String>, depth: usize) -> Vec<(String, usize)> {
    let mut m: BTreeMap<String, usize> = BTreeMap::new();
    for f in files {
        let parts: Vec<&str> = f.split('/').collect();
        let d = parts[..parts.len().saturating_sub(1).min(depth)].join("/");
        *m.entry(if d.is_empty() { ".".into() } else { d })
            .or_default() += 1;
    }
    let mut v: Vec<(String, usize)> = m.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    v
}

/// Collects the facts as JSON (also the source of the Markdown report).
pub fn facts(env: &Env) -> crate::Result<Value> {
    let (config, config_error) = match crate::config::load(env.files) {
        Ok(c) => (c, None),
        Err(e) => (Config::default(), Some(e.to_string())),
    };
    let Built {
        graph: g,
        workspace: ws,
        extras,
    } = crate::graph(env, env.files, &config, None)?;
    let tests = globset(&config.tests)?;
    let modules: Vec<&str> = g
        .nodes
        .iter()
        .filter(|n| n.is_module() && !tests.is_match(&n.file))
        .map(|n| n.file.as_str())
        .collect();

    let packages: Vec<Value> = ws
        .packages
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let n = ws.files.iter().filter(|f| f.package == i).count();
            json!({"name": p.name, "dir": p.dir, "files": n, "sourceEntry": p.source_entry, "tsconfig": p.tsconfig.is_some()})
        })
        .collect();

    let layers = Layers::new(&config, &ws.packages);
    let unlayered: Vec<String> = modules
        .iter()
        .filter(|m| layers.of(m).is_none())
        .map(|m| m.to_string())
        .collect();

    // Directories named like page or view folders, with their module counts.
    let mut page_dirs: BTreeMap<String, usize> = BTreeMap::new();
    for m in &modules {
        let parts: Vec<&str> = m.split('/').collect();
        if let Some(i) = parts.iter().position(|p| PAGE_DIRS.contains(p))
            && i + 1 < parts.len()
        {
            *page_dirs.entry(parts[..=i].join("/")).or_default() += 1;
        }
    }

    let ws_names: HashSet<&str> = ws.packages.iter().map(|p| p.name.as_str()).collect();
    let mapped: HashMap<&str, &str> = config
        .blueprint
        .systems
        .iter()
        .flat_map(|(name, pkgs)| pkgs.iter().map(move |p| (p.as_str(), name.as_str())))
        .collect();
    let mut external: Vec<(&String, &usize)> = extras
        .packages
        .iter()
        .filter(|(p, _)| !ws_names.contains(p.as_str()) && !NODE_BUILTINS.contains(&p.as_str()))
        .collect();
    external.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    let external: Vec<Value> = external
        .iter()
        .map(|(p, n)| json!({"package": p, "modules": n, "system": mapped.get(p.as_str())}))
        .collect();

    let mut url_callees: Vec<(&String, &u32)> = extras.url_callees.iter().collect();
    url_callees.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));

    let mut kinds: BTreeMap<String, usize> = BTreeMap::new();
    for e in &g.edges {
        *kinds
            .entry(format!("{}/{}", e.kind, e.confidence))
            .or_default() += 1;
    }
    let mut node_kinds: BTreeMap<&str, usize> = BTreeMap::new();
    for n in &g.nodes {
        *node_kinds.entry(n.kind.as_str()).or_default() += 1;
    }
    let mut arch: BTreeMap<String, usize> = BTreeMap::new();
    for f in rules::arch(&g, &config, &ws.packages) {
        let rule = f.message.split(':').next().unwrap_or("").to_string();
        *arch.entry(rule).or_default() += 1;
    }

    Ok(json!({
        "config": {
            "exists": env.files.exists("ig.yaml"),
            "error": config_error,
            "layers": config.layers.len(),
            "rules": config.rules.len(),
            "patterns": config.patterns.len(),
            "embedded": config.embedded,
            "blueprint": {
                "httpCalls": config.blueprint.http_calls,
                "systems": config.blueprint.systems.len(),
            },
        },
        "packages": packages,
        "directories": top_dirs(modules.iter().map(|m| m.to_string()), 3).into_iter().take(40).map(|(d, n)| json!({"dir": d, "modules": n})).collect::<Vec<_>>(),
        "pageDirs": page_dirs.into_iter().map(|(d, n)| json!({"dir": d, "modules": n})).collect::<Vec<_>>(),
        "unlayered": {
            "count": unlayered.len(),
            "dirs": top_dirs(unlayered.into_iter(), 3).into_iter().take(25).map(|(d, n)| json!({"dir": d, "modules": n})).collect::<Vec<_>>(),
        },
        "externalPackages": external,
        "urlCallees": url_callees.iter().take(25).map(|(k, v)| json!({"callee": k, "calls": v})).collect::<Vec<_>>(),
        "http": {
            "routes": node_kinds.get(config.blueprint.route_kind.as_str()).copied().unwrap_or(0),
            "linked": g.edges.iter().filter(|e| e.kind == "http").count(),
            "unmatched": extras.http_unmatched.len(),
            "unmatchedSamples": extras.http_unmatched.iter().take(20).map(|(f, l, m, u)| json!({"file": f, "line": l, "method": m, "url": u})).collect::<Vec<_>>(),
        },
        "graph": {"nodes": node_kinds, "edges": kinds, "parseErrors": g.parse_errors.iter().take(15).collect::<Vec<_>>()},
        "archViolations": arch,
    }))
}

fn table(out: &mut String, rows: &[Value], cols: &[(&str, &str)], limit: usize) {
    if rows.is_empty() {
        out.push_str("(none)\n");
        return;
    }
    out.push_str(&format!(
        "| {} |\n",
        cols.iter().map(|c| c.1).collect::<Vec<_>>().join(" | ")
    ));
    out.push_str(&format!("|{}\n", "---|".repeat(cols.len())));
    for r in rows.iter().take(limit) {
        let cells: Vec<String> = cols
            .iter()
            .map(|(k, _)| match &r[*k] {
                Value::Null => "".into(),
                Value::String(s) => s.clone(),
                v => v.to_string(),
            })
            .collect();
        out.push_str(&format!("| {} |\n", cells.join(" | ")));
    }
    if rows.len() > limit {
        out.push_str(&format!("…and {} more\n", rows.len() - limit));
    }
}

pub fn report(f: &Value) -> String {
    let mut s = String::from(
        "# ig setup\n\nFacts for writing ig.yaml.\n\n",
    );
    let c = &f["config"];
    s.push_str("## Config\n\n");
    if !c["exists"].as_bool().unwrap_or(false) {
        s.push_str("ig.yaml does not exist (run `ig init`).\n\n");
    } else if let Some(e) = c["error"].as_str() {
        s.push_str(&format!("ig.yaml is invalid: {e}\n\n"));
    } else {
        let b = &c["blueprint"];
        s.push_str(&format!(
            "{} layers, {} rules, {} patterns, embedded {}. Blueprint: http.calls {}, {} systems.\n\n",
            c["layers"], c["rules"], c["patterns"], c["embedded"], b["httpCalls"], b["systems"]
        ));
    }
    s.push_str("## Workspace packages\n\n");
    table(
        &mut s,
        f["packages"].as_array().unwrap(),
        &[
            ("name", "package"),
            ("dir", "dir"),
            ("files", "files"),
            ("sourceEntry", "entry"),
        ],
        80,
    );
    s.push_str("\n## Directories (modules, depth 3)\n\n");
    table(
        &mut s,
        f["directories"].as_array().unwrap(),
        &[("dir", "dir"), ("modules", "modules")],
        40,
    );
    s.push_str("\n## Page and view directories (journey candidates)\n\n");
    table(
        &mut s,
        f["pageDirs"].as_array().unwrap(),
        &[("dir", "dir"), ("modules", "modules")],
        40,
    );
    let u = &f["unlayered"];
    s.push_str(&format!("\n## Modules without a layer: {}\n\n", u["count"]));
    table(
        &mut s,
        u["dirs"].as_array().unwrap(),
        &[("dir", "dir"), ("modules", "modules")],
        25,
    );
    s.push_str(
        "\n## External packages (import counts; `system` = mapped in blueprint.systems)\n\n",
    );
    table(
        &mut s,
        f["externalPackages"].as_array().unwrap(),
        &[
            ("package", "package"),
            ("modules", "modules"),
            ("system", "system"),
        ],
        80,
    );
    s.push_str("\n## Functions called with a URL first argument (http.calls candidates)\n\n");
    table(
        &mut s,
        f["urlCallees"].as_array().unwrap(),
        &[("callee", "callee"), ("calls", "calls")],
        25,
    );
    let h = &f["http"];
    s.push_str(&format!(
        "\n## HTTP links\n\n{} route nodes, {} linked calls, {} calls matched no route.\n\n",
        h["routes"], h["linked"], h["unmatched"]
    ));
    if h["unmatched"].as_u64().unwrap_or(0) > 0 {
        table(
            &mut s,
            h["unmatchedSamples"].as_array().unwrap(),
            &[
                ("method", "method"),
                ("url", "url"),
                ("file", "file"),
                ("line", "line"),
            ],
            20,
        );
    }
    s.push_str("\n## Graph\n\n");
    s.push_str(&format!(
        "nodes: {}\n\nedges: {}\n\n",
        f["graph"]["nodes"], f["graph"]["edges"]
    ));
    if let Some(pe) = f["graph"]["parseErrors"]
        .as_array()
        .filter(|a| !a.is_empty())
    {
        s.push_str("parse errors:\n");
        for e in pe {
            s.push_str(&format!(
                "- {}: {}\n",
                e["file"].as_str().unwrap_or(""),
                e["message"].as_str().unwrap_or("")
            ));
        }
        s.push('\n');
    }
    s.push_str("## Arch violations (whole repo)\n\n");
    match f["archViolations"].as_object().filter(|m| !m.is_empty()) {
        Some(m) => {
            for (k, v) in m {
                s.push_str(&format!("- {k}: {v}\n"));
            }
        }
        None => s.push_str("(none)\n"),
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{Fake, env};
    use ig_core::testing::MemFiles;

    #[test]
    fn facts_and_report() {
        let cfg = "layers:\n  core: ['src/core/**']\n";
        let files = MemFiles::with(&[
            ("ig.yaml", cfg),
            ("package.json", r#"{"name": "app"}"#),
            ("src/core/a.ts", "fn a\n"),
            ("src/pages/home.ts", "fn h\n"),
        ]);
        let fe = Fake;
        let f = facts(&env(&files, None, &fe)).unwrap();
        assert_eq!(
            (
                f["config"]["exists"].as_bool(),
                f["config"]["layers"].as_u64()
            ),
            (Some(true), Some(1))
        );
        assert_eq!(f["packages"][0]["name"], "app");
        assert_eq!(f["pageDirs"][0], json!({"dir": "src/pages", "modules": 1}));
        assert_eq!(f["unlayered"]["count"], 1);
        let r = report(&f);
        assert!(r.starts_with("# ig setup\n") && r.contains("## Modules without a layer: 1"));
        let bad = MemFiles::with(&[("ig.yaml", "layers: 3\n")]);
        let f = facts(&env(&bad, None, &fe)).unwrap();
        assert!(f["config"]["error"].is_string());
    }
}
