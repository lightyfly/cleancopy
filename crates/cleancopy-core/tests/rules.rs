use cleancopy_core::render_plain;

fn plain(input: &str) -> String {
    render_plain(input).replace("\r\n", "\n")
}

#[test]
fn ordered_starts_and_nested_bullet_numbering() {
    assert_eq!(plain("3. parent\n   - child\n     - deep\n   - next\n4. last"),
        "3. parent\n    3.1 child\n        - deep\n    3.2 next\n4. last");
}

#[test]
fn tasks_have_checkboxes_not_numeric_markers() {
    assert_eq!(plain("- [ ] 待办\n- [x] 完成"), "☐ 待办\n☑ 完成");
}

#[test]
fn code_content_is_not_trimmed_or_blank_line_collapsed() {
    let literal = "  space  \n\n\n\t tab\n";
    assert_eq!(plain(&format!("```rust\n{literal}```")),
        format!("────────\n[rust]\n{literal}────────"));
}

#[test]
fn table_fallback_boundary_counts_display_columns() {
    for (length, fallback) in [(56, false), (57, true)] {
        let value = "x".repeat(length);
        let actual = plain(&format!("| 中 | A |\n| - | - |\n| 文 | {value} |"));
        assert_eq!(actual.starts_with("1. 中：文；A："), fallback, "{actual}");
    }
}

#[test]
fn inline_html_and_links_are_readable() {
    assert_eq!(plain("**粗** *斜* ~~删~~ `code` <b>中文</b><br>下一行 [文档](https://example.com) ![猫](cat.png)"),
        "粗 斜 删 code 中文\n下一行 文档 (https://example.com) [图片: 猫] (cat.png)");
    assert_eq!(plain("https://example.com"), "https://example.com");
}

#[test]
fn footnotes_are_numbered_by_reference_and_deduplicated() {
    assert_eq!(plain("B[^b], A[^a], B[^b].\n\n[^a]: first\n[^b]: second"),
        "B[1], A[2], B[1].\n\n[1] second\n\n[2] first");
}

#[test]
fn quotes_headings_and_paragraphs() {
    assert_eq!(plain("# 标题\n\n## 小节\n\n段落\n\n\n\n第二段\n\n---\n\n> 引用\n>\n> 第二段"),
        "标题\n\n【小节】\n\n段落\n\n第二段\n\n────────\n\n> 引用\n> \n> 第二段");
}
