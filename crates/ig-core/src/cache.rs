//! The base-graph cache key.

use crate::model::{TOOL_VERSION, sha1_hex};

/// Bump whenever extraction or resolution changes output, so cached base graphs are rebuilt.
pub const GRAPH_REVISION: u32 = 14;

/// Config, tool and revision, plus the `autoImports` declaration files as (path, contents) (base
/// graphs read the working tree's copy when the commit lacks them, e.g. a gitignored `.nuxt/`).
pub fn cache_key(config_key_input: &str, declarations: &[(String, String)]) -> String {
    let decls: String = declarations
        .iter()
        .map(|(f, text)| format!("|{f}:{text}"))
        .collect();
    sha1_hex(&format!(
        "{TOOL_VERSION}|{GRAPH_REVISION}|{config_key_input}{decls}"
    ))[..12]
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_is_pinned() {
        let config = crate::config::Config {
            auto_imports: vec![".nuxt/*.d.ts".into()],
            ..Default::default()
        };
        let decls = [(".nuxt/imports.d.ts".to_string(), "export {}\n".to_string())];
        let key = cache_key(&config.graph_key_input(), &decls);
        // The input format and revision 14 are a contract with users' `.ig/cache`.
        let input = format!(
            "{TOOL_VERSION}|14|{}|.nuxt/imports.d.ts:export {{}}\n",
            config.graph_key_input()
        );
        assert_eq!(key, sha1_hex(&input)[..12]);
        // What the binary's `git::cache_key` gave for this tree before the move.
        if TOOL_VERSION == "0.1.0" {
            assert_eq!(key, "bf0cd3cedcd7");
        }
    }
}
