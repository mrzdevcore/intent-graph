//! A folder of projects: which of its folders are projects, and which name each other.

use crate::BUILD_DIRS;
use ig_core::workspace::NEVER_WALKED;
use std::collections::BTreeSet;
use std::path::Path;

#[derive(Debug, Clone, PartialEq)]
pub struct Folder {
    pub dir: String,
    pub why: String,
    pub ground: bool,
}

#[derive(Debug, Default, PartialEq)]
pub struct Folders {
    pub projects: Vec<Folder>,
    pub unsure: Vec<Folder>,
    pub left_out: Vec<Folder>,
}

#[derive(Debug, PartialEq)]
pub struct Mention {
    pub from: String,
    pub to: String,
    pub files: Vec<String>,
}

const MANIFESTS: [&str; 12] = [
    "package.json",
    "go.mod",
    "Cargo.toml",
    "pyproject.toml",
    "requirements.txt",
    "pom.xml",
    "composer.json",
    "nx.json",
    "Dockerfile",
    "docker-compose.yml",
    "compose.yaml",
    "main.tf",
];
const SOURCE: [&str; 11] = [
    "ts", "tsx", "vue", "js", "py", "rs", "go", "java", "kt", "cs", "php",
];
const ARCHIVED: [&str; 4] = ["archive", "archives", "old", "backup"];
const GROUND: [&str; 7] = [
    "infra",
    "infrastructure",
    "iac",
    "vpc",
    "terraform",
    "ansible",
    "k8s",
];
const MAX_READ: u64 = 1 << 20;

fn skipped_dir(name: &str) -> bool {
    name.starts_with('.')
        || name == "ig"
        || NEVER_WALKED.contains(&name)
        || BUILD_DIRS.contains(&name)
}

fn release(name: &str) -> bool {
    name.rsplit_once('-').is_some_and(|(_, v)| {
        v.contains('.')
            && v.split('.')
                .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
    })
}

fn walk(dir: &Path, depth: usize, on: &mut dyn FnMut(&Path)) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        let Ok(t) = e.file_type() else { continue };
        if t.is_dir() {
            if depth > 0 && !skipped_dir(&e.file_name().to_string_lossy()) {
                walk(&p, depth - 1, on);
            }
        } else if t.is_file() {
            on(&p);
        }
    }
}

fn counts(dir: &Path) -> (usize, usize) {
    let (mut source, mut tf) = (0, 0);
    walk(
        dir,
        8,
        &mut |p| match p.extension().and_then(|e| e.to_str()) {
            Some("tf") => tf += 1,
            Some(e) if SOURCE.contains(&e) => source += 1,
            _ => {}
        },
    );
    (source, tf)
}

fn is_ground(name: &str, dir: &Path) -> bool {
    dir.join("ansible").is_dir()
        || name
            .split(['-', '_', '.'])
            .any(|w| GROUND.contains(&w.to_ascii_lowercase().as_str()))
}

// git is one signal among others: many real projects are not repos
pub fn scan(root: &Path) -> Folders {
    let mut out = Folders::default();
    let Ok(rd) = std::fs::read_dir(root) else {
        return out;
    };
    let mut names: Vec<String> = rd
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| !skipped_dir(n))
        .collect();
    names.sort();
    for name in names {
        let dir = root.join(&name);
        let git = dir.join(".git");
        let of = |why: String, ground: bool| Folder {
            dir: name.clone(),
            why,
            ground,
        };
        if git.is_file() {
            let text = std::fs::read_to_string(&git).unwrap_or_default();
            let main = text
                .split("/.git/worktrees/")
                .next()
                .and_then(|p| p.rsplit('/').next())
                .unwrap_or("another repo");
            out.left_out
                .push(of(format!("A worktree of {main}"), false));
            continue;
        }
        if ARCHIVED.contains(&name.to_ascii_lowercase().as_str()) {
            out.left_out.push(of("Named as an archive".into(), false));
            continue;
        }
        let ground = is_ground(&name, &dir);
        let manifest = MANIFESTS.iter().find(|m| dir.join(m).is_file());
        let why = if git.is_dir() {
            Some(manifest.map_or("Git repo".to_string(), |m| format!("Git repo, {m}")))
        } else if dir.join("ig").is_dir() {
            Some("Has its own ig/".to_string())
        } else {
            manifest.map(|m| format!("No git; {m}"))
        };
        if let Some(why) = why {
            out.projects.push(of(why, ground));
            continue;
        }
        if release(&name) {
            out.left_out.push(of("A downloaded release".into(), false));
            continue;
        }
        let files =
            |n: usize, what: &str| format!("{n} {what} {}", if n == 1 { "file" } else { "files" });
        match counts(&dir) {
            (_, tf) if tf > 0 => out
                .projects
                .push(of(format!("No git; {}", files(tf, "Terraform")), ground)),
            (0, 0) => out.left_out.push(of("No source".into(), false)),
            (s, _) if s < 3 => out
                .unsure
                .push(of(format!("{}, no manifest", files(s, "source")), false)),
            (s, _) => out
                .projects
                .push(of(format!("No git; {}", files(s, "source")), ground)),
        }
    }
    out
}

