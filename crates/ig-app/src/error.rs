//! The error use cases return. `Display` is the message ig shows; hosts match on the variants.

use ig_core::config::ConfigError;
use ig_core::ports::PortError;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// `ig.yaml` exists but cannot be read or is not UTF-8 (a bad one is `Other`).
    #[error("ig.yaml: {0}")]
    ConfigUnreadable(String),
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
            (Error::Port(PortError::Vcs("bad rev".into())), "bad rev"),
            (Error::Other(anyhow::anyhow!("boom")), "boom"),
        ];
        for (e, want) in cases {
            assert_eq!(e.to_string(), want);
        }
    }

    #[test]
    fn alternate_display_keeps_the_cause_chain() {
        let ctx = anyhow::Error::from(PortError::Io("disk".into())).context("outer");
        assert_eq!(
            format!("{:#}", anyhow::Error::from(Error::Other(ctx))),
            "outer: disk"
        );
    }
}
