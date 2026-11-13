use ig_app::map::origin::{Item, Plan, Step};
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub fn json(p: &Plan) -> Value {
    json!({"build": p.build, "move": p.moves, "drift": p.drift, "notRead": p.not_read})
}

pub fn color() -> bool {
    use std::io::IsTerminal;
    std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none_or(|v| v.is_empty())
}

struct Paint(bool);

impl Paint {
    fn on(&self, code: &str, s: &str) -> String {
        match self.0 {
            true => format!("\x1b[{code}m{s}\x1b[0m"),
            false => s.to_string(),
        }
    }
}

const SECTIONS: [(Step, &str, &str, &str, &str); 4] = [
    (
        Step::Build,
        "To build: in ig/, not in the code",
        "ig apply builds them",
        "+",
        "32",
    ),
    (
        Step::Move,
        "To move: code in the wrong box",
        "ig apply moves it",
        "~",
        "33",
    ),
    (
        Step::Drift,
        "Drift: in the code, not in ig/",
        "ig accept <id> takes it into ig/, or change the code",
        "?",
        "31",
    ),
    (
        Step::NotRead,
        "Not read: ig cannot see this code",
        "add it to a workspace or to `ignore` in ig.yaml",
        "-",
        "2",
    ),
];

fn quoted(id: &str) -> String {
    match id
        .chars()
        .all(|c| c.is_alphanumeric() || "-_./".contains(c))
    {
        true => id.to_string(),
        false => format!("'{id}'"),
    }
}

pub fn pretty(p: &Plan, color: bool) -> String {
    let c = Paint(color);
    let count = |s: Step| p.items.iter().filter(|i| i.step == s).count();
    let (build, moves, drift, unread) = (
        count(Step::Build),
        count(Step::Move),
        count(Step::Drift),
        count(Step::NotRead),
    );
    if p.items.is_empty() {
        return format!(
            "{} The code and ig/ agree.\n",
            c.on("1", "Plan: nothing to do.")
        );
    }
    let mut out = String::new();
    for (step, title, hint, mark, code) in SECTIONS {
        let items: Vec<&Item> = p.items.iter().filter(|i| i.step == step).collect();
        if items.is_empty() {
            continue;
        }
        out += &format!("{}    {}\n", c.on("1", title), c.on("2", hint));
        let mut groups: BTreeMap<&str, Vec<&Item>> = BTreeMap::new();
        let mut loose = vec![];
        for i in items {
            match &i.under {
                Some(g) => groups.entry(g).or_default().push(i),
                None => loose.push(i),
            }
        }
        let width = (loose.iter().map(|i| 4 + i.what.chars().count()))
            .chain(
                groups
                    .values()
                    .flatten()
                    .map(|i| 6 + i.what.chars().count()),
            )
            .max()
            .unwrap_or(0)
            .min(56);
        let line = |indent: usize, i: &Item| {
            let head = format!("{}{} {}", " ".repeat(indent), c.on(code, mark), i.what);
            let pad = width.saturating_sub(indent + 2 + i.what.chars().count()) + 2;
            match step {
                Step::NotRead => format!("{head}\n"),
                Step::Move => format!(
                    "{}{} {}\n      {}\n",
                    " ".repeat(indent),
                    c.on(code, mark),
                    i.id,
                    i.what
                ),
                _ => format!("{head}{}{}\n", " ".repeat(pad), c.on("2", &quoted(&i.id))),
            }
        };
        for i in &loose {
            out += &line(2, i);
        }
        for (g, list) in &groups {
            out += &format!("  {}\n", c.on(code, g));
            for i in list {
                out += &line(4, i);
            }
        }
        out += "\n";
    }
    let mut sum = format!("Plan: {build} to build, {moves} to move, {drift} drift");
    if unread > 0 {
        sum += &format!(", {unread} not read");
    }
    out += &format!("{}.\n", c.on("1", &sum));
    let mut next: Vec<(String, String)> = vec![];
    if build + moves > 0 {
        next.push((
            "ig apply".into(),
            "build and move with Claude (asks first)".into(),
        ));
    }
    let ids: Vec<String> = (p.items.iter())
        .filter(|i| i.step == Step::Drift)
        .map(|i| quoted(&i.id))
        .collect();
    match ids.len() {
        0 => {}
        1 => next.push((format!("ig accept {}", ids[0]), "take it into ig/".into())),
        n => {
            next.push(("ig accept --all".into(), format!("take all {n} into ig/")));
            next.push((format!("ig accept {} …", ids[0]), "take some".into()));
        }
    }
    if drift > 0 {
        next.push(("ig view".into(), "see them on the map".into()));
    }
    if !next.is_empty() {
        let w = next
            .iter()
            .map(|(a, _)| a.chars().count())
            .max()
            .unwrap_or(0);
        out += "\nNext:\n";
        for (cmd, why) in next {
            let pad = w - cmd.chars().count() + 3;
            out += &format!(
                "  {}{}{}\n",
                c.on("1", &cmd),
                " ".repeat(pad),
                c.on("2", &why)
            );
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(step: Step, id: &str, under: Option<&str>, what: &str) -> Item {
        Item {
            step,
            id: id.into(),
            under: under.map(String::from),
            what: what.into(),
        }
    }

    fn plan(items: Vec<Item>) -> Plan {
        Plan {
            build: vec![],
            moves: vec![],
            drift: vec![],
            not_read: vec![],
            items,
        }
    }

    #[test]
    fn an_empty_plan_says_the_two_agree() {
        assert_eq!(
            pretty(&plan(vec![]), false),
            "Plan: nothing to do. The code and ig/ agree.\n"
        );
    }

    #[test]
    fn drift_groups_under_its_source_and_says_what_to_run() {
        let p = plan(vec![
            item(
                Step::Drift,
                "front/a->front/ui",
                Some("front/a"),
                "bundle front/ui",
            ),
            item(
                Step::Drift,
                "front/a->front/m",
                Some("front/a"),
                "uses front/m",
            ),
            item(Step::Drift, "postgres", None, "store postgres"),
        ]);
        let t = pretty(&p, false);
        assert_eq!(
            t,
            "Drift: in the code, not in ig/    ig accept <id> takes it into ig/, or change the code\n  \
             ? store postgres     postgres\n  \
             front/a\n    \
             ? bundle front/ui  'front/a->front/ui'\n    \
             ? uses front/m     'front/a->front/m'\n\n\
             Plan: 0 to build, 0 to move, 3 drift.\n\n\
             Next:\n  \
             ig accept --all                   take all 3 into ig/\n  \
             ig accept 'front/a->front/ui' …   take some\n  \
             ig view                           see them on the map\n"
        );
    }

    #[test]
    fn build_move_and_unread_each_get_a_section_and_colour_only_when_asked() {
        let p = plan(vec![
            item(Step::Build, "billing", None, "service billing (billing/**)"),
            item(Step::Move, "api/refund", None, "a/** → b/**"),
            item(Step::NotRead, "scripts/**", None, "scripts/**"),
        ]);
        let t = pretty(&p, false);
        assert!(
            t.contains("  + service billing (billing/**)  billing\n"),
            "{t}"
        );
        assert!(t.contains("  ~ api/refund\n      a/** → b/**\n"), "{t}");
        assert!(t.contains("  - scripts/**\n"), "{t}");
        assert!(
            t.contains("Plan: 1 to build, 1 to move, 0 drift, 1 not read."),
            "{t}"
        );
        assert!(t.contains("ig apply   build and move"), "{t}");
        assert!(!t.contains('\x1b'));
        assert!(pretty(&p, true).contains("\x1b[32m+\x1b[0m"));
    }
}
