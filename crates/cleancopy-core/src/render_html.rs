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
    render_html_with(input, false)
}

/// `word_lists` rewrites lists for Word and WPS: no `<p>` directly inside
/// `<li>` (extra paragraphs become `<br>`), nested lists stay inside their
/// parent `<li>`, empty items are dropped, and every list has an explicit
/// `list-style-type` so nested bullets are never renumbered.
pub(crate) fn render_html_with(input: &str, word_lists: bool) -> String {
    let mut options = parse::options();
    options.render.escape = true;
    let mut bytes = Vec::new();
    // Top-level blocks render independently, so chunks concatenate to the
    // same HTML while only one chunk's AST is alive (parse::chunks).
    for chunk in parse::chunks(input) {
        let arena = Arena::new();
        let root = parse::parse(&arena, chunk);

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
        comrak::format_html(root, &options, &mut bytes).expect("writing HTML to a Vec cannot fail");
    }
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
    if word_lists {
        html = word_compatible_lists(&html);
    }
    format!("<html><body>\n{html}</body></html>\n")
}

/// Rewrite comrak's list markup for Word/WPS. Comrak escapes raw HTML, so
/// every `<` in its output starts one of its own tags.
fn word_compatible_lists(html: &str) -> String {
    #[derive(PartialEq)]
    enum Open {
        Li { paragraphs: usize },
        List,
        Other,
    }
    let mut out = String::with_capacity(html.len());
    let mut stack: Vec<Open> = Vec::new();
    let mut rest = html;
    while let Some(start) = rest.find('<') {
        out.push_str(&rest[..start]);
        let end = rest[start..]
            .find('>')
            .map_or(rest.len(), |e| start + e + 1);
        let tag = &rest[start..end];
        rest = &rest[end..];
        let name: String = tag
            .trim_start_matches(['<', '/'])
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect();
        let closing = tag.starts_with("</");
        let self_closing =
            tag.ends_with("/>") || matches!(name.as_str(), "br" | "hr" | "img" | "input");
        let direct_in_li = matches!(stack.last(), Some(Open::Li { .. }));
        match (name.as_str(), closing) {
            ("p", false) if direct_in_li => {
                if let Some(Open::Li { paragraphs }) = stack.last_mut() {
                    if *paragraphs > 0 {
                        out.push_str("<br>");
                    }
                    *paragraphs += 1;
                }
                // Swallow the newline comrak emits after the tag, if any.
                continue;
            }
            ("p", true) if direct_in_li => {
                rest = rest.strip_prefix('\n').unwrap_or(rest);
                continue;
            }
            ("li", false) => {
                stack.push(Open::Li { paragraphs: 0 });
                out.push_str(tag);
                if rest.starts_with("\n<p>") {
                    rest = &rest[1..];
                }
            }
            ("ul", false) => {
                // Real list markers (report v6 item 5): disc / circle /
                // square by nesting depth, never glyphs inlined in the text.
                let depth = stack.iter().filter(|o| matches!(o, Open::List)).count();
                stack.push(Open::List);
                let style = BULLET_STYLES[depth % BULLET_STYLES.len()];
                out.push_str(&tag.replacen(
                    "<ul",
                    &format!("<ul style=\"list-style-type:{style}\""),
                    1,
                ));
            }
            ("ol", false) => {
                stack.push(Open::List);
                out.push_str(&tag.replacen("<ol", "<ol style=\"list-style-type:decimal\"", 1));
            }
            (_, true) => {
                stack.pop();
                out.push_str(tag);
            }
            _ => {
                if !self_closing {
                    stack.push(Open::Other);
                }
                out.push_str(tag);
            }
        }
    }
    out.push_str(rest);
    // Drop empty items (WPS renders them as bare numbers).
    loop {
        let before = out.len();
        out = out
            .replace("<li></li>\n", "")
            .replace("<li>\n</li>\n", "")
            .replace("<li></li>", "");
        if out.len() == before {
            break;
        }
    }
    out
}

/// Bullet marker styles by nesting depth (all lists count toward depth).
const BULLET_STYLES: [&str; 3] = ["disc", "circle", "square"];
