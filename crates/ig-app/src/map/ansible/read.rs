//! Reading an Ansible tree as text: Jinja makes much of it invalid YAML, so tasks are the
//! top-level list items of a tasks file, and a template's links are its settings' names.

use ig_core::ports::Files;
use regex::Regex;
use std::collections::BTreeSet;
use std::sync::LazyLock;

static NAME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"^-?\s*name\s*:\s*["']?(.*?)["']?\s*$"#).unwrap());
static ROLE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?m)(?:(?:include|import)_role\s*:\s*\n\s+name|^\s*-?\s*role)\s*:\s*["']?([\w.\-]+)"#,
    )
    .unwrap()
});
static INLINE_ROLE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?:include|import)_role\s*:\s*["']?name=([\w.\-]+)"#).unwrap());
static FILE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?:(?:include|import)_tasks\s*:\s*(?:file\s*:\s*)?["']?([^\s"'{}]+)|lookup\(\s*['"]template['"]\s*,\s*['"]([^'"]+)['"]|\bsrc\s*:\s*["']?([^\s"'{}]+\.j2))"#,
    )
    .unwrap()
});
static SETTING: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^\s*([A-Z][A-Z0-9_]*)\s*[:=]").unwrap());
static VAR: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\{\{\s*([a-z][a-z0-9_]*?)_(?:url|host|endpoint|uri)\s*\}\}").unwrap()
});
static LIST_ITEM: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?m)^-\s+["']?([\w.\-]+)["']?\s*$"#).unwrap());

/// One top-level task (or handler) of a tasks file.
#[derive(Debug, Clone, PartialEq)]
pub struct Task {
    pub name: String,
    /// 1-based, inclusive.
    pub lines: (u32, u32),
    /// Roles it includes or imports.
    pub roles: Vec<String>,
    /// Files it includes or renders (tasks files, templates), as written.
    pub files: Vec<String>,
}

/// The top-level items of a tasks file: from a line starting `- ` at column 0 to the last
/// non-blank, non-comment line before the next.
pub fn tasks(text: &str) -> Vec<Task> {
    let lines: Vec<&str> = text.lines().collect();
    let starts: Vec<usize> = (0..lines.len())
        .filter(|&i| lines[i].starts_with("- ") || lines[i] == "-")
        .collect();
    let mut out = vec![];
    for (k, &s) in starts.iter().enumerate() {
        let next = starts.get(k + 1).copied().unwrap_or(lines.len());
        let mut e = next;
        while e > s + 1 && {
            let l = lines[e - 1].trim();
            l.is_empty() || l.starts_with('#') || l == "---"
        } {
            e -= 1;
        }
        let body = lines[s..e].join("\n");
        let name = lines[s..e]
            .iter()
            .take_while(|l| !l.trim_start().starts_with("block:"))
            .filter(|l| l.starts_with("- ") || l.starts_with("  ") && !l.starts_with("   "))
            .find_map(|l| NAME.captures(l.trim_start_matches("- ").trim()))
            .map(|c| c[1].to_string())
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| format!("task {}", k + 1));
        let mut roles: Vec<String> = ROLE
            .captures_iter(&body)
            .chain(INLINE_ROLE.captures_iter(&body))
            .map(|c| c[1].to_string())
            .collect();
        roles.dedup();
        let files = FILE
            .captures_iter(&body)
            .filter_map(|c| {
                (1..=3)
                    .find_map(|i| c.get(i))
                    .map(|m| m.as_str().to_string())
            })
            .collect();
        out.push(Task {
            name,
            lines: (s as u32 + 1, e as u32),
            roles,
            files,
        });
    }
    out
}

/// The `- item` lines of a YAML list (a role's `meta/targets.yml`).
pub fn list(text: &str) -> Vec<String> {
    LIST_ITEM
        .captures_iter(text)
        .map(|c| c[1].to_string())
        .collect()
}

/// What follows `roles/` in a path, when a folder of that name is in it.
pub fn under_roles(path: &str) -> Option<&str> {
    let i = path
        .find("roles/")
        .filter(|&i| i == 0 || path[..i].ends_with('/'))?;
    Some(&path[i + 6..])
}

/// A role's standard folders; the first in a path under `roles/` ends the role's own.
const STANDARD: &[&str] = &[
    "tasks",
    "meta",
    "handlers",
    "templates",
    "files",
    "defaults",
    "vars",
    "library",
];

