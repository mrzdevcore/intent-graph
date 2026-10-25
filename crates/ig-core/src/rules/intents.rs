//! `ig/intent-*`: unknown, unimplemented and untested intents.

use crate::config::Severity;
use crate::intents::Intent;
use crate::model::{Finding, Graph};
use std::collections::{BTreeSet, HashSet};

/// `scope`: the intents in scope (changed in intents.md, or all with `--all`).
pub fn intents(
    head: &Graph,
    declared: &[Intent],
    scope: &BTreeSet<String>,
    tests: &globset::GlobSet,
) -> Vec<Finding> {
    let mut implemented: HashSet<&str> = HashSet::new();
    let mut tested: HashSet<&str> = HashSet::new();
    let ids: HashSet<&str> = declared.iter().map(|i| i.id.as_str()).collect();
    let mut out = vec![];
    for n in &head.nodes {
        let in_test = n.kind == "test" || tests.is_match(&n.file);
        for t in &n.intents {
            if in_test {
                tested.insert(t);
            } else {
                implemented.insert(t);
            }
            if !ids.contains(t.as_str()) {
                out.push(
                    Finding::new(
                        "ig/intent-unknown",
                        Severity::Error,
                        "ig",
                        &n.file,
                        n.range.start_line,
                        format!("@intent {t} is not declared in intents.md"),
                    )
                    .node(&n.id)
                    .fix(format!(
                        "Add `- {t}: <one line>` to intents.md, or fix the tag."
                    )),
                );
            }
        }
    }
    for i in declared.iter().filter(|i| scope.contains(&i.id)) {
        if !implemented.contains(i.id.as_str()) {
            out.push(
                Finding::new(
                    "ig/intent-unimplemented",
                    Severity::Error,
                    "ig",
                    "intents.md",
                    i.line,
                    format!("intent {} has no implementation: {}", i.id, i.text),
                )
                .fix(format!(
                    "Tag the implementing code with `// @intent {}`.",
                    i.id
                )),
            );
        }
        if !tested.contains(i.id.as_str()) {
            out.push(
                Finding::new(
                    "ig/intent-untested",
                    Severity::Warning,
                    "ig",
                    "intents.md",
                    i.line,
                    format!("intent {} has no test: {}", i.id, i.text),
                )
                .fix(format!("Tag a test with `// @intent {}`.", i.id)),
            );
        }
    }
    out
}
