//! Reading `ig.yaml`.

use crate::Error;
use ig_core::config::Config;
use ig_core::ports::Files;

/// `ig.yaml` parsed; the default config when there is none. An unreadable file is an error.
pub fn load(files: &dyn Files) -> crate::Result<Config> {
    match files.read_bytes("ig.yaml") {
        None if files.exists("ig.yaml") => Err(Error::ConfigUnreadable("cannot be read".into())),
        None => Ok(Config::default()),
        Some(b) => match String::from_utf8(b) {
            Ok(text) => Ok(Config::parse(&text)?),
            Err(_) => Err(Error::ConfigUnreadable(
                "stream did not contain valid UTF-8".into(),
            )),
        },
    }
}
