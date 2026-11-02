use crate::parse;
use comrak::{nodes::NodeValue, Arena};

const CODE_STYLE: &str = "font-family:monospace;background-color:#f5f5f5";

/// Render Markdown as a complete, clipboard-ready HTML document with inline styles.
///
/// Raw HTML is escaped and dangerous link/image destinations are blocked by
/// comrak. Code is not syntax highlighted. Platform-specific clipboard headers
/// and fragment markers belong to the clipboard layer, not this renderer.
/// The output uses LF line endings on every platform.
pub fn render_html(input: &str) -> String {
    let arena = Arena::new();
    let root = parse::parse(&arena, input);
    // Language metadata is not needed without highlighting. Clearing it also
    // avoids comrak's special treatment of ```math and language CSS classes.
    for node in root.descendants() {
        if let NodeValue::CodeBlock(code) = &mut node.data.borrow_mut().value {
            code.info.clear();
        }
    }

    if crate::cancel::cancelled() {
        return String::new();
    }
    let mut options = parse::options();
    options.render.escape = true;
    let mut bytes = Vec::new();
    comrak::format_html(root, &options, &mut bytes).expect("writing HTML to a Vec cannot fail");
    let mut html = String::from_utf8(bytes).expect("comrak emits UTF-8 HTML");

    // These exact tags/attributes are emitted by comrak with our fixed options.
    // Raw HTML and quotes in text/attributes are escaped before this pass, so
    // user content cannot introduce tags or attributes for us to rewrite.
    html = html
        .replace(
            "<pre>",
            &format!("<pre style=\"{CODE_STYLE};padding:12px\">"),
        )
        .replace("<code>", &format!("<code style=\"{CODE_STYLE}\">"))
        .replace(
            "<table>",
            "<table border=\"1\" style=\"border-collapse:collapse\">",
        )
        .replace("<th>", "<th style=\"padding:6px 12px\">")
        .replace("<th align=", "<th style=\"padding:6px 12px\" align=")
        .replace("<td>", "<td style=\"padding:6px 12px\">")
        .replace("<td align=", "<td style=\"padding:6px 12px\" align=")
        .replace(
            "<input type=\"checkbox\" checked=\"\" disabled=\"\" />",
            "☑",
        )
        .replace("<input type=\"checkbox\" disabled=\"\" />", "☐");
    for class in ["footnotes", "footnote-ref", "footnote-backref"] {
        html = html.replace(&format!(" class=\"{class}\""), "");
    }
    format!("<html><body>\n{html}</body></html>\n")
}
