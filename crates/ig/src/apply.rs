use crate::cli::{AgentName, ApplyArgs};
use crate::{NO_SPEC, agent, explore, has_spec, say, say_all};
use anyhow::{Result, bail};
use ig_app::map::file::MapBox;
use ig_app::map::origin::{self, Origin};
use ig_app::map::{MapArrow, view};
use ig_wire::Wired;
use std::collections::BTreeMap;
use std::io::{BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::slice::from_ref;
use std::time::Duration;

const PROMPT: &str = include_str!("../assets/apply-prompt.md");
const SKILL: &str = include_str!("../assets/skills/apply.md");

const IG_TOOLS: &str = "mcp__ig__ig_plan,mcp__ig__ig_state";
const TIMEOUT: Duration = Duration::from_secs(30 * 60);
const KEEP_OUT: &[&str] = &[
    "Edit(ig/**)",
    "Write(ig/**)",
    "Edit(ig.yaml)",
    "Write(ig.yaml)",
    "mcp__ig__ig_edit",
    "mcp__ig__ig_show",
];
const KEEP_OUT_OF_BASH: &[&str] = &[
    "Bash(git push:*)",
    "Bash(git commit:*)",
    "Bash(ig accept:*)",
    "Bash(ig fmt:*)",
];

enum What<'a> {
    Box(&'a MapBox),
    Arrow(&'a MapArrow),
}

struct Item<'a> {
    id: &'a str,
    line: String,
    what: What<'a>,
}

pub fn run(root: &Path, args: &ApplyArgs, quiet: bool) -> Result<u8> {
    if args.agent.is_some_and(|a| !matches!(a, AgentName::Claude)) {
        eprintln!("ig: apply needs claude for now");
        return Ok(2);
    }
    let w = Wired::new(root);
    if !has_spec(&w.env())? {
        eprintln!("{NO_SPEC}");
        return Ok(2);
    }
    let v = view::run(&w.env())?;
    say_all(quiet, &v);
    if v.broken {
        return Ok(2);
    }
    let todo = chosen(items(&v), &args.ids)?;
    if todo.is_empty() {
        println!("ig apply: nothing to build");
        return Ok(0);
    }
    println!("{}", listing(&todo));
    if !agent::on_path("claude") {
        eprintln!("ig: claude was not found on PATH");
        return Ok(2);
    }
    if !args.yes {
        if !(std::io::stdin().is_terminal() && std::io::stderr().is_terminal()) {
            eprintln!("ig: the agent writes code: answer in a terminal, or pass --yes");
            return Ok(2);
        }
        if !ask(todo.len()) {
            say(quiet, "ig: nothing built");
            return Ok(1);
        }
    }
    let ids: Vec<String> = todo.iter().map(|i| i.id.to_string()).collect();
    let prompt = prompt(&todo, args.allow_bash);
    let mcp = explore::mcp_config(&std::env::current_exe()?, root);
    let argv = argv(&mcp, args.allow_bash);
    let argv: Vec<&str> = argv.iter().map(String::as_str).collect();
    let before = spec_files(root);
    say(
        quiet,
        &format!(
            "ig: claude is writing code (up to {} min)…",
            TIMEOUT.as_secs() / 60
        ),
    );
    let ran = match agent::run_argv(root, "claude", &argv, &prompt, TIMEOUT) {
        Ok(a) => {
            println!("{}", a.trim_end());
            true
        }
        Err(e) => {
            eprintln!("ig: {e}");
            false
        }
    };
    let touched = spec_files(root) != before;
    let v = view::run(&w.env())?;
    say_all(quiet, &v);
    if !v.broken {
        let unread = view::unread(&w.env(), &v, &ig_fs::BUILD_DIRS)?;
        println!(
            "\n{}",
            origin::plan_of(&v.boxes, &v.arrows, &unread).text("plan")
        );
    }
    if touched {
        eprintln!("ig: the agent changed ig/: review with `git diff ig/ ig.yaml`");
    }
    if touched || !ran || v.broken {
        return Ok(2);
    }
    let left = items(&v).iter().any(|i| ids.iter().any(|x| x == i.id));
    Ok(u8::from(left))
}

fn spec_files(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut out = BTreeMap::new();
    let mut todo = vec![root.join("ig"), root.join("ig.yaml")];
    while let Some(p) = todo.pop() {
        if p.is_dir() {
            todo.extend(
                std::fs::read_dir(&p)
                    .into_iter()
                    .flatten()
                    .flatten()
                    .map(|e| e.path()),
            );
        } else if let Ok(b) = std::fs::read(&p) {
            out.insert(p, b);
        }
    }
    out
}

fn items(v: &view::View) -> Vec<Item<'_>> {
    let line = |p: origin::Plan| p.lines().next().cloned().unwrap_or_default();
    let mut out: Vec<Item> = (v.boxes.iter())
        .filter(|b| matches!(b.origin, Origin::Build | Origin::Moved))
        .map(|b| Item {
            id: &b.id,
            line: line(origin::plan_of(from_ref(b), &[], &[])),
            what: What::Box(b),
        })
        .collect();
    let written = v.arrows.iter().flat_map(|a| match a.parts.is_empty() {
        true => from_ref(a),
        false => &a.parts[..],
    });
    out.extend(written.filter(|a| a.origin == Origin::Build).map(|a| Item {
        id: &a.id,
        line: line(origin::plan_of(&[], from_ref(a), &[])),
        what: What::Arrow(a),
    }));
    out.sort_by_key(|i| (i.line.starts_with("move"), i.line.clone()));
    out
}

