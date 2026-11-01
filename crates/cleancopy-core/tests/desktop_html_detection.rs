use cleancopy_core::{is_markdown, render_html, render_word_html};

#[test]
fn word_headings_have_semantics_builtin_styles_and_outline_levels() {
    let input = (1..=6)
        .map(|level| format!("{} Heading {level}\n\n", "#".repeat(level)))
        .collect::<String>();
    let html = render_word_html(&input);
    assert!(html.starts_with("<html><head>\n<style>\n"));
    assert!(html.ends_with("</body></html>\n"));
    assert!(html.contains("</style>\n</head>\n<body>"));
    for level in 1..=6 {
        assert!(html.contains(&format!("<h{level} class=\"MsoHeading{level}\"")));
        assert!(html.contains(&format!(
            "style=\"mso-style-name:'Heading {level}';mso-outline-level:{level};"
        )));
        assert!(html.contains(&format!(">Heading {level}</h{level}>")));
        assert!(html.contains(&format!("h{level}, .MsoHeading{level} {{mso-style-name:\"Heading {level}\";mso-outline-level:{level};")));
    }
}

#[test]
fn setext_headings_and_heading_inline_formatting_survive() {
    let html = render_word_html("**Title** & `code`\n===\n\n*Subtitle*\n---");
    assert!(html.contains("<h1 class=\"MsoHeading1\""));
    assert!(html.contains("<strong>Title</strong> &amp; <code style="));
    assert!(html.contains("<h2 class=\"MsoHeading2\""));
    assert!(html.contains("<em>Subtitle</em></h2>"));
}

#[test]
fn word_rendering_preserves_base_body_for_non_heading_content() {
    let input = "**bold** *italic* ~~removed~~\n\n7. seven\n8. eight\n\n- [ ] todo\n- [x] done\n\n> quoted\n\n| a | b |\n| :--- | ---: |\n| x | y |\n\n`inline`\n\n```rust\nlet x = 1;\n```\n\n[link](https://example.com)";
    let original = render_html(input);
    let word = render_word_html(input);
    assert_eq!(
        original.split_once("<body>").unwrap().1,
        word.split_once("<body>").unwrap().1
    );
    assert!(word.contains("font-family:monospace;background-color:#f5f5f5"));
    assert!(word.contains("<ol start=\"7\">"));
    assert!(word.contains("<li>☑ done</li>"));
    assert!(word.contains("<table border=\"1\""));
}

#[test]
fn word_html_keeps_untrusted_content_inert() {
    let html = render_word_html(
        "# <script>alert(1)</script>\n\n<h2 class=\"evil\">fake</h2>\n\n<style>h1{display:none}</style>\n\n`<h1>literal</h1>`\n\n[bad](javascript:evil) ![bad](data:text/html,evil)",
    );
    assert!(!html.contains("<script"));
    assert!(!html.contains("class=\"evil\""));
    assert!(!html.contains("<style>h1"));
    assert!(html.contains("&lt;h2 class=&quot;evil&quot;&gt;"));
    assert!(html.contains("&lt;h1&gt;literal&lt;/h1&gt;"));
    assert!(html.contains("href=\"\""));
    assert!(html.contains("src=\"\""));
    assert_eq!(html.matches("class=\"MsoHeading").count(), 1);
}

#[test]
fn word_html_handles_empty_unicode_and_crlf() {
    let empty = render_word_html("");
    assert!(empty.contains("<body>\n</body>"));
    let html = render_word_html("# 中文 & café\r\n\r\n正文\r\n");
    assert!(html.contains(">中文 &amp; café</h1>"));
    assert!(!html.contains('\r'));
    assert!(!html.contains("<!--StartFragment-->"));
}

#[test]
fn detection_recognizes_all_heading_levels_and_nested_constructs() {
    for level in 1..=6 {
        assert!(is_markdown(&format!("{} heading", "#".repeat(level))));
    }
    for input in [
        "plain\n\n> - nested **bold**",
        "1. first\n   - nested",
        "before\n\n~~~\ncode\n~~~",
        "[text][target]\n\n[target]: https://example.com",
        "https://example.com and *formatted*",
    ] {
        assert!(is_markdown(input), "missed {input:?}");
    }
}

#[test]
fn detection_does_not_confuse_plain_syntax_or_bare_links_with_markdown() {
    for input in [
        "https://example.com/a_b?q=x&n=2",
        "See www.example.com and mail test@example.com.",
        "a_b_c and file_name.txt",
        "C++ uses * pointers and 2 ** 3 is an expression",
        r"\# heading",
        r"\*\*bold\*\*",
        r"\`code\`",
        r"\> quote",
        "[link](https://example.com",
        "| a | b |\n| c | d |",
    ] {
        assert!(!is_markdown(input), "false positive: {input:?}");
    }
    // A marker-only ATX heading is still a valid (empty) heading.
    assert!(is_markdown("###"));
}
