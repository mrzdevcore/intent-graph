//! The pure domain: graph model, diffs, config parsing, notes and intents text, rules, review
//! metrics, plans, HTTP route matching. No I/O.

pub mod config;
pub mod diff;
pub mod http;
pub mod intents;
pub mod linediff;
pub mod model;
pub mod notes;
pub mod plan;
pub mod review;
pub mod rules;
pub mod workspace;