fn chosen<'a>(all: Vec<Item<'a>>, ids: &[String]) -> Result<Vec<Item<'a>>> {
    let unknown: Vec<&str> = (ids.iter())
        .filter(|i| !all.iter().any(|it| it.id == i.as_str()))
        .map(String::as_str)
        .collect();
    if !unknown.is_empty() {
        let known: Vec<&str> = all.iter().map(|i| i.id).collect();
        let known = match known.is_empty() {
            true => "none".to_string(),
            false => known.join(", "),
        };
        bail!(
            "not in the plan: {} (the plan's ids: {known}; quote arrow ids: ig apply 'web->api')",
            unknown.join(", ")
        );
    }
    Ok((all.into_iter())
        .filter(|i| ids.is_empty() || ids.iter().any(|x| x == i.id))
        .collect())
}

fn listing(todo: &[Item]) -> String {
    let build = todo.iter().filter(|i| !i.line.starts_with("move")).count();
    let mut t = format!("ig apply: {build} to build");
    if build < todo.len() {
        t += &format!(", {} to move", todo.len() - build);
    }
    let w = todo.iter().map(|i| i.id.chars().count()).max().unwrap_or(0);
    for i in todo {
        t += &format!("\n  {:w$}  {}", i.id, i.line);
    }
    t
}

fn ask(n: usize) -> bool {
    let what = match n {
        1 => "this item".to_string(),
        n => format!("these {n} items"),
    };
    eprint!("Let claude write the code for {what}? [y/N] ");
    let _ = std::io::stderr().flush();
    let mut line = String::new();
    let _ = std::io::stdin().lock().read_line(&mut line);
    line.trim().to_lowercase().starts_with('y')
}

fn prompt(todo: &[Item], bash: bool) -> String {
    let skill = SKILL.splitn(3, "---\n").nth(2).unwrap_or(SKILL).trim();
    let bash = match bash {
        true => "You can run commands (Bash) to build and test.",
        false => "You cannot run commands: leave building and testing to the user, and say so.",
    };
    let items: Vec<String> = todo.iter().map(details).collect();
    PROMPT
        .replace("{{bash}}", bash)
        .replace("{{skill}}", skill)
        .replace("{{items}}", &items.join("\n"))
}

fn details(i: &Item) -> String {
    let mut fields: Vec<(&str, Option<&str>)> = vec![];
    match &i.what {
        What::Box(b) => {
            fields.push(("title", Some(&b.title.value)));
            fields.push(("tech", b.tech.as_ref().map(|f| f.value.as_str())));
            fields.push((
                "description",
                b.description.as_ref().map(|f| f.value.as_str()),
            ));
            fields.push(("context", b.context.as_ref().map(|f| f.value.as_str())));
        }
        What::Arrow(a) => {
            fields.push(("label", a.label.as_deref()));
            fields.push(("note", a.note.as_deref()));
            fields.push(("context", a.context.as_deref()));
        }
    }
    let mut t = format!("- `{}`: {}", i.id, i.line);
    for (k, v) in fields {
        if let Some(v) = v.filter(|v| !v.trim().is_empty()) {
            t += &format!("\n  {k}: {}", v.trim().replace('\n', "\n    "));
        }
    }
    t
}

fn argv(mcp: &str, bash: bool) -> Vec<String> {
    let extra = if bash { ",Bash" } else { "" };
    let mut argv: Vec<String> = [
        "claude",
        "-p",
        "--output-format",
        "text",
        "--restricted",
        "--strict-mcp-config",
        "--mcp-config",
        mcp,
    ]
    .map(String::from)
    .to_vec();
    argv.extend([
        // -p answers no permission prompt: what it may do is approved here or never
        "--allowed-tools".into(),
        format!("Edit,Write,{IG_TOOLS}{extra}"),
        "--tools".into(),
        format!("Read,Grep,Glob,Edit,Write,{IG_TOOLS}{extra}"),
        "--disallowed-tools".into(),
    ]);
    argv.extend(KEEP_OUT.iter().map(|r| r.to_string()));
    if bash {
        argv.extend(KEEP_OUT_OF_BASH.iter().map(|r| r.to_string()));
    }
    argv.extend(explore::secret_rules().map(String::from));
    argv
}

