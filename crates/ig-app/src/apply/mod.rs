//! `ig apply`: apply state (`.ig/apply.json`), tamper hashes, exceptions, the reviewer's
//! findings and the final check. Plan conformance is in `conformance`.

mod conformance;

pub use conformance::{change_hash, findings};

use crate::check::{self, Opts};
use crate::{Env, Error, bail, baseline, plan};
use anyhow::Context;
use ig_core::config::Config;
use ig_core::model::{Finding, sha1_bytes};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashSet};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplyState {
    pub plan: String,
    pub base: String,
    pub hashes: BTreeMap<String, String>,
    /// The baseline's fingerprints at start, so finish can list the exceptions added since.
    #[serde(default)]
    pub baseline: Vec<String>,
}

const STATE: &str = ".ig/apply.json";
const REVIEW: &str = ".ig/agent-review.json";

pub fn load(env: &Env) -> Option<ApplyState> {
    crate::read_json(env.files, STATE)
}

/// The apply in progress, when it is usable and no explicit `base` overrides it.
pub fn active(env: &Env, base: Option<&str>) -> Option<ApplyState> {
    load(env).filter(|st| base.is_none() && stale(env, st).is_none())
}

/// Why `st` can't be used (its plan or base commit is gone), or None when it is usable.
pub fn stale(env: &Env, st: &ApplyState) -> Option<String> {
    if !env.files.exists(&st.plan) {
        Some(format!("plan {} is missing", st.plan))
    } else if env.vcs.and_then(|v| v.verify(&st.base)).is_none() {
        Some(format!("base commit {} is missing", st.base))
    } else {
        None
    }
}

fn save(env: &Env, st: &ApplyState) -> crate::Result<()> {
    env.files
        .write_atomic(STATE, &serde_json::to_vec_pretty(st)?)?;
    Ok(())
}

fn hash_file(env: &Env, rel: &str) -> String {
    env.files
        .read_bytes(rel)
        .map(|b| sha1_bytes(&b))
        .unwrap_or_else(|| "absent".into())
}

/// Guarded files: path → sha1 of the contents, or `"absent"`.
fn hashes(env: &Env, plan: &str, config: &Config) -> BTreeMap<String, String> {
    let globs: Vec<String> = config
        .checkers
        .iter()
        .flat_map(|c| c.configs.iter().cloned())
        .collect();
    [plan, "ig.yaml", "rules.md", baseline::PATH]
        .into_iter()
        .map(String::from)
        .chain(env.files.glob(&globs).unwrap_or_default())
        .map(|p| {
            let h = hash_file(env, &p);
            (p, h)
        })
        .collect()
}

/// `ig apply start` for the plan at `rel` (root-relative) with `text`.
pub fn start(env: &Env, rel: &str, text: &str) -> crate::Result<()> {
    plan::parse(rel, text)?;
    let Some(base) = env.vcs.and_then(|r| r.head()) else {
        bail!("apply start: needs a git repository with at least one commit");
    };
    let hashes = hashes(env, rel, &crate::config::load(env.files)?);
    let baseline = baseline::load(env).into_iter().collect();
    save(
        env,
        &ApplyState {
            plan: rel.into(),
            base,
            hashes,
            baseline,
        },
    )
}

/// `ig apply abort`: drops the apply state; returns the plan it was for.
pub fn abort(env: &Env) -> crate::Result<String> {
    let Some(st) = load(env) else {
        return Err(Error::NoApplyInProgress);
    };
    env.files.remove(STATE)?;
    Ok(st.plan)
}

/// Exceptions (baseline entries with a reason) added since `ig apply start`: (fingerprint, reason).
pub fn exceptions_since(env: &Env, st: &ApplyState) -> Vec<(String, String)> {
    baseline::exceptions(&baseline::read(env).unwrap_or_default())
        .into_iter()
        .filter(|(fp, _)| !st.baseline.iter().any(|b| b == fp))
        .collect()
}

pub fn accept(env: &Env) -> crate::Result<()> {
    let Some(mut st) = load(env) else {
        return Err(Error::NoApplyInProgress);
    };
    st.hashes = hashes(env, &st.plan, &crate::config::load(env.files)?);
    save(env, &st)
}

