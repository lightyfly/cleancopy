use cleancopy_core::{render_numbered, NumberingStyle};

const SAMPLE: &str = "# 项目周报\n## 本周进展\n- 完成 **剪贴板监听**\n- 接入 Word 富文本\n  - 标题映射 标题1/2\n  - 表格与代码块\n## 下周计划\n1. Windows 托盘\n2. 识别失败兜底";

#[test]
fn exact_settings_preview_and_newline_contract() {
    assert_eq!(render_numbered(SAMPLE, NumberingStyle::default()), "项目周报\n\n一、本周进展\n1. 完成 剪贴板监听\n2. 接入 Word 富文本\n    （1）标题映射 标题1/2\n    （2）表格与代码块\n\n二、下周计划\n1. Windows 托盘\n2. 识别失败兜底");
    for input in ["", " \n\t", "<!-- comment -->"] {
        assert_eq!(render_numbered(input, NumberingStyle::Chinese), "");
    }
    for input in ["plain", "plain\n", "plain\r\n"] {
        assert_eq!(render_numbered(input, NumberingStyle::Chinese), "plain");
    }
}

#[test]
fn alternative_settings_previews() {
    assert_eq!(render_numbered(SAMPLE, NumberingStyle::ChineseCompact), "项目周报\n\n一、本周进展\n1. 完成 剪贴板监听\n2. 接入 Word 富文本\n    （1）标题映射 标题1/2\n    （2）表格与代码块\n\n二、下周计划\n1. Windows 托盘\n2. 识别失败兜底");
    assert_eq!(render_numbered(SAMPLE, NumberingStyle::Decimal), "项目周报\n\n1. 本周进展\n（1）完成 剪贴板监听\n（2）接入 Word 富文本\n    ①标题映射 标题1/2\n    ②表格与代码块\n\n2. 下周计划\n（1）Windows 托盘\n（2）识别失败兜底");
}

#[test]
fn headings_reset_descendants_and_titles_reset_document() {
    let input = "# T\n## A\n### B\n### C\n#### D\n##### E\n###### F\n## G\n### H\n# U\n## I";
    assert_eq!(
        render_numbered(input, NumberingStyle::Chinese),
        "一、T\n\n（一）A\n\n1. B\n\n2. C\n\nD\n\nE\n\nF\n\n（二）G\n\n1. H\n\n二、U\n\n（一）I"
    );
    assert_eq!(
        render_numbered("### A\n#### B\n## C\n### D", NumberingStyle::Decimal),
        "1.1 A\n\n1.1.1 B\n\n2. C\n\n2.1 D"
    );
}

#[test]
fn deep_lists_and_sibling_resets_in_all_styles() {
    let input = "- a\n  - b\n    - c\n      - d\n        - e\n      - f\n    - g\n  - h\n- i";
    for (style, expected) in [
        (NumberingStyle::Chinese, "1. a\n    （1）b\n        ①c\n            • d\n                • e\n            • f\n        ②g\n    （2）h\n2. i"),
        (NumberingStyle::ChineseCompact, "1. a\n    （1）b\n        ①c\n            • d\n                • e\n            • f\n        ②g\n    （2）h\n2. i"),
        (NumberingStyle::Decimal, "1. a\n    （1）b\n        ①c\n            • d\n                • e\n            • f\n        ②g\n    （2）h\n2. i"),
    ] { assert_eq!(render_numbered(input, style), expected); }
    assert_eq!(
        render_numbered("9. a\n10. b\n\nparagraph\n\n- c", NumberingStyle::Decimal),
        "1. a\n2. b\n\nparagraph\n\n3. c"
    );
}

