//! Terminal UI modules for feed formatting and render management.
//!
//! The UI layer is split between data formatting helpers and the actual ratatui loop,
//! keeping the input/output display logic separate from the radio logic.

pub mod feed;
pub mod tui;

pub use tui::run_tui;