/// `ig except`: records an exception with its reason in the baseline. During apply only the
/// baseline's hash is re-recorded, so other pending tamper stays visible. Refuses `ig/plan` and
/// `ig/tamper` findings of the last check: those are never baselined.
pub fn except(env: &Env, fingerprint: &str, reason: &str) -> crate::Result<()> {
    let last: Value = crate::read_json(env.files, ".ig/findings.json").unwrap_or_default();
    let rule = last["findings"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|f| f["fingerprint"].as_str() == Some(fingerprint))
        .and_then(|f| f["ruleId"].as_str());
    if let Some(rule @ ("ig/plan" | "ig/tamper")) = rule {
        bail!(
            "except: {fingerprint} is an {rule} finding; exceptions are for tool and review findings only. Update the plan (with the user's yes), then run `ig apply accept`"
        );
    }
    let mut items = baseline::read(env)?;
    items.retain(|v| v.as_str().or_else(|| v["fingerprint"].as_str()) != Some(fingerprint));
    items.push(json!({"fingerprint": fingerprint, "reason": reason}));
    env.files.write_atomic(
        baseline::PATH,
        (serde_json::to_string_pretty(&items)? + "\n").as_bytes(),
    )?;
    if let Some(mut st) = load(env) {
        st.hashes
            .insert(baseline::PATH.into(), hash_file(env, baseline::PATH));
        save(env, &st)?;
    }
    Ok(())
}

/// `ig apply review`: stores the reviewer agent's JSON array (prose around it is ignored) for
/// the current change; returns the finding count.
pub fn review_ingest(env: &Env, json: &str) -> crate::Result<usize> {
    let (Some(a), Some(b)) = (json.find('['), json.rfind(']')) else {
        bail!("apply review: no JSON array in the input");
    };
    let items: Vec<Value> = serde_json::from_str(&json[a..=b.max(a)])
        .context("apply review: the reviewer output is not a JSON array")?;
    for (i, it) in items.iter().enumerate() {
        if !it["rule"].is_string() || !it["message"].is_string() {
            bail!("apply review: item {i} needs a string `rule` and `message`: {it}");
        }
    }
    let run = check::run(env, &Opts::default())?;
    env.files.write_atomic(
        REVIEW,
        &serde_json::to_vec_pretty(&json!({"change": change_hash(&run), "findings": items}))?,
    )?;
    Ok(items.len())
}

/// The final check: 0 when nothing blocks (apply is then cleared and the plan marked done).
/// What it reports goes to `out`, also when it fails part way.
pub fn finish(env: &Env, out: &mut String) -> crate::Result<u8> {
    let Some(st) = load(env) else {
        return Err(Error::NoApplyInProgress);
    };
    if let Some(r) = stale(env, &st) {
        return Err(Error::StaleApply(r));
    }
    let mut run = check::run(env, &Opts::default())?;
    let seen: HashSet<String> = run
        .result
        .findings
        .iter()
        .map(|f| f.fingerprint.clone())
        .collect();
    let extra: Vec<Finding> = findings(env, &st, &run, true)
        .into_iter()
        .filter(|f| !seen.contains(&f.fingerprint))
        .collect();
    run.result.findings.extend(extra);
    if conformance::stored_review(env, &run).is_none() {
        run.result.findings.push(
            Finding::new(
                "ig/review-missing",
                ig_core::config::Severity::Error,
                "ig",
                REVIEW,
                0,
                "the reviewer agent has not reviewed this change; run it, then ig apply finish"
                    .into(),
            )
            .fingerprinted(),
        );
    }
    let block_on = run.config.block_on;
    run.result.blocking = run.result.findings.iter().any(|f| f.severity >= block_on);
    // The last check result, so `ig except` sees the final ig/plan findings too.
    check::write_state(env, &run.result)?;
    let excepted = exceptions_since(env, &st);
    if !excepted.is_empty() {
        out.push_str("exceptions added during this apply:\n");
        for (fp, reason) in &excepted {
            out.push_str(&format!("- {fp}: {reason}\n"));
        }
    }
    if run.result.blocking {
        out.push_str(&check::agent_text(&run.result, block_on));
        return Ok(1);
    }
    let Some(text) = env
        .files
        .read_bytes(&st.plan)
        .and_then(|b| String::from_utf8(b).ok())
    else {
        bail!("{}: cannot be read", st.plan);
    };
    if !plan::parse(&st.plan, &text)?.done {
        let mut lines: Vec<&str> = text.lines().collect();
        let at = lines
            .iter()
            .position(|l| l.starts_with("# "))
            .map_or(0, |i| i + 1);
        lines.insert(at, "Status: done");
        env.files
            .write_atomic(&st.plan, (lines.join("\n") + "\n").as_bytes())?;
    }
    env.files.remove(STATE)?;
    out.push_str("apply finished\n");
    Ok(0)
}

#[cfg(test)]
mod tests;
