//! Layer assignment from `ig.yaml` `layers`.

use crate::config::Config;
use crate::workspace::{self, Package};
use globset::GlobMatcher;

/// Layer assignment: the most specific entry wins (longest literal prefix), ties go to the first declared.
pub struct Layers<'c> {
    entries: Vec<(&'c str, GlobMatcher, usize)>,
}

fn literal_prefix(g: &str) -> usize {
    g.find(['*', '?', '[', '{']).unwrap_or(g.len())
}

impl<'c> Layers<'c> {
    pub fn new(config: &'c Config, packages: &[Package]) -> Layers<'c> {
        let mut entries = vec![];
        for (name, globs) in &config.layers {
            for g in globs {
                let pattern = match packages.iter().find(|p| &p.name == g) {
                    Some(p) if p.dir == "." => "**".to_string(),
                    Some(p) => format!("{}/**", p.dir),
                    None => g.trim_start_matches("./").to_string(),
                };
                if let Ok(m) = workspace::glob(&pattern) {
                    entries.push((name.as_str(), m.compile_matcher(), literal_prefix(&pattern)));
                }
            }
        }
        Layers { entries }
    }

    pub fn of(&self, file: &str) -> Option<&'c str> {
        let mut best: Option<(&str, usize)> = None;
        for (name, m, spec) in &self.entries {
            if m.is_match(file) && best.is_none_or(|(_, s)| *spec > s) {
                best = Some((name, *spec));
            }
        }
        best.map(|(n, _)| n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layers_most_specific_wins() {
        let c = Config::parse(
            "layers:\n  a: ['packages/**']\n  b: ['packages/core/**']\n  c: ['packages/core/**']\n",
        )
        .unwrap();
        let l = Layers::new(&c, &[]);
        assert_eq!(l.of("packages/core/x.ts"), Some("b"));
        assert_eq!(l.of("packages/ui/x.ts"), Some("a"));
        assert_eq!(l.of("apps/x.ts"), None);
    }
}