/// The folder of each role among `files`: what holds a `tasks/` or `meta/` file, at any depth
/// under `roles/` (`roles/services/gitlab`).
pub fn role_dirs(files: &[String]) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for f in files {
        let Some(rest) = under_roles(f) else { continue };
        let parts: Vec<&str> = rest.split('/').collect();
        let last = parts.len().saturating_sub(1);
        let Some(i) = (1..last).find(|&i| STANDARD.contains(&parts[i])) else {
            continue;
        };
        if matches!(parts[i], "tasks" | "meta") {
            let cut = f.len() - rest.len() + parts[..i].join("/").len();
            out.insert(f[..cut].to_string());
        }
    }
    out
}

/// The plays of a playbook: each top-level item with `hosts:`, and the roles it applies as
/// written (`services/gitlab`). Any other file has none.
pub fn plays(text: &str) -> Vec<(String, Vec<String>)> {
    static HOSTS: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"^(?:- +| +)hosts\s*:\s*["']?([^"'\s#]+)"#).unwrap());
    static ITEM: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"^-\s+(?:\{\s*)?(?:role\s*:\s*)?["']?([\w./\-]+)"#).unwrap());
    let mut out: Vec<(String, Vec<String>)> = vec![];
    // the indent of the `roles:` being read, and whether the play has `hosts:` yet
    let (mut list, mut hosts, mut roles) = (None, None, vec![]);
    let mut flush = |hosts: &mut Option<String>, roles: &mut Vec<String>| {
        if let Some(h) = hosts.take() {
            out.push((h, std::mem::take(roles)));
        }
        roles.clear();
    };
    for l in text.lines() {
        if l.starts_with("- ") {
            flush(&mut hosts, &mut roles);
            list = None;
        }
        let body = l.trim_start();
        let indent = l.len() - body.len();
        if let Some(c) = HOSTS.captures(l) {
            hosts = Some(c[1].to_string());
        }
        match list {
            Some(at) if !body.is_empty() && indent <= at && !body.starts_with('-') => list = None,
            Some(at) if indent >= at => {
                let c = ITEM.captures(body);
                roles.extend(c.map(|c| c[1].to_string()));
            }
            _ => {}
        }
        if list.is_none() && body.trim_start_matches("- ").trim_end() == "roles:" {
            list = Some(if l.starts_with("- ") { 2 } else { indent });
        }
    }
    flush(&mut hosts, &mut roles);
    out
}

/// The plays of every playbook among `files`, outside `roles/`.
pub fn playbooks(files: &dyn Files) -> Vec<(String, Vec<String>)> {
    let all = files
        .glob(&["**/*.yml".into(), "**/*.yaml".into()])
        .unwrap_or_default();
    (all.iter().filter(|f| under_roles(f).is_none()))
        .filter_map(|f| files.read(f))
        .flat_map(|t| plays(&t))
        .collect()
}

/// The hosts of the plays that apply the role at `rel` (`services/gitlab`, or by its name alone).
pub fn deployed_to(plays: &[(String, Vec<String>)], rel: &str) -> Vec<String> {
    let name = rel.rsplit('/').next().unwrap_or(rel);
    let mut out: Vec<String> = (plays.iter())
        .filter(|(_, roles)| roles.iter().any(|r| r == rel || r == name))
        .map(|(h, _)| h.clone())
        .collect();
    out.sort();
    out.dedup();
    out
}

/// A front end's public prefixes, left off a setting's name.
const PUBLIC: &[&str] = &[
    "NUXT_PUBLIC_",
    "NEXT_PUBLIC_",
    "VITE_",
    "VUE_APP_",
    "REACT_APP_",
];

/// The upper-case setting names of a template (`PRODUCT_API_HOST: …`, `NUXT_PUBLIC_` and the
/// like left off), each with its line.
pub fn settings(text: &str) -> Vec<(String, u32)> {
    let starts: Vec<usize> = std::iter::once(0)
        .chain(text.match_indices('\n').map(|(i, _)| i + 1))
        .collect();
    SETTING
        .captures_iter(text)
        .map(|c| {
            let at = c.get(1).unwrap().start();
            let key = PUBLIC.iter().find_map(|p| c[1].strip_prefix(p));
            (
                key.unwrap_or(&c[1]).to_string(),
                starts.partition_point(|&s| s <= at) as u32,
            )
        })
        .collect()
}

