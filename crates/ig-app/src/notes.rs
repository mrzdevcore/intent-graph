//! Plan notes: the notes file next to the plan, validated against the graph. Plan paths are
//! as the caller gives them; `files` resolves them (the binary roots it at the working
//! directory).

pub use ig_core::notes::*;

use anyhow::{Result, bail};
use ig_core::model::{Graph, suggest};
use ig_core::ports::Files;
use std::path::{Path, PathBuf};

/// `plans/<slug>.md` -> `plans/<slug>.notes.md`.
pub fn path_for(plan: &Path) -> PathBuf {
    plan.with_extension("notes.md")
}

fn file_text(files: &dyn Files, path: &Path) -> String {
    crate::read_text(files, &path.to_string_lossy())
}

/// The notes of the plan at `plan`; none when it has no notes file.
pub fn read(files: &dyn Files, plan: &Path) -> Vec<Note> {
    parse(&file_text(files, &path_for(plan)))
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
pub fn append(files: &dyn Files, plan: &Path, input: NoteInput, merged: &Graph) -> Result<Note> {
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
        let s = suggest(merged, id);
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
    let old = file_text(files, &path);
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
    files.write_atomic(&path.to_string_lossy(), out.as_bytes())?;
    Ok(note)
}

/// Ticks (or reopens) note `id` and sets its reply.
pub fn set_done(
    files: &dyn Files,
    plan: &Path,
    id: &str,
    done: bool,
    reply: Option<&str>,
) -> Result<()> {
    let path = path_for(plan);
    let old = file_text(files, &path);
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
    files.write_atomic(&path.to_string_lossy(), out.join("\n").as_bytes())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ig_core::testing::MemFiles;

    fn graph(ids: &[&str]) -> Graph {
        let mut g = Graph::empty();
        g.nodes = ids
            .iter()
            .map(|id| ig_core::model::Node {
                id: id.to_string(),
                kind: "function".into(),
                name: id.to_string(),
                file: "a.ts".into(),
                range: ig_core::model::Range {
                    start_line: 1,
                    end_line: 1,
                },
                parent: None,
                package: None,
                exported: false,
                hash: String::new(),
                metrics: ig_core::model::Metrics {
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
        let f = MemFiles::default();
        let plan = Path::new("plans/x.md");
        let g = graph(&["a.ts", "a.ts#foo", "b.ts"]);
        let input = |kind, target: &str| NoteInput {
            kind,
            target: target.into(),
            to: None,
            layer: None,
            text: None,
        };
        let n1 = append(
            &f,
            plan,
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
            &f,
            plan,
            NoteInput {
                layer: Some("core".into()),
                ..input(NoteKind::Move, "b.ts")
            },
            &g,
        )
        .unwrap();
        assert_eq!(n2.id, "N2");
        let err = append(&f, plan, input(NoteKind::Remove, "a.ts#fooo"), &g)
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("unknown node a.ts#fooo") && err.contains("a.ts#foo"),
            "{err}"
        );
        let err = append(
            &f,
            plan,
            NoteInput {
                to: Some("c.ts".into()),
                ..input(NoteKind::Link, "a.ts")
            },
            &g,
        )
        .unwrap_err();
        assert!(err.to_string().contains("unknown node c.ts"));
        assert!(
            append(&f, plan, input(NoteKind::Comment, "a.ts"), &g).is_err(),
            "a comment needs text"
        );
        let n3 = append(
            &f,
            plan,
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
        assert_eq!(append(&f, plan, c("— x"), &g).unwrap().id, "N4");
        assert_eq!(append(&f, plan, c("foo —"), &g).unwrap().id, "N5");
        set_done(&f, plan, "N5", false, Some("— reply —")).unwrap();
        set_done(&f, plan, "N2", true, Some("moved")).unwrap();
        assert!(set_done(&f, plan, "N9", true, None).is_err());
        let text = f.read("plans/x.notes.md").unwrap();
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
            append(&f, plan, link("eats"), &g)
                .unwrap_err()
                .to_string()
                .contains("unknown link kind")
        );
        assert!(append(&f, plan, link("imports"), &g).is_ok());
    }
}
