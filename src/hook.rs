//! `ig hook claude`: the Claude Code Stop hook. Never fails; errors become a systemMessage.

use crate::check;
use serde_json::{Value, json};
use std::io::Read;
use std::path::Path;

pub fn claude(root: &Path) {
    let out = match run(root) {
        Ok(v) => v,
        Err(e) => json!({ "systemMessage": format!("ig hook error: {e:#}") }),
    };
    println!("{out}");
}

fn session_file(root: &Path, session: &str) -> std::path::PathBuf {
    let safe: String = session
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    root.join(".ig/hook").join(if safe.is_empty() {
        "default".into()
    } else {
        safe
    })
}

fn run(root: &Path) -> anyhow::Result<Value> {
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input)?;
    let v: Value = if input.trim().is_empty() {
        json!({})
    } else {
        serde_json::from_str(&input)?
    };
    let session = v
        .get("session_id")
        .and_then(Value::as_str)
        .unwrap_or("default");
    let file = session_file(root, session);
    let run = check::run(root, &check::Opts::default())?;
    if !run.result.blocking {
        let _ = std::fs::remove_file(&file);
        return Ok(json!({}));
    }
    decide(
        &file,
        run.config.hook_max_rounds,
        check::agent_text(&run.result, run.config.block_on),
        crate::apply::load(root).is_some_and(|st| crate::apply::stale(root, &st).is_none()),
    )
}

/// Blocks up to `max` rounds, then hands off. While `asking` (apply in progress) it blocks once
/// more telling the agent to ask the user, then lets the turn end.
fn decide(file: &Path, max: u32, reason: String, asking: bool) -> anyhow::Result<Value> {
    let rounds: u32 = std::fs::read_to_string(file)
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0);
    if asking && rounds > max {
        let _ = std::fs::remove_file(file);
        return Ok(json!({}));
    }
    let reason = if asking && rounds == max {
        format!(
            "{reason}\n\nYou have tried ig's findings {max} times. Stop fixing. Ask the user what to do, showing each remaining finding and the options: another approach, update the plan, accept an exception (ig except), or change the rule."
        )
    } else {
        reason
    };
    if !asking && rounds >= max {
        let _ = std::fs::remove_file(file);
        return Ok(json!({
            "systemMessage": format!("ig check still blocks after {max} rounds; handing off to a human. Run `ig check` to see the findings.")
        }));
    }
    std::fs::create_dir_all(file.parent().unwrap())?;
    std::fs::write(file, (rounds + 1).to_string())?;
    Ok(json!({ "decision": "block", "reason": reason }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounds_cap_then_hand_off() {
        let dir = std::env::temp_dir().join(format!("ig-hook-test-{}", std::process::id()));
        let f = session_file(&dir, "s/1");
        assert!(f.ends_with("s_1"));
        for _ in 0..3 {
            assert_eq!(
                decide(&f, 3, "r".into(), false).unwrap()["decision"],
                "block"
            );
        }
        let v = decide(&f, 3, "r".into(), false).unwrap();
        assert!(v["systemMessage"].as_str().unwrap().contains("human"));
        assert!(!f.exists());
        // While applying: one more block that asks the user, then the turn ends.
        for _ in 0..2 {
            assert_eq!(
                decide(&f, 2, "r".into(), true).unwrap()["decision"],
                "block"
            );
        }
        let v = decide(&f, 2, "r".into(), true).unwrap();
        assert_eq!(v["decision"], "block");
        assert!(v["reason"].as_str().unwrap().contains("Ask the user"));
        assert_eq!(decide(&f, 2, "r".into(), true).unwrap(), json!({}));
        assert!(!f.exists());
        let _ = std::fs::remove_dir_all(dir);
    }
}
