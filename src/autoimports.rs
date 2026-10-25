//! `autoImports`: names a framework makes global without an `import` (Nuxt's `.nuxt/imports.d.ts`
//! and `components.d.ts`, unplugin-auto-import), read from its generated declaration files.

use crate::model::ParseError;
use crate::workspace::{self, Workspace};
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::LazyLock;

#[derive(Default)]
pub struct AutoImports {
    /// Global name → (repo-relative file, export name).
    names: HashMap<String, (String, String)>,
    /// Names declared from a package (`export { useI18n } from 'vue-i18n'`): nothing in the repo.
    external: HashSet<String>,
    /// Source dir a declaration file's entries point into → that declaration file.
    dirs: HashMap<String, String>,
}

impl AutoImports {
    pub fn get(&self, name: &str) -> Option<(&str, &str)> {
        self.names.get(name).map(|(f, e)| (f.as_str(), e.as_str()))
    }

    pub fn is_external(&self, name: &str) -> bool {
        self.external.contains(name) && !self.names.contains_key(name)
    }

    /// The declaration file whose entries point into `dir`, if any.
    pub fn covering(&self, dir: &str) -> Option<&str> {
        self.dirs.get(dir).map(String::as_str)
    }
}

/// Repo-relative files matching `patterns` (sorted), and the patterns that matched none.
pub fn files(root: &Path, patterns: &[String]) -> (Vec<String>, Vec<String>) {
    let is_glob = |p: &str| p.contains(['*', '?', '[', '{']);
    let mut walked = vec![];
    if patterns.iter().any(|p| is_glob(p)) {
        // ponytail: walks the whole tree (generated dirs are hidden, so source discovery skips
        // them), skipping only node_modules and .git.
        // Symlinked dirs are followed, each real dir once (no cycles): a base-graph worktree
        // links a missing `.nuxt/` as a dir symlink (`link_missing_extends`).
        let mut seen = HashSet::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            if !dir.canonicalize().is_ok_and(|c| seen.insert(c)) {
                continue;
            }
            let mut entries: Vec<_> = std::fs::read_dir(&dir)
                .into_iter()
                .flatten()
                .flatten()
                .collect();
            entries.sort_by_key(|e| e.file_name());
            for e in entries {
                let p = e.path();
                if p.is_dir() {
                    if e.file_name() != "node_modules" && e.file_name() != ".git" {
                        stack.push(p);
                    }
                } else if let Some(r) = workspace::rel(root, &p) {
                    walked.push(r);
                }
            }
        }
    }
    let (mut found, mut unmatched) = (vec![], vec![]);
    for p in patterns {
        let before = found.len();
        if is_glob(p) {
            if let Ok(g) = workspace::glob(p).map(|g| g.compile_matcher()) {
                found.extend(walked.iter().filter(|r| g.is_match(r.as_str())).cloned());
            }
        } else if root.join(p).is_file() {
            found.push(workspace::normalize(p));
        }
        if found.len() == before {
            unmatched.push(p.clone());
        }
    }
    found.sort();
    found.dedup();
    (found, unmatched)
}

static REEXPORT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"export\s*\{([^}]*)\}\s*from\s*['"]([^'"]+)['"]"#).unwrap());
static TYPEOF: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"['"]?([A-Za-z_$][\w$]*)['"]?\s*:\s*\(?\s*typeof\s+import\(\s*['"]([^'"]+)['"]\s*\)\s*\)?\s*(?:\.([\w$]+)|\[\s*['"]([^'"]+)['"]\s*\])"#).unwrap()
});

/// Reads the declaration files `patterns` name. A missing or unreadable file is a parse error
/// (`.nuxt/` before `nuxt prepare`), never a failure.
pub fn load(root: &Path, patterns: &[String], ws: &Workspace) -> (AutoImports, Vec<ParseError>) {
    let mut ai = AutoImports::default();
    let (found, unmatched) = files(root, patterns);
    let mut errors: Vec<ParseError> = unmatched
        .into_iter()
        .map(|p| ParseError {
            file: p,
            message:
                "autoImports: no such declaration file (generate it first, e.g. `nuxt prepare`)"
                    .into(),
        })
        .collect();
    let known: HashSet<&str> = ws.files.iter().map(|f| f.path.as_str()).collect();
    for decl in found {
        let Ok(text) = std::fs::read_to_string(root.join(&decl)) else {
            errors.push(ParseError {
                file: decl,
                message: "autoImports: unreadable declaration file".into(),
            });
            continue;
        };
        let dir = decl.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
        let mut add = |name: &str, spec: &str, export: &str| {
            if !spec.starts_with('.') {
                ai.external.insert(name.to_string());
            }
            if let Some(file) = resolve(dir, spec, &known) {
                let src = file.rsplit_once('/').map_or("", |(d, _)| d);
                ai.dirs
                    .entry(src.to_string())
                    .or_insert_with(|| decl.clone());
                ai.names
                    .entry(name.to_string())
                    .or_insert((file, export.to_string()));
            }
        };
        for c in REEXPORT.captures_iter(&text) {
            for item in c[1].split(',') {
                let item = item.trim().trim_start_matches("type ").trim();
                let (orig, local) = match item.split_once(" as ") {
                    Some((o, l)) => (o.trim(), l.trim()),
                    None => (item, item),
                };
                if !local.is_empty() {
                    add(local, &c[2], orig);
                }
            }
        }
        for c in TYPEOF.captures_iter(&text) {
            let export = c.get(3).or(c.get(4)).map_or("", |m| m.as_str());
            add(&c[1], &c[2], export);
        }
    }
    (ai, errors)
}

