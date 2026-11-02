use cleancopy_core::{render_numbered, NumberingStyle};

const SAMPLE: &str = "# 项目周报\n## 本周进展\n- 完成 **剪贴板监听**\n- 接入 Word 富文本\n  - 标题映射 标题1/2\n  - 表格与代码块\n## 下周计划\n1. Windows 托盘\n2. 识别失败兜底";

#[test]
fn exact_settings_preview_and_newline_contract() {
    assert_eq!(render_numbered(SAMPLE, NumberingStyle::default()), "项目周报\n\n一、本周进展\n（一）完成 剪贴板监听\n（二）接入 Word 富文本\n    1. 标题映射 标题1/2\n    2. 表格与代码块\n\n二、下周计划\n（一）Windows 托盘\n（二）识别失败兜底");
    for input in ["", " \n\t", "<!-- comment -->"] {
        assert_eq!(render_numbered(input, NumberingStyle::Chinese), "");
    }
    for input in ["plain", "plain\n", "plain\r\n"] {
        assert_eq!(render_numbered(input, NumberingStyle::Chinese), "plain");
    }
}

#[test]
fn alternative_settings_previews() {
    assert_eq!(render_numbered(SAMPLE, NumberingStyle::ChineseCompact), "项目周报\n\n一、本周进展\n1. 完成 剪贴板监听\n2. 接入 Word 富文本\n    (1) 标题映射 标题1/2\n    (2) 表格与代码块\n\n二、下周计划\n1. Windows 托盘\n2. 识别失败兜底");
    assert_eq!(render_numbered(SAMPLE, NumberingStyle::Decimal), "项目周报\n\n1. 本周进展\n1.1 完成 剪贴板监听\n1.2 接入 Word 富文本\n    1.2.1 标题映射 标题1/2\n    1.2.2 表格与代码块\n\n2. 下周计划\n2.1 Windows 托盘\n2.2 识别失败兜底");
}

#[test]
fn headings_reset_descendants_and_titles_reset_document() {
    let input = "# T\n## A\n### B\n### C\n#### D\n##### E\n###### F\n## G\n### H\n# U\n## I";
    assert_eq!(render_numbered(input, NumberingStyle::Chinese), "T\n\n一、A\n\n（一）B\n\n（二）C\n\n1. D\n\n（1）E\n\n①F\n\n二、G\n\n（一）H\n\nU\n\n一、I");
    assert_eq!(
        render_numbered("### A\n#### B\n## C\n### D", NumberingStyle::Decimal),
        "1.1 A\n\n1.1.1 B\n\n2. C\n\n2.1 D"
    );
}

#[test]
fn deep_lists_and_sibling_resets_in_all_styles() {
    let input = "- a\n  - b\n    - c\n      - d\n        - e\n      - f\n    - g\n  - h\n- i";
    for (style, expected) in [
        (NumberingStyle::Chinese, "一、a\n    （一）b\n        1. c\n            （1）d\n                ①e\n            （2）f\n        2. g\n    （二）h\n二、i"),
        (NumberingStyle::ChineseCompact, "一、a\n    1. b\n        (1) c\n            1.1.1.1 d\n                1.1.1.1.1 e\n            1.1.1.2 f\n        (2) g\n    2. h\n二、i"),
        (NumberingStyle::Decimal, "1. a\n    1.1 b\n        1.1.1 c\n            1.1.1.1 d\n                1.1.1.1.1 e\n            1.1.1.2 f\n        1.1.2 g\n    1.2 h\n2. i"),
    ] { assert_eq!(render_numbered(input, style), expected); }
    assert_eq!(
        render_numbered("9. a\n10. b\n\nparagraph\n\n- c", NumberingStyle::Decimal),
        "1. a\n2. b\n\nparagraph\n\n3. c"
    );
}

#[test]
fn every_heading_changes_list_numbering_level() {
    for (heading, expected) in [
        ("# T", "一、item"),
        ("## T", "（一）item"),
        ("### T", "1. item"),
        ("#### T", "（1）item"),
        ("##### T", "①item"),
        ("###### T", "a. item"),
    ] {
        assert!(
            render_numbered(&format!("{heading}\n- item"), NumberingStyle::Chinese)
                .ends_with(expected)
        );
    }
}

