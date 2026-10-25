//! Plan co-design notes: `plans/<slug>.notes.md` next to the plan, one Markdown task line per
//! note. The viewer adds them through `ig serve`; the agent ticks them (`- [x]`) and may append
//! ` — <reply>`.

use serde::{Deserialize, Serialize};

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

pub fn parse_line(line: &str) -> Option<Note> {
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

/// One line, and no "—" (" — " starts the reply; a dash at either end would make one).
pub fn clean(s: &str) -> String {
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace('—', "-")
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
}
