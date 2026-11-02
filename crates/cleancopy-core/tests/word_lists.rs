use cleancopy_core::render_word_html;

const LOOSE: &str = "1. **性能**：快\n\n   - 子项 A\n   - 子项 B\n\n2. 兼容性\n\n   第二段\n\n   1. 深一\n      - 更深\n3. \n";

fn body(html: &str) -> &str {
    &html[html.find("<body>").unwrap()..]
}

#[test]
fn loose_list_items_have_no_paragraphs_inside_li() {
    let html = render_word_html(LOOSE);
    let body = body(&html);
    assert!(
        !body.contains("<li>\n<p>") && !body.contains("<li><p>"),
        "{body}"
    );
    assert!(!body.contains("<p>"), "{body}");
    assert!(body.contains("<li><strong>性能</strong>：快<ul"), "{body}");
    assert!(body.contains("兼容性<br>第二段<ol"), "{body}");
}

#[test]
fn nested_lists_stay_inside_parent_li_with_explicit_types() {
    let html = render_word_html(LOOSE);
    let body = body(&html);
    let expected = "<ol style=\"list-style-type:decimal\">\n<li><strong>性能</strong>：快<ul style=\"list-style-type:none\">\n<li>◦&nbsp;子项 A</li>\n<li>◦&nbsp;子项 B</li>\n</ul>\n</li>";
    assert!(body.contains(expected), "{body}");
    // Bullets are bullets at every depth, never decimal.
    assert!(
        body.contains("<li>深一\n<ul style=\"list-style-type:none\">\n<li>▪&nbsp;更深"),
        "{body}"
    );
    assert_eq!(
        body.matches("<ul style=\"list-style-type:").count(),
        body.matches("<ul").count()
    );
    assert_eq!(
        body.matches("<ol style=\"list-style-type:decimal\"")
            .count(),
        body.matches("<ol").count()
    );
}

#[test]
fn empty_list_items_are_dropped() {
    let html = render_word_html(LOOSE);
    let body = body(&html);
    assert!(
        !body.contains("<li></li>") && !body.contains("<li>\n</li>"),
        "{body}"
    );
    assert_eq!(body.matches("<li>").count(), 6, "{body}");
}

#[test]
fn tight_lists_and_non_list_paragraphs_are_unchanged() {
    let html = render_word_html("para\n\n- a\n- b\n\nafter\n");
    assert!(
        html.contains("<ul style=\"list-style-type:none\">"),
        "{html}"
    );
    let body = body(&html);
    assert!(
        body.contains("<p>para</p>") && body.contains("<p>after</p>"),
        "{body}"
    );
    assert!(
        body.contains("<li>•&nbsp;a</li>\n<li>•&nbsp;b</li>"),
        "{body}"
    );
}

#[test]
fn bullets_are_font_independent_unicode_glyphs() {
    // Report v5 M-3: WPS draws disc/square from Symbol/Wingdings, which are
    // blank when those fonts are missing; the glyph is text instead.
    let html = render_word_html("- a\n  - b\n    - c\n      - d\n");
    let body = body(&html);
    for marker in [
        "<li>•&nbsp;a",
        "<li>◦&nbsp;b",
        "<li>▪&nbsp;c",
        "<li>•&nbsp;d",
    ] {
        assert!(body.contains(marker), "{marker}: {body}");
    }
    for bad in [
        "disc",
        "circle",
        "square",
        "Symbol",
        "Wingdings",
        "mso-bullet",
    ] {
        assert!(!body.contains(bad), "{bad}: {body}");
    }
    assert_eq!(body.matches("list-style-type:none").count(), 4, "{body}");
}