#[test]
fn inline_ast_stripping_tables_quotes_and_verbatim_code() {
    let input = "**bold** _italic_ ~~gone~~ `x_*` [label](https://example.com) ![alt](cat.png) \\*literal\\* <b>html</b><br>next\n\n> quote\n\n| **A** | B |\n| --- | --- |\n| `x` | y |\n\n```rust\n  x  \n\n\tcode\n```";
    assert_eq!(render_numbered(input, NumberingStyle::Chinese), "bold italic gone x_* label alt *literal* html\nnext\n\nquote\n\nA\tB\nx\ty\n\n      x  \n    \n    \tcode");
}

#[test]
fn multiline_and_loose_items_keep_indentation() {
    assert_eq!(
        render_numbered(
            "- first\n  continued\n\n  second paragraph\n  - nested\n- last",
            NumberingStyle::Chinese
        ),
        "一、first\n    continued\n    second paragraph\n    （一）nested\n二、last"
    );
}

#[test]
fn nested_first_items_and_embedded_headings_preserve_item_boundaries() {
    assert_eq!(
        render_numbered("-\n  - child\n\n  tail", NumberingStyle::Decimal),
        "1. \n    1.1 child\n    tail"
    );
    assert_eq!(
        render_numbered(
            "## Section\n\n- ### Item heading\n  - child\n- sibling",
            NumberingStyle::Decimal
        ),
        "1. Section\n1.1 Item heading\n    1.1.1 child\n1.2 sibling"
    );
}

#[test]
fn stripping_markup_preserves_original_inline_spaces() {
    for style in [
        NumberingStyle::Chinese,
        NumberingStyle::ChineseCompact,
        NumberingStyle::Decimal,
    ] {
        for (input, expected) in [
            ("**粗** _斜_", "粗 斜"),
            ("**粗**   _斜_", "粗   斜"),
            ("**A** **B**", "A B"),
            ("**粗**_斜_", "粗斜"),
            ("**粗 _内 斜_**  ~~删~~", "粗 内 斜  删"),
            ("[粗](https://example.com) _斜_ `a  b`", "粗 斜 a  b"),
            ("<b>粗</b>  <i>斜</i>", "粗  斜"),
        ] {
            assert_eq!(
                render_numbered(input, style),
                expected,
                "{style:?}: {input}"
            );
        }
        assert!(render_numbered("## **粗** _斜_\n- **粗**   _斜_", style).ends_with("粗   斜"));
    }
}

#[test]
fn chinese_fifth_and_sixth_levels_have_named_markers() {
    assert_eq!(
        render_numbered(
            "###### Section\n- one\n- two\n- three\n###### A\n###### B\n###### C",
            NumberingStyle::Chinese
        ),
        "①Section\na. one\nb. two\nc. three\n\n②A\n\n③B\n\n④C"
    );
    assert_eq!(
        render_numbered(
            "#### Section\n- one\n  - alpha\n  - beta\n  - gamma\n- two\n- three",
            NumberingStyle::Chinese
        ),
        "1. Section\n（1）one\n    ①alpha\n    ②beta\n    ③gamma\n（2）two\n（3）three"
    );
}

#[test]
fn decimal_lists_continue_across_paragraphs_and_reset_at_headings() {
    assert_eq!(
        render_numbered("## A\n1. first\n\nparagraph\n\n1. second\n   - child\n\n## B\n1. third\n\n# New\n1. fourth", NumberingStyle::Decimal),
        "1. A\n1.1 first\n\nparagraph\n\n1.2 second\n    1.2.1 child\n\n2. B\n2.1 third\n\nNew\n1. fourth"
    );
    assert_eq!(
        render_numbered(
            "- parent\n  - child\n\n  paragraph\n\n  1. next\n- sibling\n  - reset",
            NumberingStyle::Decimal
        ),
        "1. parent\n    1.1 child\n    paragraph\n    1.2 next\n2. sibling\n    2.1 reset"
    );
}

#[test]
fn serialization_names_and_default_are_stable() {
    assert_eq!(NumberingStyle::default(), NumberingStyle::Chinese);
    for (style, name) in [
        (NumberingStyle::Chinese, "chinese"),
        (NumberingStyle::ChineseCompact, "chinese_compact"),
        (NumberingStyle::Decimal, "decimal"),
    ] {
        let json = format!("\"{name}\"");
        assert_eq!(serde_json::to_string(&style).unwrap(), json);
        assert_eq!(
            serde_json::from_str::<NumberingStyle>(&json).unwrap(),
            style
        );
    }
    assert!(serde_json::from_str::<NumberingStyle>("\"Chinese\"").is_err());
}
