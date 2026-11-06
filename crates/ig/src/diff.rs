use crate::cli::DiffArgs;
use crate::say;
use anyhow::Result;
use ig_app::map::diff::{self, Diff, IntentStatus};
use ig_app::map::view;
use ig_wire::Wired;
use std::path::Path;

pub fn run(root: &Path, args: &DiffArgs, quiet: bool) -> Result<u8> {
    if ig_wire::is_landscape(root) {
        eprintln!("ig: ig diff does not read a folder of projects yet");
        return Ok(2);
    }
    let w = Wired::new(root);
    let Some(vcs) = w.vcs() else {
        eprintln!("ig: ig diff needs a git repository");
        return Ok(2);
    };
    let base = match &args.base {
        Some(b) => b.clone(),
        None => {
            let mut notices = vec![];
            let sha = ig_core::ports::Vcs::resolve_base(vcs, None, &mut notices)?;
            notices.iter().for_each(|n| say(quiet, &format!("ig: {n}")));
            match sha {
                Some(s) => s,
                None => {
                    eprintln!("ig: ig diff needs a commit to compare against");
                    return Ok(2);
                }
            }
        }
    };
    let at = |at: &Path, use_spec: bool, cfg: &_| {
        view::architecture(&Wired::new(at).env(), use_spec, Some(cfg))
    };
    let d = diff::run(&w.env(), &base, args.head.as_deref(), &at)?;
    if args.json {
        println!("{}", serde_json::to_string_pretty(&d)?);
    } else {
        print!("{}", text(&d));
    }
    Ok(0)
}

fn short(rev: &str) -> &str {
    if rev.len() == 40 && rev.bytes().all(|b| b.is_ascii_hexdigit()) {
        &rev[..7]
    } else {
        rev
    }
}

fn text(d: &Diff) -> String {
    let head = d.head.as_deref().map_or("working tree", short);
    let mut out = format!("{}..{head}: {}\n", short(&d.base), d.summary);
    for c in d.boxes.changed.iter().filter(|c| c.direct) {
        let n = c.files_changed;
        let files = if n == 1 { "file" } else { "files" };
        out += &format!("  changed  {} ({n} {files})\n", c.title);
    }
    for b in &d.boxes.added {
        out += &format!("  added    {}\n", b.title);
    }
    for b in &d.boxes.removed {
        out += &format!("  removed  {}\n", b.title);
    }
    for a in &d.arrows.added {
        out += &format!("  arrow +  {} → {} ({})\n", a.from, a.to, a.kind);
    }
    for a in &d.arrows.removed {
        out += &format!("  arrow −  {} → {} ({})\n", a.from, a.to, a.kind);
    }
    out += &intent(d);
    for p in &d.problems.new {
        out += &format!("  problem  {}\n", p.text);
    }
    for p in &d.problems.fixed {
        out += &format!("  fixed    {}\n", p.text);
    }
    out
}

fn intent(d: &Diff) -> String {
    let i = &d.intent;
    let mut out = String::new();
    match i.status {
        IntentStatus::Added => out += "  ig/      added\n",
        IntentStatus::Removed => out += "  ig/      removed\n",
        IntentStatus::BaseBroken | IntentStatus::Same | IntentStatus::Changed => {}
    }
    if let Some(n) = &i.note {
        out += &format!("  ig/      {n}\n");
    }
    for b in &i.added {
        out += &format!("  ig/      {}: added\n", b.id);
    }
    for b in &i.removed {
        out += &format!("  ig/      {}: removed\n", b.id);
    }
    for m in &i.moved {
        let at = |p: &Option<String>| p.clone().unwrap_or_else(|| "top".into());
        out += &format!(
            "  ig/      {}: moved {} → {}\n",
            m.id,
            at(&m.from),
            at(&m.to)
        );
    }
    for c in &i.changed {
        out += &format!("  ig/      {}: {}\n", c.id, c.fields.join(", "));
    }
    for a in &i.arrows_added {
        out += &format!("  ig/      arrow + {} → {} ({})\n", a.from, a.to, a.kind);
    }
    for a in &i.arrows_removed {
        out += &format!("  ig/      arrow − {} → {} ({})\n", a.from, a.to, a.kind);
    }
    out
}
