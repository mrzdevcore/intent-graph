//! The board engine core: model, gestures, undo with agent steps, merge, hit-testing and the
//! scene. Pure and clock-free (time is the host's), so it builds for wasm32-unknown-unknown.

mod camera;
mod delta;
mod engine;
mod history;
mod hit;
mod input;
mod merge;
mod scene;
mod snap;
#[cfg(target_arch = "wasm32")]
mod web;

pub use camera::Camera;
pub use delta::{delta, meaning};
pub use engine::{EditRequest, Engine, Out};
pub use history::{History, Step};
pub use input::{Input, Tool, mods};
pub use merge::{Merged, merge, place_new};
pub use scene::{A11yItem, Label, Line, Paint, Rect, Scene};
pub use snap::{GRID, default_link_kind, move_ops, row_at, snap};

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RowKind {
    Layer,
    Other,
    External,
}

/// A drawn row of the architecture: its band spans `top..=bottom` (viewer `Lane`).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Lane {
    pub row: u32,
    pub name: String,
    pub kind: RowKind,
    pub top: f32,
    pub bottom: f32,
}

/// A faded box of the existing code: selectable, markable and linkable, never moved.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct CtxBox {
    /// The graph id (a file, `file#name`, `system:<name>`).
    pub id: String,
    pub label: String,
    /// The graph kind (`module`, `route`, `system`, `page`, …): picks the default link kind.
    pub kind: String,
    #[serde(default)]
    pub layer: Option<String>,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// What the board draws around the design, built by the viewer's `context.ts`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Context {
    pub lanes: Vec<Lane>,
    pub boxes: Vec<CtxBox>,
    /// `(from, to, kind)` between context boxes.
    pub edges: Vec<(String, String, String)>,
}

/// A check finding's severity (mirrors `ig_core::config::Severity`).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Note,
    Warning,
    Error,
}