#[cfg(test)]
mod tests {
    use super::*;

    const BILLING: &str = r#"service "billing" title="Billing" tech="TypeScript" {
    at "billing/**"
    context "Use Stripe."
    calls "tiny" label="asks for prices" note="once per checkout"
}
"#;

    fn view_of(billing: &str) -> view::View {
        let dir = crate::tests::tmp_with(&[
            ("src/a/a.ts", "export function f() {}\n"),
            (
                "ig/system.kdl",
                "system \"system\" title=\"tiny\" {\n    at \"**\"\n}\n",
            ),
            (
                "ig/tiny.kdl",
                "module \"tiny\" title=\"Tiny\" {\n    at \"src/**\"\n}\n",
            ),
            ("ig/billing.kdl", billing),
        ]);
        if billing.is_empty() {
            std::fs::remove_file(dir.join("ig/billing.kdl")).unwrap();
        }
        let v = view::run(&crate::tests::env(&dir)).unwrap();
        assert!(!v.broken, "{:?}", v.notices);
        let _ = std::fs::remove_dir_all(&dir);
        v
    }

    #[test]
    fn the_items_are_the_plan_s_to_build_with_their_ids() {
        let v = view_of(BILLING);
        let all = items(&v);
        let ids: Vec<&str> = all.iter().map(|i| i.id).collect();
        assert_eq!(ids, ["billing->tiny", "billing"]);
        assert_eq!(all[1].line, "to build: service billing (billing/**)");
        let one = chosen(items(&v), &["billing->tiny".into()]).unwrap();
        assert_eq!(one.len(), 1);
        let Err(e) = chosen(items(&v), &["nope".into(), "billing".into()]) else {
            panic!()
        };
        assert_eq!(
            e.to_string(),
            "not in the plan: nope (the plan's ids: billing->tiny, billing; quote arrow ids: ig apply 'web->api')"
        );
        assert_eq!(
            listing(&one),
            "ig apply: 1 to build\n  billing->tiny  to build: billing calls tiny"
        );
    }

    #[test]
    fn nothing_to_build_is_no_item() {
        let v = view_of("");
        assert!(chosen(items(&v), &[]).unwrap().is_empty());
        let Err(e) = chosen(items(&v), &["billing".into()]) else {
            panic!()
        };
        assert!(e.to_string().contains("the plan's ids: none"), "{e}");
    }

    #[test]
    fn the_prompt_holds_the_skill_and_what_ig_says_of_each_item() {
        let v = view_of(BILLING);
        let p = prompt(&items(&v), false);
        assert!(!p.contains("{{") && !p.contains("name: apply"), "{p}");
        assert!(p.contains("# Build what the plan says"), "{p}");
        assert!(p.contains("You cannot run commands"), "{p}");
        assert!(
            p.contains(
                "- `billing->tiny`: to build: billing calls tiny\n  label: asks for prices\n  note: once per checkout"
            ),
            "{p}"
        );
        assert!(
            p.contains("- `billing`: to build: service billing (billing/**)\n  title: Billing\n  tech: TypeScript\n  context: Use Stripe."),
            "{p}"
        );
        assert!(prompt(&items(&v), true).contains("You can run commands (Bash)"));
    }

    #[test]
    fn the_agent_writes_code_outside_ig_and_runs_no_command_unless_allowed() {
        let a = argv("{}", false);
        let at = |flag: &str| a[a.iter().position(|x| x == flag).unwrap() + 1].as_str();
        assert_eq!(
            at("--allowed-tools"),
            "Edit,Write,mcp__ig__ig_plan,mcp__ig__ig_state"
        );
        assert_eq!(
            at("--tools"),
            "Read,Grep,Glob,Edit,Write,mcp__ig__ig_plan,mcp__ig__ig_state"
        );
        let denied: Vec<&str> = (a.iter())
            .skip_while(|x| *x != "--disallowed-tools")
            .skip(1)
            .map(String::as_str)
            .collect();
        for r in [
            "Edit(ig/**)",
            "Write(ig/**)",
            "Edit(ig.yaml)",
            "Write(ig.yaml)",
        ] {
            assert!(denied.contains(&r), "{denied:?}");
        }
        assert!(denied.contains(&"Read(**/.env*)"), "{denied:?}");
        assert!(a.iter().all(|x| !x.contains("Bash") && x != "--safe-mode"));
        assert!(
            a.contains(&"--restricted".to_string())
                && a.contains(&"--strict-mcp-config".to_string())
        );
        let b = argv("{}", true);
        let at = |flag: &str| b[b.iter().position(|x| x == flag).unwrap() + 1].as_str();
        assert!(at("--allowed-tools").ends_with(",Bash") && at("--tools").ends_with(",Bash"));
        for r in KEEP_OUT_OF_BASH {
            assert!(b.contains(&r.to_string()), "{b:?}");
        }
    }
}
