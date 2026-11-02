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
    // A single leading H1 is the document title: Word's built-in "Title"
    // style instead of Heading 1 (design.md §7.1).
    let body = html
        .split_once("<body>")
        .map_or("", |(_, b)| b)
        .trim_start();
    if body.starts_with("<h1 ") && html.matches("<h1 ").count() == 1 {
        styles.push_str(
            "p.MsoTitle {mso-style-name:Title;mso-outline-level:body;font-size:26pt;font-weight:bold;}\n",
        );
        if let (Some(start), Some(end)) = (html.find("<h1 "), html.find("</h1>")) {
            let open_end = start + html[start..].find('>').unwrap_or(0) + 1;
            let inner = html[open_end..end].to_owned();
            html.replace_range(
                start..end + "</h1>".len(),
                &format!(
                    "<p class=\"MsoTitle\" style=\"mso-style-name:Title;font-size:26pt;font-weight:bold\"><b>{inner}</b></p>"
                ),
            );
        }
    }
    styles.push_str("</style>\n</head>\n");
    html.replacen("<html><body>", &format!("<html>{styles}<body>"), 1)
}
