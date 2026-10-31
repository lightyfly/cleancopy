use std::error::Error;
use std::fmt;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Payload {
    pub html: String,
    pub plain: String,
    pub markdown: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotImplemented;

impl fmt::Display for NotImplemented {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("clipboard support is not implemented yet")
    }
}

impl Error for NotImplemented {}

pub trait ClipboardWriter {
    fn write(&self, payload: &Payload) -> Result<(), NotImplemented>;
    fn read_text(&self) -> Result<String, NotImplemented>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct StubClipboard;

impl ClipboardWriter for StubClipboard {
    fn write(&self, _payload: &Payload) -> Result<(), NotImplemented> {
        Err(NotImplemented)
    }

    fn read_text(&self) -> Result<String, NotImplemented> {
        Err(NotImplemented)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stub_operations_explicitly_fail() {
        let clipboard = StubClipboard;
        assert_eq!(clipboard.write(&Payload::default()), Err(NotImplemented));
        assert_eq!(clipboard.read_text(), Err(NotImplemented));
        assert!(NotImplemented.to_string().contains("not implemented"));
    }
}
