//! Agent sessions. Task 6 fills in `start_session`; every entry point goes through the trust gate.

// ponytail: unused until Task 6 wires the chat; drop the allow then.
#![allow(dead_code)]

use crate::trust;
use std::path::Path;

#[derive(Debug, PartialEq)]
pub enum AgentError {
    NotTrusted,
    NotImplemented,
}

pub fn start_session(root: &Path, data_dir: &Path) -> Result<(), AgentError> {
    if !trust::is_trusted(data_dir, root, &trust::inventory(root)) {
        return Err(AgentError::NotTrusted);
    }
    Err(AgentError::NotImplemented)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_untrusted_project_never_starts() {
        let base = std::env::temp_dir().join(format!("ig-agent-{}", std::process::id()));
        std::fs::create_dir_all(&base).unwrap();
        let data = base.join("data");
        assert_eq!(start_session(&base, &data), Err(AgentError::NotTrusted));
        trust::trust(&data, &base, &trust::inventory(&base)).unwrap();
        assert_eq!(start_session(&base, &data), Err(AgentError::NotImplemented));
    }
}
