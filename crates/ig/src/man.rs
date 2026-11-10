use ig_app::map::arrows::TELLING;
use ig_app::map::kinds::Kinds;

const SOURCE: &str = include_str!("../assets/ig-map.md");
const PAGE: &str = "assets/ig-map.5";

fn esc(s: &str) -> String {
    s.replace('\\', "\\e").replace('-', "\\-")
}

fn inline(s: &str) -> String {
    s.split('`')
        .enumerate()
        .map(|(i, part)| match i % 2 {
            1 => format!("\\fB{}\\fR", esc(part)),
            _ => esc(part),
        })
        .collect()
}

fn line(s: String) -> String {
    match s.starts_with(['.', '\'']) {
        true => format!("\\&{s}"),
        false => s,
    }
}

fn roff(md: &str) -> String {
    let mut out = String::new();
    let mut lines = md.lines();
    let (name, what) = lines
        .next()
        .and_then(|l| l.strip_prefix("# "))
        .and_then(|l| l.split_once(" - "))
        .expect("the page opens with `# name - summary`");
    out += &format!(
        ".TH {} 5\n.SH NAME\n{} \\- {}\n",
        name.to_uppercase(),
        esc(name),
        esc(what)
    );
    let mut code = false;
    let mut para = true;
    for l in lines {
        if l == "```" {
            out += if code {
                ".fi\n.RE\n"
            } else {
                ".PP\n.RS 4\n.nf\n"
            };
            code = !code;
            para = !code;
        } else if code {
            out += &line(esc(l));
            out += "\n";
        } else if let Some(h) = l.strip_prefix("## ") {
            out += &format!(".SH {h}\n");
            para = false;
        } else if l.is_empty() {
            para = true;
        } else if let Some(item) = l.strip_prefix("- ") {
            out += &format!(".IP \\(bu 2\n{}\n", line(inline(item)));
            para = false;
        } else {
            if para {
                out += ".PP\n";
            }
            out += &line(inline(l));
            out += "\n";
            para = false;
        }
    }
    out
}

#[test]
fn the_man_page_is_rendered_from_the_reference() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(PAGE);
    let page = roff(SOURCE);
    if std::env::var_os("IG_UPDATE_MAN").is_some() {
        std::fs::write(&path, &page).unwrap();
    }
    let on_disk = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        on_disk == page,
        "{PAGE} is out of date: run IG_UPDATE_MAN=1 cargo test -p intent-graph man"
    );
}

#[test]
fn the_reference_names_every_kind_and_arrow() {
    for k in Kinds::builtin().views() {
        assert!(
            SOURCE.contains(&format!("- `{}`:", k.name)),
            "kind {}",
            k.name
        );
        for p in k.props {
            assert!(
                SOURCE.contains(&format!("`{}`", p.name)),
                "property {}",
                p.name
            );
        }
    }
    for a in TELLING {
        let name = serde_json::to_value(a).unwrap();
        let name = name.as_str().unwrap();
        assert!(SOURCE.contains(&format!("- `{name}`:")), "arrow {name}");
    }
}

#[test]
fn roff_escapes_dashes_and_leading_dots() {
    assert_eq!(
        roff("# a-b - c\n\n.x `-q`\n"),
        ".TH A-B 5\n.SH NAME\na\\-b \\- c\n.PP\n\\&.x \\fB\\-q\\fR\n"
    );
}
