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

/// Inputs at least this large are parsed in chunks (see [`chunks`]).
const CHUNK_MIN_INPUT: usize = 1 << 20;
/// Approximate size of one chunk.
const CHUNK_TARGET: usize = 64 << 10;

thread_local! {
    static CHUNK_SIZES: std::cell::Cell<(usize, usize)> =
        const { std::cell::Cell::new((CHUNK_MIN_INPUT, CHUNK_TARGET)) };
}

/// Run `f` with different chunking thresholds (tests use tiny ones to prove
/// chunked rendering is identical to whole-document rendering).
#[doc(hidden)]
pub fn with_chunk_sizes<T>(min_input: usize, target: usize, f: impl FnOnce() -> T) -> T {
    let previous = CHUNK_SIZES.with(|c| c.replace((min_input, target)));
    let result = f();
    CHUNK_SIZES.with(|c| c.set(previous));
    result
}

/// Split a large document into runs of whole top-level blocks.
///
/// comrak's AST costs roughly 500 bytes per node, so a 5 MB document peaks
/// near 1 GB when parsed at once. Renderers parse and render one chunk at a
/// time instead, dropping each AST before the next. A split is made only
/// where the result is provably the same as parsing the whole document:
///
/// - at a blank line followed by an unindented line, outside fenced code;
/// - never before a list marker, so lists (and their numbering) stay whole;
/// - never in documents with document-wide state or multi-paragraph raw
///   blocks: link reference or footnote definitions, raw HTML blocks, or
///   indented fences. Those documents are parsed whole.
pub fn chunks(input: &str) -> Vec<&str> {
    let (min_input, target) = CHUNK_SIZES.with(|c| c.get());
    if input.len() < min_input {
        return vec![input];
    }
    let mut out = Vec::new();
    let mut chunk_start = 0;
    let mut offset = 0;
    let mut previous_blank = false;
    // Open fence: (character, length).
    let mut fence: Option<(u8, usize)> = None;
    for line in input.split_inclusive('\n') {
        let start = offset;
        offset += line.len();
        let content = line.trim_end_matches(['\n', '\r']);
        let indent = content.len() - content.trim_start_matches([' ', '\t']).len();
        let rest = &content.as_bytes()[indent..];
        let blank = rest.is_empty();
        let run = |c: u8| rest.iter().take_while(|&&b| b == c).count();
        let fence_char = rest.first().copied().filter(|c| matches!(c, b'`' | b'~'));
        if let Some(c) = fence_char.filter(|&c| run(c) >= 3) {
            if indent > 3 && fence.is_none() {
                return vec![input];
            }
            match fence {
                Some((open, len))
                    if open == c
                        && run(c) >= len
                        && indent <= 3
                        && rest[run(c)..].iter().all(|b| b.is_ascii_whitespace()) =>
                {
                    fence = None;
                }
                None => fence = Some((c, run(c))),
                _ => {}
            }
            previous_blank = false;
            continue;
        }
        if fence.is_some() {
            previous_blank = false;
            continue;
        }
        if indent <= 3 && (rest.first() == Some(&b'<') || is_definition(rest)) {
            return vec![input];
        }
        if previous_blank
            && !blank
            && indent == 0
            && !is_list_marker(rest)
            && start - chunk_start >= target
        {
            out.push(&input[chunk_start..start]);
            chunk_start = start;
        }
        previous_blank = blank;
    }
    out.push(&input[chunk_start..]);
    out
}

fn is_definition(line: &[u8]) -> bool {
    line.first() == Some(&b'[')
        && line
            .iter()
            .position(|&b| b == b']')
            .is_some_and(|close| line.get(close + 1) == Some(&b':'))
}

fn is_list_marker(line: &[u8]) -> bool {
    let after = |n: usize| line.get(n).is_none_or(|b| b.is_ascii_whitespace());
    match line.first() {
        Some(b'-' | b'+' | b'*') => after(1),
        Some(b'0'..=b'9') => {
            let digits = line.iter().take_while(|b| b.is_ascii_digit()).count();
            matches!(line.get(digits), Some(b'.' | b')')) && after(digits + 1)
        }
        _ => false,
    }
}
