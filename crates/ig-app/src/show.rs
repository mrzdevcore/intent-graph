//! `ig_show`: an answer shown in the graph. The agent names nodes, edges and ordered steps; every
//! id is checked against the graph before the viewer is told.

use crate::map::view::View;
use anyhow::{Result, anyhow, bail};
use ig_core::model::{Graph, suggest_among};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeSet, HashSet};

/// Where the show is written, for the viewers to pick up.
pub const SHOW: &str = ".ig/show.json";
// not IG_APP: that one names the app binary for `ig app`
pub const IN_APP: &str = "IG_IN_APP";

const MAX_NODES: usize = 60;
const MAX_STEPS: usize = 40;
const MAX_TITLE: usize = 200;
const MAX_TEXT: usize = 500;
const MAX_ID: usize = 512;

fn short(what: &str, s: &str, max: usize) -> Result<()> {
    let n = s.chars().count();
    if n > max {
        bail!("{what} is {n} characters: at most {max}");
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Show {
    pub version: u32,
    pub seq: u64,
    pub title: String,
    pub nodes: Vec<String>,
    #[serde(default)]
    pub edges: Vec<(String, String)>,
    #[serde(default)]
    pub steps: Vec<ShowStep>,
    #[serde(default)]
    pub mode: ShowMode,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShowStep {
    /// A node id, or `from|to` for an edge.
    pub target: String,
    pub text: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ShowMode {
    #[default]
    Focus,
    Architecture,
    Chain,
}

/// What the agent passes: a show without `version` and `seq`; edges as `[from, to]` or
/// `"from|kind|to"`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    title: String,
    nodes: Vec<String>,
    #[serde(default)]
    edges: Vec<Value>,
    #[serde(default)]
    steps: Vec<ShowStep>,
    #[serde(default)]
    mode: ShowMode,
}

/// What a show may name: the code graph's nodes and edges, and the map's boxes and arrows.
pub struct Targets {
    ids: BTreeSet<String>,
    edges: HashSet<(String, String)>,
}

impl Targets {
    pub fn new(g: &Graph, map: Option<&View>) -> Self {
        let mut ids: BTreeSet<String> = g.nodes.iter().map(|n| n.id.clone()).collect();
        let mut edges: HashSet<(String, String)> = g
            .edges
            .iter()
            .map(|e| (e.from.clone(), e.to.clone()))
            .collect();
        if let Some(v) = map {
            ids.extend(v.boxes.iter().map(|b| b.id.clone()));
            for a in v
                .arrows
                .iter()
                .flat_map(|a| std::iter::once(a).chain(&a.parts))
            {
                edges.insert((a.from.clone(), a.to.clone()));
            }
        }
        Targets { ids, edges }
    }
}

fn known(g: &Targets, id: &str) -> Result<()> {
    short("an id", id, MAX_ID)?;
    if g.ids.contains(id) {
        return Ok(());
    }
    let s: Vec<String> = suggest_among(g.ids.iter().map(String::as_str), id)
        .into_iter()
        .take(3)
        .collect();
    if s.is_empty() {
        bail!("unknown id {id}");
    }
    bail!("unknown id {id}; did you mean {}?", s.join(", "))
}

fn edge(g: &Targets, from: &str, to: &str) -> Result<(String, String)> {
    known(g, from)?;
    known(g, to)?;
    if !g.edges.contains(&(from.to_string(), to.to_string())) {
        bail!("no edge {from} -> {to} in the graph");
    }
    Ok((from.to_string(), to.to_string()))
}

/// The show for `input`, numbered `seq`, once every id and edge is in `g`.
pub fn validate(g: &Targets, input: &Value, seq: u64) -> Result<Show> {
    let i: Input = serde_json::from_value(input.clone())
        .map_err(|e| anyhow!("expected {{title, nodes, edges?, steps?, mode?}}: {e}"))?;
    if i.nodes.is_empty() {
        bail!("nodes is empty: name at least one node id");
    }
    if i.nodes.len() > MAX_NODES {
        bail!("{} nodes: at most {MAX_NODES}", i.nodes.len());
    }
    if i.steps.len() > MAX_STEPS {
        bail!("{} steps: at most {MAX_STEPS}", i.steps.len());
    }
    short("title", &i.title, MAX_TITLE)?;
    for s in &i.steps {
        short("a step's text", &s.text, MAX_TEXT)?;
    }
    for n in &i.nodes {
        known(g, n)?;
    }
    let edges = i
        .edges
        .iter()
        .map(|e| match e {
            Value::Array(a) if a.len() == 2 && a.iter().all(Value::is_string) => {
                edge(g, a[0].as_str().unwrap(), a[1].as_str().unwrap())
            }
            Value::String(k) => match k.split('|').collect::<Vec<_>>()[..] {
                [from, _, to] => edge(g, from, to),
                _ => bail!("edge {k}: expected \"from|kind|to\""),
            },
            _ => bail!("edge {e}: expected [from, to] or \"from|kind|to\""),
        })
        .collect::<Result<Vec<_>>>()?;
    for s in &i.steps {
        match s.target.split_once('|') {
            Some((from, to)) => {
                edge(g, from, to)?;
            }
            None => known(g, &s.target)?,
        }
    }
    Ok(Show {
        version: 1,
        seq,
        title: i.title,
        nodes: i.nodes,
        edges,
        steps: i.steps,
        mode: i.mode,
    })
}

/// The file's text: pretty, fields in a fixed order, a trailing newline.
pub fn canonical(s: &Show) -> String {
    serde_json::to_string_pretty(s).expect("a show serializes") + "\n"
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn g() -> Graph {
        let mut g = Graph::empty();
        let node = |id: &str| {
            serde_json::from_value(json!({"id": id, "kind": "function", "name": id, "file": "src/a.ts",
                "range": {"startLine": 1, "endLine": 1}, "hash": "", "metrics": {"loc": 1, "complexity": 1}, "intents": []}))
            .unwrap()
        };
        g.nodes = vec![node("src/a.ts#foo"), node("a"), node("b")];
        g.edges = vec![
            serde_json::from_value(json!({"from": "a", "to": "b", "kind": "calls", "confidence": "resolved", "file": "src/a.ts", "line": 1}))
                .unwrap(),
        ];
        g
    }

    #[test]
    fn unknown_id_errors_with_suggestions() {
        let e = validate(
            &Targets::new(&g(), None),
            &json!({"title": "t", "nodes": ["src/a.ts#fo"]}),
            1,
        )
        .unwrap_err();
        assert!(
            e.to_string()
                .contains("unknown id src/a.ts#fo; did you mean src/a.ts#foo"),
            "{e}"
        );
    }

    #[test]
    fn edge_pairs_and_keys_are_accepted_and_must_exist() {
        let s = validate(
            &Targets::new(&g(), None),
            &json!({"title": "t", "nodes": ["a"], "edges": [["a", "b"], "a|calls|b"], "steps": [{"target": "a|b", "text": "x"}]}),
            1,
        )
        .unwrap();
        let ab = ("a".to_string(), "b".to_string());
        assert_eq!(s.edges, vec![ab.clone(), ab]);
        let e = validate(
            &Targets::new(&g(), None),
            &json!({"title": "t", "nodes": ["a"], "edges": [["b", "a"]]}),
            1,
        );
        assert!(e.unwrap_err().to_string().contains("no edge b -> a"));
    }

    #[test]
    fn limits_and_empty_nodes_are_refused() {
        assert!(
            validate(
                &Targets::new(&g(), None),
                &json!({"title": "t", "nodes": []}),
                1
            )
            .is_err()
        );
        let many: Vec<&str> = vec!["a"; 61];
        assert!(
            validate(
                &Targets::new(&g(), None),
                &json!({"title": "t", "nodes": many}),
                1
            )
            .is_err()
        );
        let steps: Vec<Value> = (0..41)
            .map(|_| json!({"target": "a", "text": "x"}))
            .collect();
        let e = validate(
            &Targets::new(&g(), None),
            &json!({"title": "t", "nodes": ["a"], "steps": steps}),
            1,
        );
        assert!(e.unwrap_err().to_string().contains("at most 40"));
        let ok: Vec<&str> = vec!["a"; 60];
        assert!(
            validate(
                &Targets::new(&g(), None),
                &json!({"title": "t", "nodes": ok}),
                1
            )
            .is_ok()
        );
    }

    #[test]
    fn long_title_step_text_and_ids_are_refused() {
        let t = |v: Value| {
            validate(&Targets::new(&g(), None), &v, 1)
                .unwrap_err()
                .to_string()
        };
        let long = |n: usize| "x".repeat(n);
        assert!(
            t(json!({"title": long(201), "nodes": ["a"]}))
                .contains("title is 201 characters: at most 200")
        );
        let steps = json!([{"target": "a", "text": long(501)}]);
        assert!(t(json!({"title": "t", "nodes": ["a"], "steps": steps})).contains("at most 500"));
        assert!(
            t(json!({"title": "t", "nodes": [long(513)]}))
                .contains("an id is 513 characters: at most 512")
        );
        let ok = json!({"title": long(200), "nodes": ["a"], "steps": [{"target": "a", "text": long(500)}]});
        assert!(validate(&Targets::new(&g(), None), &ok, 1).is_ok());
    }

    #[test]
    fn valid_show_round_trips() {
        let s = validate(
            &Targets::new(&g(), None),
            &json!({"title": "How a calls b", "nodes": ["a", "b"], "edges": ["a|calls|b"], "mode": "chain",
                "steps": [{"target": "a", "text": "a starts"}, {"target": "a|b", "text": "a calls b"}]}),
            7,
        )
        .unwrap();
        assert_eq!((s.version, s.seq, s.mode), (1, 7, ShowMode::Chain));
        let text = canonical(&s);
        assert!(text.ends_with("}\n") && text.contains("\"mode\": \"chain\""));
        assert_eq!(serde_json::from_str::<Show>(&text).unwrap(), s);
    }
}