/// The service names a setting or variable points at: `PRODUCT_API_HOST` and
/// `{{ product_api_url }}` both name `product_api`, then `product`.
pub fn services(text: &str) -> Vec<(String, u32)> {
    let mut out = vec![];
    for (key, line) in settings(text) {
        let k = key.to_ascii_lowercase();
        let Some(base) = ["_base_url", "_url", "_host", "_endpoint", "_uri"]
            .iter()
            .find_map(|s| k.strip_suffix(s))
        else {
            continue;
        };
        out.push((base.to_string(), line));
    }
    let starts: Vec<usize> = std::iter::once(0)
        .chain(text.match_indices('\n').map(|(i, _)| i + 1))
        .collect();
    for c in VAR.captures_iter(text) {
        let at = c.get(0).unwrap().start();
        out.push((
            c[1].to_string(),
            starts.partition_point(|&s| s <= at) as u32,
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plays_read_every_way_of_naming_a_role() {
        let text = "---\n- name: Web\n  hosts: web\n  vars:\n    - not_a_role\n  roles:\n    - a\n    - role: services/b\n      tags: x\n    - { role: c, tags: y }\n  tasks:\n    - debug: {}\n\n- hosts: \"db\"\n  roles:\n  - d\n- import_playbook: other.yml\n";
        let want = [("web", vec!["a", "services/b", "c"]), ("db", vec!["d"])].map(|(h, r)| {
            (
                h.to_string(),
                r.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
            )
        });
        assert_eq!(plays(text), want);
    }

    #[test]
    fn a_role_folder_ends_at_its_first_standard_folder() {
        let files: Vec<String> = [
            "roles/a/tasks/main.yml",
            "x/roles/group/b/meta/main.yml",
            "x/roles/group/README.md",
            "roles/c/files/tasks/not-a-role.yml",
            "roles/d/tasks",
            "myroles/e/tasks/main.yml",
        ]
        .map(String::from)
        .to_vec();
        let dirs: Vec<String> = role_dirs(&files).into_iter().collect();
        assert_eq!(dirs, ["roles/a", "x/roles/group/b"]);
    }

    const TASKS: &str = "---
- name: Deploy payment API frontoffice
  include_role:
    name: helper_deploy_helm_chart
  vars:
    values: \"{{ lookup('template', 'values-frontoffice.yml.j2') | from_yaml }}\"

# the back office
- name: \"Deploy backoffice\"
  import_tasks: backoffice.yml

- include_role: name=helper_helm_repo_install
- block:
    - name: inner
      template:
        src: cron.yml.j2
";

    #[test]
    fn tasks_are_top_level_items_with_what_they_use() {
        let t = tasks(TASKS);
        let names: Vec<&str> = t.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "Deploy payment API frontoffice",
                "Deploy backoffice",
                "task 3",
                "task 4"
            ]
        );
        assert_eq!(t[0].lines, (2, 6));
        assert_eq!(t[0].roles, ["helper_deploy_helm_chart"]);
        assert_eq!(t[0].files, ["values-frontoffice.yml.j2"]);
        assert_eq!(t[1].lines, (9, 10));
        assert_eq!(t[1].files, ["backoffice.yml"]);
        assert_eq!(t[2].roles, ["helper_helm_repo_install"]);
        assert_eq!(t[3].files, ["cron.yml.j2"]);
    }

    #[test]
    fn templates_name_the_services_they_call() {
        let v = "data:\n  PRODUCT_API_HOST: \"{{ api_url }}\"\n  LOGGER_LEVEL: x\n  url: \"{{ catalog_api_url }}\"\n  NUXT_PUBLIC_CART_API_URL: x\n";
        assert_eq!(
            services(v),
            [
                ("product_api".to_string(), 2),
                ("cart_api".to_string(), 5),
                ("api".to_string(), 2),
                ("catalog_api".to_string(), 4)
            ]
        );
        assert_eq!(settings(v)[1], ("LOGGER_LEVEL".into(), 3));
        assert_eq!(
            list("---\n- appro_platform\n- \"retail\"\n"),
            ["appro_platform", "retail"]
        );
    }
}
