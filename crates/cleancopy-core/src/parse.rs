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
