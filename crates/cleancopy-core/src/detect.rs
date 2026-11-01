//! Detect Markdown formatting using the same parser configuration as the renderers.

use comrak::{nodes::NodeValue, parse_document, Arena};

/// Return whether `text` contains a parsed Markdown formatting construct.
///
/// Plain paragraphs, soft line breaks, and escaped or unmatched punctuation do
/// not count. Bare URLs and email addresses do not count either, but explicit
/// links (including angle-bracket autolinks) do. Raw HTML, which CommonMark
/// accepts as markup, and explicit hard line breaks also count.
pub fn is_markdown(text: &str) -> bool {
    let arena = Arena::new();
    let mut options = crate::parse::options();
    // Disable GFM's bare URL/email recognition, not CommonMark's <...> links.
    options.extension.autolink = false;
    let root = parse_document(&arena, text, &options);

    root.descendants().any(|node| {
        matches!(
            node.data.borrow().value,
            NodeValue::Heading(_)
                | NodeValue::List(_)
                | NodeValue::Item(_)
                | NodeValue::TaskItem(_)
                | NodeValue::BlockQuote
                | NodeValue::ThematicBreak
                | NodeValue::CodeBlock(_)
                | NodeValue::Code(_)
                | NodeValue::Emph
                | NodeValue::Strong
                | NodeValue::Strikethrough
                | NodeValue::Link(_)
                | NodeValue::Image(_)
                | NodeValue::Table(_)
                | NodeValue::FootnoteDefinition(_)
                | NodeValue::FootnoteReference(_)
                | NodeValue::HtmlBlock(_)
                | NodeValue::HtmlInline(_)
                | NodeValue::LineBreak
        )
    })
}

#[cfg(test)]
mod tests {
    use super::is_markdown;

    #[test]
    fn detects_parsed_markdown_constructs() {
        for text in [
            "# Heading",
            "Heading\n=======",
            "- item",
            "1. item",
            "- [x] task",
            "> quote",
            "---",
            "```rust\nlet x = 1;\n```",
            "    indented code",
            "some `code` here",
            "some *emphasis* here",
            "some **strong** here",
            "some ~~deleted~~ text",
            "[label](https://example.com)",
            "[label][ref]\n\n[ref]: https://example.com",
            "![alt](image.png)",
            "<https://example.com>",
            "<person@example.com>",
            "| a | b |\n| --- | --- |\n| c | d |",
            "note[^1]\n\n[^1]: footnote",
            "<div>HTML</div>",
            "some <em>HTML</em>",
            "hard  \nbreak",
            "hard\\\nbreak",
        ] {
            assert!(is_markdown(text), "missed Markdown: {text:?}");
        }
    }

    #[test]
    fn ignores_plain_text_and_unparsed_inline_syntax() {
        for text in [
            "",
            " \t\n",
            "ordinary prose",
            "first line\nsecond line",
            "first paragraph\n\nsecond paragraph",
            "https://example.com",
            "http://example.com/path?q=1",
            "www.example.com",
            "person@example.com",
            "Visit https://example.com or email person@example.com.",
            r"\*escaped emphasis\*",
            r"\# escaped heading",
            r"\[label](https://example.com)",
            "unmatched *emphasis",
            "unmatched **strong",
            "unmatched ~~strike",
            "unmatched `code",
            "[label](unfinished",
            "[unresolved reference]",
            "snake_case_identifier",
            "2 * 3 = 6",
            "#not-a-heading",
            "a | b",
        ] {
            assert!(!is_markdown(text), "misclassified plain text: {text:?}");
        }
    }

    #[test]
    fn bare_links_do_not_hide_other_formatting() {
        assert!(is_markdown("https://example.com and **bold**"));
        assert!(is_markdown("person@example.com and `code`"));
        assert!(crate::parse::options().extension.autolink);
    }
}