pub fn is_landscape(root: &Path) -> bool {
    let own = root.join("ig").is_dir()
        || root.join(".git").exists()
        || MANIFESTS.iter().any(|m| root.join(m).is_file());
    !own && scan(root).projects.len() >= 2
}

// never opened: they can hold secrets, and a link needs no secret to be seen
fn private(name: &str) -> bool {
    name.starts_with(".env")
        || name.ends_with(".env")
        || name.contains(".tfstate")
        || name.ends_with(".tfvars")
        || name.ends_with(".pem")
        || name.ends_with(".key")
        || name.starts_with("secrets")
        || name.starts_with("credentials")
}

fn lockfile(name: &str) -> bool {
    matches!(
        name,
        "pnpm-lock.yaml"
            | "package-lock.json"
            | "yarn.lock"
            | "bun.lock"
            | "go.sum"
            | "Cargo.lock"
            | "poetry.lock"
            | "uv.lock"
    )
}

// `llk-ody-${env}` is that project; the `powerbi-client` package is not `powerbi`
fn names(text: &str, name: &str) -> bool {
    let word = |c: char| c.is_alphanumeric() || c == '_';
    let open = |first: Option<char>, second: Option<char>| match first {
        Some('-') => !second.is_some_and(word),
        Some(c) => !word(c),
        None => true,
    };
    text.match_indices(name).any(|(i, _)| {
        let (mut before, mut after) = (text[..i].chars().rev(), text[i + name.len()..].chars());
        open(before.next(), before.next()) && open(after.next(), after.next())
    })
}

