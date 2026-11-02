//! Word-compatible heading metadata on top of the safe HTML renderer.

/// Render Markdown as safe HTML with Word's built-in heading style metadata.
///
/// Headings retain their semantic `h1` through `h6` tags. Both the stylesheet
/// and inline styles identify their Word heading style and outline level so
/// clipboard consumers can preserve document structure as well as appearance.
/// All content rendering and sanitization is delegated to [`crate::render_html`].
pub fn render_word_html(input: &str) -> String {
    let mut html = crate::render_html::render_html_with(input, true);
    let mut styles = String::from("<head>\n<style>\n");
    for (level, size) in [(1, 24), (2, 18), (3, 14), (4, 12), (5, 10), (6, 8)] {
        // Word uses one-based outline levels in its HTML dialect (unlike the
        // zero-based outlineLvl values in OOXML).
        styles.push_str(&format!(
            "h{level}, .MsoHeading{level} {{mso-style-name:\"Heading {level}\";mso-outline-level:{level};font-size:{size}pt;font-weight:bold;}}\n"
        ));
        // The base renderer emits these exact tags. User-provided HTML is
        // already escaped, so neither text nor attributes can be rewritten.
        html = html.replace(
            &format!("<h{level}>"),
            &format!(
                "<h{level} class=\"MsoHeading{level}\" style=\"mso-style-name:'Heading {level}';mso-outline-level:{level};font-size:{size}pt;font-weight:bold\">"
            ),
        );
    }
    styles.push_str("</style>\n</head>\n");
    html.replacen("<html><body>", &format!("<html>{styles}<body>"), 1)
}
