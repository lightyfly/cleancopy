use comrak::{nodes::AstNode, parse_document, Arena, Options};

pub fn options() -> Options<'static> {
    let mut options = Options::default();
    options.extension.table = true;
    options.extension.tasklist = true;
    options.extension.strikethrough = true;
    options.extension.autolink = true;
    options.extension.footnotes = true;
    options
}

pub fn parse<'a>(arena: &'a Arena<AstNode<'a>>, markdown: &str) -> &'a AstNode<'a> {
    parse_document(arena, markdown, &options())
}

/// Document title rule (design.md §7.1): when the document has exactly one
/// H1 and it is the first block, that H1 is the document title.
pub fn title_heading<'a>(root: &'a AstNode<'a>) -> Option<&'a AstNode<'a>> {
    use comrak::nodes::NodeValue;
    let is_h1 =
        |n: &AstNode<'_>| matches!(&n.data.borrow().value, NodeValue::Heading(h) if h.level == 1);
    let first = root.first_child()?;
    (is_h1(first) && root.children().filter(|n| is_h1(n)).count() == 1).then_some(first)
}

/// Whether the top-level outline has no H1 at all.
pub fn has_no_h1<'a>(root: &'a AstNode<'a>) -> bool {
    use comrak::nodes::NodeValue;
    !root
        .children()
        .any(|n| matches!(&n.data.borrow().value, NodeValue::Heading(h) if h.level == 1))
}