/// A relative `spec` from the declaration file's dir → a workspace source file, probing extensions.
/// Bare packages, `#alias` specs and paths leaving the repo resolve to nothing.
fn resolve(dir: &str, spec: &str, known: &HashSet<&str>) -> Option<String> {
    if !spec.starts_with('.') {
        return None;
    }
    let mut parts: Vec<&str> = dir.split('/').filter(|s| !s.is_empty()).collect();
    for seg in spec.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            s => parts.push(s),
        }
    }
    let base = parts.join("/");
    let exts = [".ts", ".tsx", ".vue", ".js", ".jsx", ".mts", ".mjs"];
    std::iter::once(base.clone())
        .chain(exts.iter().map(|e| format!("{base}{e}")))
        .chain(exts.iter().map(|e| format!("{base}/index{e}")))
        .find(|c| known.contains(c.as_str()))
}

#[cfg(test)]
static TMP: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// A fresh temporary directory holding `files`.
#[cfg(test)]
pub fn tmp_with(files: &[(&str, &str)]) -> std::path::PathBuf {
    let n = TMP.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("ig-auto-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    for (p, s) in files {
        let f = dir.join(p);
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(f, s).unwrap();
    }
    dir
}

#[cfg(test)]
pub fn workspace_of(dir: &Path) -> Workspace {
    let config = crate::config::Config {
        embedded: vec![".vue".into()],
        ..Default::default()
    };
    Workspace::discover(dir, &config).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nuxt_forms() {
        let dir = tmp_with(&[
            (
                ".nuxt/imports.d.ts",
                "export { useCatalog, useA as useB } from '../src/composables/catalog';\nexport { useI18n } from 'vue-i18n';\n",
            ),
            (
                ".nuxt/types/imports.d.ts",
                "  const usePage: typeof import('../../src/composables/page')['usePage']\n",
            ),
            (
                ".nuxt/components.d.ts",
                "  'DashboardReport': typeof import(\"../src/components/DashboardReport.vue\")['default']\n",
            ),
            (
                "src/composables/catalog.ts",
                "export const useCatalog = () => 1; export const useA = () => 2;\n",
            ),
            ("src/composables/page.ts", "export function usePage() {}\n"),
            (
                "src/components/DashboardReport.vue",
                "<script setup lang=\"ts\"></script>\n",
            ),
        ]);
        let ws = workspace_of(&dir);
        let (ai, errs) = load(&dir, &[".nuxt/**/*.d.ts".into()], &ws);
        assert!(errs.is_empty());
        assert_eq!(
            ai.get("useCatalog"),
            Some(("src/composables/catalog.ts", "useCatalog"))
        );
        assert_eq!(ai.get("useB"), Some(("src/composables/catalog.ts", "useA")));
        assert_eq!(
            ai.get("usePage"),
            Some(("src/composables/page.ts", "usePage"))
        );
        assert_eq!(
            ai.get("DashboardReport"),
            Some(("src/components/DashboardReport.vue", "default"))
        );
        assert_eq!(ai.get("useI18n"), None); // Review Focus 2
    }

    #[cfg(unix)]
    #[test]
    fn glob_walk_follows_symlinked_dirs_without_looping() {
        // As in a base-graph worktree: `.nuxt` links to a dir outside the root.
        let outside = tmp_with(&[("imports.d.ts", "export {}\n")]);
        let dir = tmp_with(&[("src/a.ts", "")]);
        std::os::unix::fs::symlink(&outside, dir.join(".nuxt")).unwrap();
        std::os::unix::fs::symlink(&dir, dir.join("src/loop")).unwrap();
        let (found, unmatched) = files(&dir, &[".nuxt/*.d.ts".into()]);
        assert_eq!(found, [".nuxt/imports.d.ts"]);
        assert!(unmatched.is_empty());
    }

    #[test]
    fn missing_declaration_file_is_a_notice() {
        // Review Focus 1
        let dir = tmp_with(&[("src/a.ts", "export const a = 1;\n")]);
        let (ai, errs) = load(&dir, &[".nuxt/imports.d.ts".into()], &workspace_of(&dir));
        assert!(ai.get("a").is_none());
        assert_eq!(errs.len(), 1);
    }
}
