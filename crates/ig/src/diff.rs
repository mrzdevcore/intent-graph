use crate::cli::DiffArgs;
use crate::say;
use anyhow::Result;
use ig_app::map::diff::{self, ArrowRef, Diff, IntentStatus};
use ig_app::map::view;
use ig_wire::Wired;
use std::collections::{HashMap, HashSet};
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
    let view_at = |at: Option<&Path>| match at {
        None => view::run(&w.env()),
        Some(r) => view::run(&Wired::new(r).env()),
    };
    let want = (!args.json).then_some(&view_at as diff::ViewAt);
    let (d, head_view) = diff::run_with_view(&w.env(), &base, args.head.as_deref(), &at, want)?;
    if args.json {
        println!("{}", serde_json::to_string_pretty(&d)?);
        return Ok(0);
    }
    print!("{}", text(&d));
    if let Some(v) = head_view {
        crate::render::ignore_ig_dir(root)?;
        let path = crate::render::write_diff(&w, &v, &d)?;
        say(quiet, "ig: wrote .ig/diff.html");
        if !args.no_open {
            crate::render::open(&path);
        }
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
    let changed = d.boxes.changed.iter().filter(|c| c.direct);
    let mut named: Vec<(&str, &str)> = changed.clone().map(|c| (&*c.id, &*c.title)).collect();
    let boxes = d.boxes.added.iter().chain(&d.boxes.removed);
    named.extend(boxes.map(|b| (&*b.id, &*b.title)));
    for a in d.arrows.added.iter().chain(&d.arrows.removed) {
        named.extend([(&*a.from, &*a.from_title), (&*a.to, &*a.to_title)]);
    }
    let mut ids: HashMap<&str, HashSet<&str>> = HashMap::new();
    for (id, title) in named {
        ids.entry(title).or_default().insert(id);
    }
    let name = |id: &str, title: &str| match ids.get(title).is_some_and(|s| s.len() > 1) {
        true => format!("{title} ({id})"),
        false => title.to_string(),
    };
    for c in changed {
        let n = c.files_changed;
        let files = if n == 1 { "file" } else { "files" };
        out += &format!("  changed  {} ({n} {files})\n", name(&c.id, &c.title));
    }
    for b in &d.boxes.added {
        out += &format!("  added    {}\n", name(&b.id, &b.title));
    }
    for b in &d.boxes.removed {
        out += &format!("  removed  {}\n", name(&b.id, &b.title));
    }
    let arrow = |a: &ArrowRef| {
        let (f, t) = (name(&a.from, &a.from_title), name(&a.to, &a.to_title));
        format!("{f} → {t} ({})", a.kind)
    };
    for a in &d.arrows.added {
        out += &format!("  arrow +  {}\n", arrow(a));
    }
    for a in &d.arrows.removed {
        out += &format!("  arrow −  {}\n", arrow(a));
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
    if let IntentStatus::Added | IntentStatus::Removed | IntentStatus::BaseBroken = i.status {
        return (i.note.as_ref()).map_or(String::new(), |n| format!("  ig/      {n}\n"));
    }
    let mut out = String::new();
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
        let (f, t) = (&a.from_title, &a.to_title);
        out += &format!("  ig/      arrow + {f} → {t} ({})\n", a.kind);
    }
    for a in &i.arrows_removed {
        let (f, t) = (&a.from_title, &a.to_title);
        out += &format!("  ig/      arrow − {f} → {t} ({})\n", a.kind);
    }
    out
}
