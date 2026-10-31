//! Platform-independent Markdown parsing and W1 renderers.

pub mod parse;
pub mod render_md;
pub mod render_plain;
pub mod width;

pub use render_md::render_markdown;
pub use render_plain::render_plain;

pub(crate) fn native_newlines(text: &str) -> String {
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    if cfg!(windows) {
        text.replace('\n', "\r\n")
    } else {
        text
    }
}
