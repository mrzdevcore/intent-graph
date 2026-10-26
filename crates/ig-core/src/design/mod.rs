//! design.json (v1). The pure model and validation live in `ig-design`; the translation to a
//! plan needs ig-core's `Plan` and stays here.

mod plan;

pub use ig_design::*;
pub use plan::{Planned, extras, gaps, graph_id, to_plan};

#[cfg(test)]
mod tests;
