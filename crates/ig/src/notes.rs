//! Plan co-design notes: `plans/<slug>.notes.md` next to the plan, one Markdown task line per
//! note. The viewer adds them through `ig serve`; the agent ticks them (`- [x]`) and may append
//! ` — <reply>`.

use crate::model::Graph;
use crate::{check, mcp};
use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NoteKind {
    Comment,
    Move,
    Link,
    Remove,
    Add,
}

/// One note. By kind:
/// - comment: `target` (and `to` for an edge), `text`;
/// - move: `target`, `layer`;
/// - link: `target` -> `to`, `text` is the edge kind;
/// - remove: `target` (and `to` for an edge);
/// - add: `text` is the node kind, `target` its name, `layer` the file or layer it goes in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Note {
    pub id: String,
    pub done: bool,
    pub kind: NoteKind,
    pub target: String,
    pub to: Option<String>,
    pub layer: Option<String>,
    pub text: Option<String>,
    pub reply: Option<String>,
}

/// A note as the viewer sends it.
#[derive(Debug, Clone, Deserialize)]
pub struct NoteInput {
    pub kind: NoteKind,
    pub target: String,
    #[serde(default)]
    pub to: Option<String>,
    #[serde(default)]
    pub layer: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
}

pub fn parse(text: &str) -> Vec<Note> {
    text.lines().filter_map(parse_line).collect()
}

fn parse_line(line: &str) -> Option<Note> {
    let (mark, rest) = line.trim_end().strip_prefix("- [")?.split_once("] ")?;
    let done = match mark {
        " " => false,
        "x" | "X" => true,
        _ => return None,
    };
    let (id, rest) = rest.split_once(' ')?;
    id.strip_prefix('N')?.parse::<u32>().ok()?;
    let (body, reply) = match rest.split_once(" — ") {
        Some((b, r)) => (b, Some(r.to_string())),
        None => (rest, None),
    };
    let (verb, args) = body.split_once(' ')?;
    let edge = |s: &str| match s.split_once(" -> ") {
        Some((a, b)) => (a.to_string(), Some(b.to_string())),
        None => (s.to_string(), None),
    };
    let s = |x: &str| Some(x.to_string());
    let (kind, (target, to), layer, text) = match verb {
        "comment" => {
            let (t, text) = args.strip_prefix("on ")?.split_once(": ")?;
            (NoteKind::Comment, edge(t), None, s(text))
        }
        "move" => {
            let (t, l) = args.rsplit_once(" to layer ")?;
            (NoteKind::Move, (t.to_string(), None), s(l), None)
        }
        "link" => {
            let (t, k) = args.strip_suffix(')')?.rsplit_once(" (")?;
            let e = edge(t);
            e.1.as_ref()?;
            (NoteKind::Link, e, None, s(k))
        }
        "remove" => (NoteKind::Remove, edge(args), None, None),
        "add" => {
            let (k, rest) = args.split_once(' ')?;
            let (name, at) = rest.rsplit_once(" in ")?;
            (NoteKind::Add, (name.to_string(), None), s(at), s(k))
        }
        _ => return None,
    };
    Some(Note {
        id: id.to_string(),
        done,
        kind,
        target,
        to,
        layer,
        text,
        reply,
    })
}

/// The note's line (without a newline).
pub fn format(n: &Note) -> String {
    let t = |s: &Option<String>| s.clone().unwrap_or_default();
    let target = match &n.to {
        Some(to) => format!("{} -> {to}", n.target),
        None => n.target.clone(),
    };
    let body = match n.kind {
        NoteKind::Comment => format!("comment on {target}: {}", t(&n.text)),
        NoteKind::Move => format!("move {target} to layer {}", t(&n.layer)),
        NoteKind::Link => format!("link {target} ({})", t(&n.text)),
        NoteKind::Remove => format!("remove {target}"),
        NoteKind::Add => format!("add {} {target} in {}", t(&n.text), t(&n.layer)),
    };
    let mut line = format!("- [{}] {} {body}", if n.done { 'x' } else { ' ' }, n.id);
    if let Some(r) = &n.reply {
        line += " — ";
        line += r;
    }
    line
}

