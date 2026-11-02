//! Desktop configuration, event-driven paste engine, and stage B UI previews.

pub mod config;
pub mod engine;
pub mod html_source;
#[cfg(target_os = "linux")]
pub mod linux;
pub mod secure_input;
pub mod ui;
