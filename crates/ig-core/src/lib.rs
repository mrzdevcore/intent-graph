//! The pure domain: graph model, config parsing, rules, HTTP route matching, and the ports
//! adapters implement. No I/O.

pub mod cache;
pub mod config;
pub mod http;
pub mod model;
pub mod ports;
pub mod rules;
#[cfg(any(test, feature = "testing"))]
pub mod testing;
pub mod workspace;
