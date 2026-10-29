//! Reading an Ansible tree as text: Jinja makes much of it invalid YAML, so tasks are the
//! top-level list items of a tasks file, and a template's links are its settings' names.

use regex::Regex;
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
    LazyLock::new(|| Regex::new(r"(?m)^\s*([A-Z][A-Z0-9_]*)\s*:").unwrap());
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
