//! The pure domain: graph model, diffs, config parsing, notes and intents text, rules, review
//! metrics, plans, HTTP route matching, and the ports adapters implement. No I/O.

pub mod cache;
pub mod config;
pub mod design;
pub mod diff;
pub mod http;
pub mod intents;
pub mod linediff;
pub mod model;
pub mod notes;
pub mod plan;
pub mod ports;
pub mod review;
pub mod rules;
#[cfg(any(test, feature = "testing"))]
pub mod testing;
pub mod workspace;
