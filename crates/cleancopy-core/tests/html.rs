use cleancopy_core::render_html;

#[test]
fn complete_document_including_empty_input() {
    assert_eq!(render_html(""), "<html><body>\n</body></html>\n");
    let html = render_html("# 中文 & < >\r\n\r\n**bold** *italic* ~~gone~~\n\n---");
    assert!(html.contains("<h1>中文 &amp; &lt; &gt;</h1>"));
    assert!(html.contains("<strong>bold</strong> <em>italic</em> <del>gone</del>"));
    assert!(html.contains("<hr />"));
    assert!(!html.contains('\r'));
    assert!(!html.contains("<!--StartFragment-->"));
}

#[test]
fn escapes_raw_html_instead_of_passing_through_active_markup() {
    let html = render_html(
        "<script>alert('x')</script>\n\n<span class=\"bad\" style=\"color:red\" onclick=\"evil()\">text</span>\n\n<style>body{display:none}</style>\n\n<input type=\"checkbox\" disabled=\"\" />",
    );
    for tag in ["<script", "<span", "<style", "<input"] {
        assert!(!html.contains(tag), "unexpected active tag {tag}: {html}");
    }
    assert!(html.contains("&lt;script&gt;"));
    assert!(html.contains("&lt;span class=&quot;bad&quot;"));
    assert!(html.contains("&lt;input type=&quot;checkbox&quot;"));
    assert!(html.contains("text&lt;/span&gt;"));
}

#[test]
fn blocks_dangerous_link_and_image_schemes() {
    for url in [
        "javascript:alert(1)",
        "JaVaScRiPt:alert(1)",
        "vbscript:evil",
        "file:///etc/passwd",
        "data:text/html,evil",
        "jav&#x61;script:evil",
    ] {
        let html = render_html(&format!("[link](<{url}>) ![image](<{url}>)"));
        assert!(html.contains("href=\"\""), "unsafe link {url}: {html}");
        assert!(html.contains("src=\"\""), "unsafe image {url}: {html}");
    }
}

#[test]
fn escapes_text_code_titles_alt_and_urls() {
    let html = render_html(
        "Text &amp; & < >\n\n`<code>&\"`\n\n```html\n<script>&\"\n  indented\n```\n\n[link](https://example.com/?a=1&b=2 \"a \\\"quote\\\" <title>\")\n\n![a \\\"quote\\\" <alt>](https://example.com/image.png \"image \\\"title\\\"\")",
    );
    assert!(html.contains("Text &amp; &amp; &lt; &gt;"));
    assert!(html.contains("&lt;code&gt;&amp;&quot;"));
    assert!(html.contains("&lt;script&gt;&amp;&quot;\n  indented\n</code></pre>"));
    assert!(html.contains("href=\"https://example.com/?a=1&amp;b=2\""));
    assert!(html.contains("title=\"a &quot;quote&quot; &lt;title&gt;\""));
    assert!(html.contains("alt=\"a &quot;quote&quot; &lt;alt&gt;\""));
    assert!(html.contains("title=\"image &quot;title&quot;\""));
    assert!(!html.contains("<script>"));
}

#[test]
fn inline_styles_tables_tasks_and_unhighlighted_code() {
    let html = render_html(
        "7. seven\n8. eight\n\n- [ ] todo\n- [x] done\n\n| left | center | right |\n| :--- | :---: | ---: |\n| a | b | c |\n\n```rust\nlet x = 1;\n```\n\n```math\nx < y\n```\n\n`inline`\n\nnote[^n]\n\n[^n]: footnote",
    );
    assert!(html.contains("<ol start=\"7\">"));
    assert!(html.contains("<li>☐ todo</li>"));
    assert!(html.contains("<li>☑ done</li>"));
    assert!(html.contains("<table border=\"1\" style=\"border-collapse:collapse\">"));
    assert!(html.contains("<thead>"));
    assert!(html.contains("<th style=\"padding:6px 12px\" align=\"left\">"));
    assert!(html.contains("<td style=\"padding:6px 12px\" align=\"right\">"));
    assert!(html.contains("<pre style=\"font-family:monospace;background-color:#f5f5f5;padding:12px\"><code style=\"font-family:monospace;background-color:#f5f5f5\">let x = 1;\n</code></pre>"));
    assert!(html.contains(">x &lt; y\n</code></pre>"));
    assert!(html
        .contains("<code style=\"font-family:monospace;background-color:#f5f5f5\">inline</code>"));
    for forbidden in [" class=", "<style", "<input", "<span", "language-rust"] {
        assert!(!html.contains(forbidden), "unexpected {forbidden}: {html}");
    }
}
