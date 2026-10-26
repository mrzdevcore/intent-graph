//! The error use cases return. `Display` is the message ig shows; hosts match on the variants.

use ig_core::config::ConfigError;
use ig_core::plan::PlanError;
use ig_core::ports::PortError;
use std::path::PathBuf;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// `ig.yaml` exists but cannot be read or is not UTF-8 (a bad one is `Other`).
    #[error("ig.yaml: {0}")]
    ConfigUnreadable(String),
    /// `.ig/baseline.json` cannot be read, is not UTF-8 or does not parse; `path` is absolute.
    #[error("{}: {reason}", .path.display())]
    BaselineUnreadable { path: PathBuf, reason: String },
    /// `--head` names a commit where the ig root does not exist. The graph build once failed on
    /// the missing directory there, so the message is still the system's "not found" text.
    #[error("{}", std::io::Error::from_raw_os_error(2))]
    HeadRootMissing,
    #[error("no apply in progress")]
    NoApplyInProgress,
    /// The apply's plan or base commit is gone; the reason says which.
    #[error("stale apply state ({0}): run ig apply abort")]
    StaleApply(String),
    /// Transparent, so `{e:#}` keeps showing the YAML cause.
    #[error(transparent)]
    PlanParse(#[from] PlanError),
    #[error(transparent)]
    Port(#[from] PortError),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl From<ConfigError> for Error {
    fn from(e: ConfigError) -> Error {
        Error::Other(e.into())
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Error {
        Error::Other(e.into())
    }
}

impl From<globset::Error> for Error {
    fn from(e: globset::Error) -> Error {
        Error::Other(e.into())
    }
}

/// `anyhow::bail!`, as an `Error::Other`.
macro_rules! bail {
    ($($t:tt)*) => {
        return Err($crate::Error::Other(::anyhow::anyhow!($($t)*)))
    };
}
pub(crate) use bail;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_is_the_old_message() {
        let cases = [
            (
                Error::ConfigUnreadable("cannot be read".into()),
                "ig.yaml: cannot be read",
            ),
            (
                Error::ConfigUnreadable("stream did not contain valid UTF-8".into()),
                "ig.yaml: stream did not contain valid UTF-8",
            ),
            (
                Error::BaselineUnreadable {
                    path: "/r/.ig/baseline.json".into(),
                    reason: "cannot be read".into(),
                },
                "/r/.ig/baseline.json: cannot be read",
            ),
            (
                Error::HeadRootMissing,
                "No such file or directory (os error 2)",
            ),
            (Error::NoApplyInProgress, "no apply in progress"),
            (
                Error::StaleApply("plan p.md is missing".into()),
                "stale apply state (plan p.md is missing): run ig apply abort",
            ),
            (
                Error::PlanParse(PlanError::NoBlock("p.md".into())),
                "p.md: no ```ig-plan block",
            ),
            (Error::Port(PortError::Vcs("bad rev".into())), "bad rev"),
            (Error::Other(anyhow::anyhow!("boom")), "boom"),
        ];
        for (e, want) in cases {
            assert_eq!(e.to_string(), want);
        }
    }

    #[test]
    fn alternate_display_keeps_the_cause_chain() {
        let bad = || ig_core::plan::parse("p.md", "```ig-plan\nnodes: [\n```\n").unwrap_err();
        let want = format!("{:#}", anyhow::Error::from(bad()));
        assert!(want.starts_with("p.md: ig-plan block: "), "{want}");
        let e = Error::PlanParse(bad());
        assert_eq!(format!("{:#}", anyhow::Error::from(e)), want);
        let ctx = anyhow::Error::from(PortError::Io("disk".into())).context("outer");
        assert_eq!(
            format!("{:#}", anyhow::Error::from(Error::Other(ctx))),
            "outer: disk"
        );
    }
}