pub fn mentions(root: &Path, projects: &[String]) -> Vec<Mention> {
    let mut out = vec![];
    for from in projects {
        let base = root.join(from);
        let mut found: Vec<BTreeSet<String>> = vec![BTreeSet::new(); projects.len()];
        walk(&base, 32, &mut |p| {
            let name = p
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            if private(&name)
                || lockfile(&name)
                || std::fs::metadata(p).map_or(true, |m| m.len() > MAX_READ)
            {
                return;
            }
            let Ok(bytes) = std::fs::read(p) else { return };
            if bytes.contains(&0) {
                return;
            }
            let text = String::from_utf8_lossy(&bytes);
            for (i, to) in projects.iter().enumerate() {
                if to != from && names(&text, to) {
                    found[i].insert(
                        p.strip_prefix(&base)
                            .unwrap_or(p)
                            .to_string_lossy()
                            .into_owned(),
                    );
                }
            }
        });
        for (i, files) in found.into_iter().enumerate() {
            if !files.is_empty() {
                out.push(Mention {
                    from: from.clone(),
                    to: projects[i].clone(),
                    files: files.into_iter().collect(),
                });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn tree(tag: &str, files: &[(&str, &str)]) -> PathBuf {
        let d = std::env::temp_dir().join(format!("ig-folders-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        for (p, s) in files {
            let f = d.join(p);
            std::fs::create_dir_all(f.parent().unwrap()).unwrap();
            std::fs::write(f, s).unwrap();
        }
        d
    }
    fn dirs(fs: &[Folder]) -> Vec<&str> {
        fs.iter().map(|f| f.dir.as_str()).collect()
    }

    #[test]
    fn a_project_need_not_be_a_repo_and_a_repo_s_worktree_is_not_a_project() {
        let d = tree(
            "sort",
            &[
                ("api/.git/HEAD", "x"),
                ("api/go.mod", "module a"),
                ("web/package.json", "{}"),
                (
                    "web-DM-1/.git",
                    "gitdir: /x/clients/web/.git/worktrees/web-DM-1\n",
                ),
                ("web-DM-1/package.json", "{}"),
                ("authf/main.ts", "x"),
                ("authf/deploy/a.tf", "x"),
                ("karuka/ig/system.kdl", "x"),
                ("archive/old/a.ts", "x"),
                ("keycloak-22.0.4/lib/a.jar", "x"),
                ("documents/a.pdf", "x"),
                ("pipeline/run.py", "x"),
                ("big/a.py", "x"),
                ("big/b.py", "x"),
                ("big/c.py", "x"),
                ("node_modules/x/package.json", "{}"),
                (".claude/x.json", "{}"),
                ("ig/system.kdl", "x"),
            ],
        );
        let f = scan(&d);
        assert_eq!(dirs(&f.projects), ["api", "authf", "big", "karuka", "web"]);
        assert_eq!(dirs(&f.unsure), ["pipeline"]);
        assert_eq!(f.unsure[0].why, "1 source file, no manifest");
        assert_eq!(
            dirs(&f.left_out),
            ["archive", "documents", "keycloak-22.0.4", "web-DM-1"]
        );
        assert_eq!(f.left_out[3].why, "A worktree of web");
        assert_eq!(f.projects[0].why, "Git repo, go.mod");
        assert_eq!(f.projects[1].why, "No git; 1 Terraform file");
    }

    #[test]
    fn infrastructure_is_known_by_its_name_or_its_ansible() {
        let d = tree(
            "ground",
            &[
                ("llk-infra/a.tf", "x"),
                ("llk-vpc/main.tf", "x"),
                ("llk-iac/go.mod", "x"),
                ("commune/.git/HEAD", "x"),
                ("commune/ansible/site.yml", "x"),
                ("llk-authz/a.tf", "x"),
                ("infravision/package.json", "{}"),
            ],
        );
        let f = scan(&d);
        let ground: Vec<_> = f
            .projects
            .iter()
            .filter(|p| p.ground)
            .map(|p| p.dir.as_str())
            .collect();
        assert_eq!(ground, ["commune", "llk-iac", "llk-infra", "llk-vpc"]);
    }

    #[test]
    fn a_folder_is_a_landscape_only_when_it_is_no_project_itself() {
        let two = [("a/go.mod", "x"), ("b/go.mod", "x")];
        assert!(is_landscape(&tree("yes", &two)));
        assert!(!is_landscape(&tree("one", &two[..1])));
        for own in ["ig/system.kdl", ".git/HEAD", "package.json"] {
            let d = tree(&format!("own{}", own.len()), &[two[0], two[1], (own, "x")]);
            assert!(!is_landscape(&d), "{own}");
        }
    }

    #[test]
    fn mentions_are_names_in_readable_files_never_in_secrets() {
        let d = tree(
            "names",
            &[
                ("web/src/config.ts", "const u = 'https://api/x'; // api"),
                ("web/package.json", r#"{"dependencies":{"api-client":"1"}}"#),
                ("web/.env", "URL=db"),
                ("web/prod.tfvars", "db"),
                ("web/terraform.tfstate", "db"),
                ("web/pnpm-lock.yaml", "db"),
                ("web/node_modules/x/a.js", "db"),
                ("web/logo.bin", "db\0"),
                ("api/README.md", "not the web-app"),
                ("api/fly.toml", "app = 'db-${env}'"),
                ("db/README.md", "x"),
            ],
        );
        let m = mentions(&d, &["api".into(), "db".into(), "web".into()]);
        assert_eq!(
            m,
            [
                Mention {
                    from: "api".into(),
                    to: "db".into(),
                    files: vec!["fly.toml".into()]
                },
                Mention {
                    from: "web".into(),
                    to: "api".into(),
                    files: vec!["src/config.ts".into()]
                },
            ]
        );
    }
}
