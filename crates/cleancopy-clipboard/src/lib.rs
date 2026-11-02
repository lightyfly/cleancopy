//! Native clipboard access. Linux writers must remain alive to serve X11 requests.
use std::{error::Error, fmt};

pub mod cf_html;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Payload {
    pub html: String,
    pub plain: String,
    pub markdown: String,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum WriteMode {
    #[default]
    Multi,
    Plain,
    Html,
    Markdown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardFormat {
    pub name: String,
    /// Native data size in bytes, not characters.
    pub bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardError(pub String);
impl fmt::Display for ClipboardError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl Error for ClipboardError {}
pub type Result<T> = std::result::Result<T, ClipboardError>;

pub(crate) fn error(e: impl fmt::Display) -> ClipboardError {
    ClipboardError(e.to_string())
}

pub trait ClipboardWriter {
    fn write(&self, payload: &Payload) -> Result<()>;
    fn read_text(&self) -> Result<String>;
}

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::{
    run_holder, spawn_holder, NativeClipboard, NativeSnapshot, NativeTarget, TransferProgress,
};
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::NativeClipboard;
#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub use windows::NativeClipboard;

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
#[derive(Debug, Default)]
pub struct NativeClipboard;
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
impl NativeClipboard {
    pub fn new() -> Result<Self> {
        Err(error(
            "clipboard backend is not implemented on this platform",
        ))
    }
    pub fn write_mode(&self, _: &Payload, _: WriteMode) -> Result<()> {
        Err(error(
            "clipboard backend is not implemented on this platform",
        ))
    }
    pub fn list_formats(&self) -> Result<Vec<ClipboardFormat>> {
        Err(error(
            "clipboard backend is not implemented on this platform",
        ))
    }
}

impl ClipboardWriter for NativeClipboard {
    fn write(&self, payload: &Payload) -> Result<()> {
        self.write_mode(payload, WriteMode::Multi)
    }
    fn read_text(&self) -> Result<String> {
        self.read_native_text()
    }
}
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
impl NativeClipboard {
    fn read_native_text(&self) -> Result<String> {
        Err(error(
            "clipboard backend is not implemented on this platform",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn payload_and_mode_defaults() {
        assert_eq!(Payload::default().plain, "");
        assert_eq!(WriteMode::default(), WriteMode::Multi);
        assert_eq!(error("native error").to_string(), "native error");
    }
}
