//! Platform-independent Markdown parsing and renderers.

pub mod cancel;
pub mod detect;
pub mod parse;
pub mod render_numbered;
pub mod render_rtf;
pub mod render_word_html;

pub use detect::is_markdown;
pub use render_numbered::{render_numbered, NumberingStyle};
pub use render_rtf::render_rtf;
pub use render_word_html::render_word_html;
pub mod render_html;
pub mod render_md;
pub mod render_plain;
pub mod width;

pub use render_html::render_html;
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