/// `plans/<slug>.md` -> `plans/<slug>.notes.md`.
pub fn path_for(plan: &Path) -> PathBuf {
    plan.with_extension("notes.md")
}

/// The notes of the plan at `plan`; none when it has no notes file.
pub fn read(plan: &Path) -> Vec<Note> {
    parse(&std::fs::read_to_string(path_for(plan)).unwrap_or_default())
}

/// One line, and no "—" (" — " starts the reply; a dash at either end would make one).
fn clean(s: &str) -> String {
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace('—', "-")
}

/// The edge kinds the graph has.
const LINK_KINDS: &[&str] = &[
    "calls",
    "imports",
    "http",
    "uses",
    "renders",
    "extends",
    "implements",
];

/// Validates `input` against `merged` (the graph with the plan applied), gives it the next id and
/// appends its line to the plan's notes file.
pub fn append(plan: &Path, input: NoteInput, merged: &Graph) -> Result<Note> {
    let opt = |o: &Option<String>| o.as_deref().map(clean).filter(|s| !s.is_empty());
    let (target, to, layer, text) = (
        clean(&input.target),
        opt(&input.to),
        opt(&input.layer),
        opt(&input.text),
    );
    if target.is_empty() {
        bail!("a note needs a target");
    }
    let known = |id: &str| -> Result<()> {
        if merged.nodes.iter().any(|n| n.id == id) {
            return Ok(());
        }
        let s = mcp::suggest(merged, id);
        if s.is_empty() {
            bail!("unknown node {id}");
        }
        bail!("unknown node {id}; did you mean {}", s.join(", "))
    };
    let need = |o: &Option<String>, what: &str| -> Result<()> {
        if o.is_none() {
            bail!("a {:?} note needs {what}", input.kind);
        }
        Ok(())
    };
    match input.kind {
        NoteKind::Comment => need(&text, "text")?,
        NoteKind::Move => need(&layer, "a layer")?,
        NoteKind::Link => {
            need(&to, "a `to` node")?;
            if let Some(k) = text.as_deref().filter(|k| !LINK_KINDS.contains(k)) {
                bail!(
                    "unknown link kind {k:?}; expected one of {}",
                    LINK_KINDS.join(", ")
                );
            }
        }
        NoteKind::Remove => {}
        NoteKind::Add => {
            need(&layer, "a file or layer")?;
            if target.contains(' ') {
                bail!("a new node's name has no spaces");
            }
        }
    }
    if input.kind != NoteKind::Add {
        known(&target)?;
    }
    if let Some(to) = &to {
        known(to)?;
    }
    let text = match input.kind {
        NoteKind::Link => text.or(Some("calls".into())),
        NoteKind::Add => text.or(Some("function".into())),
        _ => text,
    };
    let path = path_for(plan);
    let old = std::fs::read_to_string(&path).unwrap_or_default();
    let next = parse(&old)
        .iter()
        .filter_map(|n| n.id[1..].parse::<u32>().ok())
        .max()
        .unwrap_or(0)
        + 1;
    let note = Note {
        id: format!("N{next}"),
        done: false,
        kind: input.kind,
        target,
        to,
        layer,
        text,
        reply: None,
    };
    let mut out = if old.is_empty() {
        let name = plan.file_name().unwrap_or_default().to_string_lossy();
        format!("# Notes on {name}\n\n")
    } else {
        old
    };
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out += &format(&note);
    out.push('\n');
    check::write_atomic(&path, out.as_bytes())?;
    Ok(note)
}