#[test]
fn lists_start_below_the_nearest_heading_scheme() {
    // Headings own 一、/（一）; lists start at 1. unless the heading chain
    // already uses 1. (then （1）), and so on down the ladder.
    // Single leading H1 = title (unnumbered); no H1 numbers from H2 the same way.
    for (style, heading, expected) in [
        (NumberingStyle::Chinese, "# T", "T\n1. item"),
        (NumberingStyle::Chinese, "## T", "一、T\n1. item"),
        (NumberingStyle::Chinese, "### T", "（一）T\n1. item"),
        (NumberingStyle::Chinese, "#### T", "1. T\n（1）item"),
        // H5+ are unnumbered; their lists follow the numbered chain.
        (
            NumberingStyle::Chinese,
            "### S\n###### T",
            "（一）S\n\nT\n1. item",
        ),
        (NumberingStyle::ChineseCompact, "## T", "一、T\n1. item"),
        (NumberingStyle::ChineseCompact, "### T", "1. T\n（1）item"),
        (NumberingStyle::ChineseCompact, "#### T", "（1）T\n①item"),
        (
            NumberingStyle::ChineseCompact,
            "### S\n###### T",
            "1. S\n\nT\n（1）item",
        ),
        // Several H1s: H1 is numbered, H4+ unnumbered.
        (
            NumberingStyle::Chinese,
            "# A\n# B\n### T",
            "一、A\n\n二、B\n\n1. T\n（1）item",
        ),
        (
            NumberingStyle::Chinese,
            "# A\n# B\n#### T",
            "一、A\n\n二、B\n\nT\n1. item",
        ),
        (
            NumberingStyle::ChineseCompact,
            "# A\n# B\n### T",
            "一、A\n\n二、B\n\n（1）T\n①item",
        ),
        (NumberingStyle::Decimal, "## T", "1. T\n（1）item"),
        (NumberingStyle::Decimal, "### T", "1.1 T\n（1）item"),
    ] {
        let out = render_numbered(&format!("{heading}\n- item"), style);
        assert_eq!(out, expected, "{style:?} {heading}");
    }
}

#[test]
fn single_leading_h1_is_title_and_multiple_h1s_are_numbered() {
    let input = "# A\n## B\n### C\n#### D\n- l\n##### E";
    assert_eq!(
        render_numbered(input, NumberingStyle::Chinese),
        "A\n\n一、B\n\n（一）C\n\n1. D\n（1）l\n\nE"
    );
    assert_eq!(
        render_numbered(input, NumberingStyle::ChineseCompact),
        "A\n\n一、B\n\n1. C\n\n（1）D\n①l\n\nE"
    );
    let multi = "# A\n## B\n### C\n#### D\n- l\n##### E\n# A2\n## B2";
    assert_eq!(
        render_numbered(multi, NumberingStyle::Chinese),
        "一、A\n\n（一）B\n\n1. C\n\nD\n（1）l\n\nE\n\n二、A2\n\n（一）B2"
    );
    assert_eq!(
        render_numbered(multi, NumberingStyle::ChineseCompact),
        "一、A\n\n1. B\n\n（1）C\n\nD\n①l\n\nE\n\n二、A2\n\n1. B2"
    );
    // One H1 that is not the first block is not a title.
    assert_eq!(
        render_numbered("intro\n# A\n## B", NumberingStyle::Chinese),
        "intro\n\n一、A\n\n（一）B"
    );
    // Decimal: H1 always unnumbered, H5+ unnumbered (bold in rich output).
    assert_eq!(
        render_numbered(multi, NumberingStyle::Decimal),
        "A\n\n1. B\n\n1.1 C\n\n1.1.1 D\n（1）l\n\nE\n\nA2\n\n1. B2"
    );
}

#[test]
fn lists_never_count_toward_heading_numbering_or_repeat_a_scheme() {
    // Report v3 O-3: list （一） followed by a sub-heading （一） repeated.
    let input = "## 甲组\n- 甲\n- 乙\n### 小节A\n- 一\n  - 二\n### 小节B";
    let out = render_numbered(input, NumberingStyle::Chinese);
    assert_eq!(
        out,
        "一、甲组\n1. 甲\n2. 乙\n\n（一）小节A\n1. 一\n    （1）二\n\n（二）小节B"
    );
    let decimal = render_numbered(input, NumberingStyle::Decimal);
    assert_eq!(
        decimal,
        "1. 甲组\n（1）甲\n（2）乙\n\n1.1 小节A\n（1）一\n    ①二\n\n1.2 小节B"
    );
    // No two nesting levels of one block share a scheme.
    for style in [
        NumberingStyle::Chinese,
        NumberingStyle::ChineseCompact,
        NumberingStyle::Decimal,
    ] {
        // Two levels: the ladder ends in a repeating `•`.
        let out = render_numbered("#### H\n- a\n  - b", style);
        let markers: Vec<String> = out
            .lines()
            .skip(1)
            .map(|l| l.trim_start().chars().take(1).collect())
            .collect();
        let mut unique = markers.clone();
        unique.dedup();
        assert_eq!(unique.len(), markers.len(), "{style:?}: {out}");
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
        "1. first\n    continued\n    second paragraph\n    （1）nested\n2. last"
    );
}

