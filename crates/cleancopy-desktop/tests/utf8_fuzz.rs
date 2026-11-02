//! Property tests: no conversion may panic on multi-byte text (report v4 C-1).

use cleancopy_core::{
    render_html, render_markdown, render_numbered, render_plain, render_rtf, render_word_html,
    NumberingStyle,
};
use cleancopy_desktop::html_source::{html_to_markdown, visible_chars};
use proptest::prelude::*;

const PIECES: &[&str] = &[
    "研发",
    "测试团队",
    "😀",
    "👨‍👩‍👧",
    "é",
    "İ",
    "ß",
    "&amp;",
    "&lt;",
    "&gt;",
    "&nbsp;",
    "&#x4E2D;",
    "&#20013;",
    "&#xZZ;",
    "&bogus;",
    "&",
    ";",
    "&研;",
    "&amp",
    "<p>",
    "</p>",
    "<pre><code class=\"language-rust\">",
    "</code></pre>",
    "<br>",
    "<h2>",
    "</h2>",
    "<li>",
    "<ul>",
    "</ul>",
    "<code-block>",
    "</code-block>",
    "<button>复制</button>",
    "# ",
    "## ",
    "- ",
    "1. ",
    "**",
    "`",
    "```\n",
    "|a|b|\n|-|-|\n",
    "\n",
    " ",
    "x",
    "<",
    ">",
    "\"",
];

fn mixed() -> impl Strategy<Value = String> {
    prop::collection::vec(
        prop_oneof![
            prop::sample::select(PIECES).prop_map(str::to_owned),
            any::<char>().prop_map(String::from),
        ],
        0..60,
    )
    .prop_map(|v| v.concat())
}

fn check_all(s: &str) {
    let _ = html_to_markdown(s);
    let _ = visible_chars(s);
    let _ = render_plain(s);
    let _ = render_markdown(s);
    let _ = render_html(s);
    let _ = render_rtf(s);
    let _ = render_word_html(s);
    for style in [
        NumberingStyle::Chinese,
        NumberingStyle::ChineseCompact,
        NumberingStyle::Decimal,
    ] {
        let _ = render_numbered(s, style);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]
    #[test]
    fn conversions_never_panic_on_mixed_text(s in mixed()) {
        check_all(&s);
    }

    #[test]
    fn conversions_never_panic_on_arbitrary_text(s in "\\PC{0,200}") {
        check_all(&s);
    }
}

#[test]
fn entity_followed_by_chinese_regression() {
    for html in [
        "<p>研发&amp;测试团队</p>",
        "<p>A &amp; B 普通</p>",
        "<p>&研发测试团队</p>",
        "<p>&#x4E2D;😀&lt;研</p>",
    ] {
        check_all(html);
        assert!(visible_chars(html) > 0);
    }
    assert!(html_to_markdown("<p>研发&amp;测试团队</p>").contains("研发&测试团队"));
}