/// Ticks (or reopens) note `id` and sets its reply.
pub fn set_done(plan: &Path, id: &str, done: bool, reply: Option<&str>) -> Result<()> {
    let path = path_for(plan);
    let old = std::fs::read_to_string(&path).unwrap_or_default();
    let mut found = false;
    let mut out: Vec<String> = old
        .lines()
        .map(|l| match parse_line(l) {
            Some(mut n) if n.id == id => {
                found = true;
                n.done = done;
                if let Some(r) = reply.map(clean).filter(|r| !r.is_empty()) {
                    n.reply = Some(r);
                }
                format(&n)
            }
            _ => l.to_string(),
        })
        .collect();
    if !found {
        bail!("no note {id} in {}", path.display());
    }
    out.push(String::new());
    check::write_atomic(&path, out.join("\n").as_bytes())
}

/// Blocks until an open note that was not there when called appears, or `timeout` passes (then
/// none). Polls the file: it works without `ig serve`.
pub fn wait(plan: &Path, timeout: Duration) -> Vec<Note> {
    let seen: HashSet<String> = read(plan).into_iter().map(|n| n.id).collect();
    let end = Instant::now() + timeout;
    while Instant::now() < end {
        let new: Vec<Note> = read(plan)
            .into_iter()
            .filter(|n| !n.done && !seen.contains(&n.id))
            .collect();
        if !new.is_empty() {
            return new;
        }
        std::thread::sleep(Duration::from_millis(300));
    }
    vec![]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(id: &str, kind: NoteKind, target: &str) -> Note {
        Note {
            id: id.into(),
            done: false,
            kind,
            target: target.into(),
            to: None,
            layer: None,
            text: None,
            reply: None,
        }
    }

    #[test]
    fn round_trips_all_kinds_with_done_and_reply() {
        let s = |x: &str| Some(x.to_string());
        let route = "packages/api/src/routes.ts#GET /users/:id";
        let notes = vec![
            Note {
                text: s("why here: and not core?"),
                ..note("N1", NoteKind::Comment, route)
            },
            Note {
                to: s("b.ts#g"),
                text: s("drop it"),
                done: true,
                reply: s("done — moved"),
                ..note("N2", NoteKind::Comment, "a.ts#f")
            },
            Note {
                layer: s("core"),
                ..note("N3", NoteKind::Move, "packages/ui/src/x.ts")
            },
            Note {
                to: s("b.ts"),
                text: s("imports"),
                reply: s("no: it would cycle"),
                ..note("N4", NoteKind::Link, "a.ts")
            },
            Note {
                done: true,
                ..note("N5", NoteKind::Remove, "a.ts#f")
            },
            Note {
                to: s("b.ts#g"),
                ..note("N6", NoteKind::Remove, "a.ts#f")
            },
            Note {
                text: s("function"),
                layer: s("packages/core/src/export.ts"),
                ..note("N7", NoteKind::Add, "exportRows")
            },
            Note {
                text: s("module"),
                layer: s("core"),
                done: true,
                ..note("N8", NoteKind::Add, "cache")
            },
        ];
        let text = format!(
            "# Notes\n\nfree text\n{}\n",
            notes.iter().map(format).collect::<Vec<_>>().join("\n")
        );
        assert!(text.contains(&format!(
            "- [ ] N1 comment on {route}: why here: and not core?"
        )));
        assert!(text.contains("- [x] N2 comment on a.ts#f -> b.ts#g: drop it — done — moved"));
        assert!(text.contains("- [ ] N3 move packages/ui/src/x.ts to layer core"));
        assert!(text.contains("- [ ] N4 link a.ts -> b.ts (imports) — no: it would cycle"));
        assert!(text.contains("- [x] N5 remove a.ts#f"));
        assert!(text.contains("- [ ] N7 add function exportRows in packages/core/src/export.ts"));
        assert_eq!(parse(&text), notes);
    }

    fn graph(ids: &[&str]) -> Graph {
        let mut g = Graph::empty();
        g.nodes = ids
            .iter()
            .map(|id| crate::model::Node {
                id: id.to_string(),
                kind: "function".into(),
                name: id.to_string(),
                file: "a.ts".into(),
                range: crate::model::Range {
                    start_line: 1,
                    end_line: 1,
                },
                parent: None,
                package: None,
                exported: false,
                hash: String::new(),
                metrics: crate::model::Metrics {
                    loc: 1,
                    complexity: 1,
                },
                intents: vec![],
            })
            .collect();
        g
    }

    #[test]
    fn append_numbers_sequentially_and_rejects_unknown_ids_with_suggestions() {
        let dir = std::env::temp_dir().join(format!("ig-notes-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("plans")).unwrap();
        let plan = dir.join("plans/x.md");
        let g = graph(&["a.ts", "a.ts#foo", "b.ts"]);
        let input = |kind, target: &str| NoteInput {
            kind,
            target: target.into(),
            to: None,
            layer: None,
            text: None,
        };
        let n1 = append(
            &plan,
            NoteInput {
                text: Some("multi\nline — text".into()),
                ..input(NoteKind::Comment, "a.ts#foo")
            },
            &g,
        )
        .unwrap();
        assert_eq!(
            (n1.id.as_str(), n1.text.as_deref()),
            ("N1", Some("multi line - text"))
        );
        let n2 = append(
            &plan,
            NoteInput {
                layer: Some("core".into()),
                ..input(NoteKind::Move, "b.ts")
            },
            &g,
        )
        .unwrap();
        assert_eq!(n2.id, "N2");
        let err = append(&plan, input(NoteKind::Remove, "a.ts#fooo"), &g)
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("unknown node a.ts#fooo") && err.contains("a.ts#foo"),
            "{err}"
        );
        let err = append(
            &plan,
            NoteInput {
                to: Some("c.ts".into()),
                ..input(NoteKind::Link, "a.ts")
            },
            &g,
        )
        .unwrap_err();
        assert!(err.to_string().contains("unknown node c.ts"));
        assert!(
            append(&plan, input(NoteKind::Comment, "a.ts"), &g).is_err(),
            "a comment needs text"
        );
        let n3 = append(
            &plan,
            NoteInput {
                text: Some("function".into()),
                layer: Some("core".into()),
                ..input(NoteKind::Add, "brandNew")
            },
            &g,
        )
        .unwrap();
        assert_eq!(n3.id, "N3");
        // A dash at either end of the text or reply must not read as the reply separator.
        let c = |t: &str| NoteInput {
            text: Some(t.into()),
            ..input(NoteKind::Comment, "a.ts#foo")
        };
        assert_eq!(append(&plan, c("— x"), &g).unwrap().id, "N4");
        assert_eq!(append(&plan, c("foo —"), &g).unwrap().id, "N5");
        set_done(&plan, "N5", false, Some("— reply —")).unwrap();
        set_done(&plan, "N2", true, Some("moved")).unwrap();
        assert!(set_done(&plan, "N9", true, None).is_err());
        let text = std::fs::read_to_string(path_for(&plan)).unwrap();
        assert!(
            text.contains("- [x] N2 move b.ts to layer core — moved\n"),
            "{text}"
        );
        let all = parse(&text);
        assert_eq!(
            all.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(),
            ["N1", "N2", "N3", "N4", "N5"]
        );
        assert_eq!(all[3].text.as_deref(), Some("- x"));
        assert_eq!(
            (all[4].text.as_deref(), all[4].reply.as_deref()),
            (Some("foo -"), Some("- reply -"))
        );
        let link = |k: &str| NoteInput {
            to: Some("b.ts".into()),
            text: Some(k.into()),
            ..input(NoteKind::Link, "a.ts")
        };
        assert!(
            append(&plan, link("eats"), &g)
                .unwrap_err()
                .to_string()
                .contains("unknown link kind")
        );
        assert!(append(&plan, link("imports"), &g).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