#[test]
fn nested_first_items_and_embedded_headings_preserve_item_boundaries() {
    assert_eq!(
        render_numbered("-\n  - child\n\n  tail", NumberingStyle::Decimal),
        "1. \n    （1）child\n    tail"
    );
    assert_eq!(
        render_numbered(
            "## Section\n\n- ### Item heading\n  - child\n- sibling",
            NumberingStyle::Decimal
        ),
        "1. Section\n（1）Item heading\n    ①child\n（2）sibling"
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
fn unnumbered_deep_headings_are_standalone_standalone_paragraphs() {
    assert_eq!(
        render_numbered(
            "###### Section\n- one\n- two\n- three\n###### A\n###### B\n###### C",
            NumberingStyle::Chinese
        ),
        "Section\n1. one\n2. two\n3. three\n\nA\n\nB\n\nC"
    );
    assert_eq!(
        render_numbered(
            "# T\n# T2\n### Sub\n#### Section\n- one\n  - alpha\n  - beta\n  - gamma\n- two\n- three",
            NumberingStyle::Chinese
        ),
        "一、T\n\n二、T2\n\n1. Sub\n\nSection\n（1）one\n    ①alpha\n    ②beta\n    ③gamma\n（2）two\n（3）three"
    );
}

#[test]
fn decimal_lists_continue_across_paragraphs_and_reset_at_headings() {
    assert_eq!(
        render_numbered("## A\n1. first\n\nparagraph\n\n1. second\n   - child\n\n## B\n1. third\n\n# New\n1. fourth", NumberingStyle::Decimal),
        "1. A\n（1）first\n\nparagraph\n\n（2）second\n    ①child\n\n2. B\n（1）third\n\nNew\n（1）fourth"
    );
    assert_eq!(
        render_numbered(
            "- parent\n  - child\n\n  paragraph\n\n  1. next\n- sibling\n  - reset",
            NumberingStyle::Decimal
        ),
        "1. parent\n    （1）child\n    paragraph\n    （2）next\n2. sibling\n    （1）reset"
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

#[test]
fn list_base_is_global_to_the_deepest_heading_scheme() {
    // Report v5 N-4: a list under H3 directly followed by an H4 must not share
    // the H4's `1.` / `（1）` shape.
    let input = "# 标题\n## 二级\n### 三级\n1. 步骤一\n2. 步骤二\n#### 四级\n- 项";
    assert_eq!(
        render_numbered(input, NumberingStyle::Chinese),
        "标题\n\n一、二级\n\n（一）三级\n（1）步骤一\n（2）步骤二\n\n1. 四级\n（1）项"
    );
    assert_eq!(
        render_numbered(input, NumberingStyle::ChineseCompact),
        "标题\n\n一、二级\n\n1. 三级\n①步骤一\n②步骤二\n\n（1）四级\n①项"
    );
    // Global, not per section: the first section's list already starts below
    // the `1.` used by a later H4, and the ladder continues （1）→①→•.
    let input = "## 甲\n- a\n  - b\n    - c\n## 乙\n### 丙\n#### 丁";
    assert_eq!(
        render_numbered(input, NumberingStyle::Chinese),
        "一、甲\n（1）a\n    ①b\n        • c\n\n二、乙\n\n（一）丙\n\n1. 丁"
    );
    // Decimal: H2–H4 numbered, H5+ unnumbered, lists below `1.`.
    assert_eq!(
        render_numbered(
            "## A\n### B\n#### C\n##### D\n###### E\n- l",
            NumberingStyle::Decimal
        ),
        "1. A\n\n1.1 B\n\n1.1.1 C\n\nD\n\nE\n（1）l"
    );
}
