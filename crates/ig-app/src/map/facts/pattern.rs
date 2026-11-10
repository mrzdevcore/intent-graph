//! A box's inside drawn as a pattern: onion rings, layered bands, a hexagonal core, or none.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase", try_from = "RawPattern")]
pub enum Pattern {
    Onion,
    Layered { bands: Vec<String> },
    Hexagonal,
    None,
}

/// `Pattern` as read: serde lets unknown keys through on an internally tagged unit variant even
/// with `deny_unknown_fields`, so it is read through this struct.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPattern {
    kind: String,
    bands: Option<Vec<String>>,
}

impl TryFrom<RawPattern> for Pattern {
    type Error = String;
    fn try_from(r: RawPattern) -> Result<Pattern, String> {
        match (r.kind.as_str(), r.bands) {
            ("layered", Some(bands)) => Ok(Pattern::Layered { bands }),
            ("onion", None) => Ok(Pattern::Onion),
            ("hexagonal", None) => Ok(Pattern::Hexagonal),
            ("none", None) => Ok(Pattern::None),
            (k, _) => Err(format!(
                "pattern: kind `{k}` with these keys is not one of onion, hexagonal, none, \
                 layered (with bands)"
            )),
        }
    }
}
