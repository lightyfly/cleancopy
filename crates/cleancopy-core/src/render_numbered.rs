use crate::parse;
use comrak::{
    nodes::{AstNode, ListType, NodeValue},
    Arena,
};
use serde::{Deserialize, Serialize};

/// Heading numbering patterns. Chinese styles number H1–H3 only
/// (`一、 （一） 1.` / compact `一、 1. （1）`); Decimal numbers H2+ below an
/// unnumbered H1 title (`1. 1.1 1.1.1`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NumberingStyle {
    #[default]
    Chinese,
    ChineseCompact,
    Decimal,
}

/// Render plain text with heading-relative numbering and four-space list nesting.
/// Output uses LF and has no terminal newline. H1 resets the document counters;
/// skipped heading ancestors are treated as 1. Decimal list blocks in the same
/// section continue across paragraphs; other styles restart each block at 1.
/// Source markers are ignored. Beyond named Chinese levels, decimal paths
/// provide unambiguous numbering without a nesting limit.
pub fn render_numbered(markdown: &str, style: NumberingStyle) -> String {
    // Large documents are handled one chunk of top-level blocks at a time
    // (parse::chunks). The heading outline decides document-wide numbering,
    // so collect it first, then render with one renderer across chunks.
    let chunks = parse::chunks(markdown);
    let mut levels = Vec::new();
    let mut first_is_h1 = false;
    for (index, chunk) in chunks.iter().enumerate() {
        let arena = Arena::new();
        let root = parse::parse(&arena, chunk);
        for (position, node) in root.children().enumerate() {
            if let NodeValue::Heading(h) = &node.data.borrow().value {
                levels.push(h.level);
                first_is_h1 |= index == 0 && position == 0 && h.level == 1;
            }
        }
    }
    // A single leading H1 is the unnumbered document title and H2 becomes the
    // first numbered level; with several H1s (or one that is not first) H1 is
    // numbered. A document without any H1 numbers from H2 like the title case.
    let h1_count = levels.iter().filter(|&&l| l == 1).count();
    let offset = usize::from((first_is_h1 && h1_count == 1) || h1_count == 0);
    let list_base = document_list_base(&levels, style, offset);
    let mut renderer = Renderer {
        offset,
        list_base,
        style,
        headings: vec![],
        heading_level: 0,
        next_list: 1,
        bullets: false,
    };
    let mut out = String::new();
    for chunk in chunks {
        if crate::cancel::cancelled() {
            break;
        }
        let arena = Arena::new();
        // A chunk never starts with a list, so the heading-then-list single
        // newline inside Renderer::blocks never spans a chunk boundary.
        let text = renderer.blocks(parse::parse(&arena, chunk));
        if !text.is_empty() {
            if !out.is_empty() {
                out.push_str("\n\n");
            }
            out.push_str(&text);
        }
    }
    out
}

struct Renderer {
    /// 1 when H1 is the document title (or absent), else 0.
    offset: usize,
    /// First list-ladder rung for every list in the document: one below the
    /// deepest heading numbering scheme used anywhere (design.md §7.1).
    list_base: usize,
    style: NumberingStyle,
    headings: Vec<usize>,
    heading_level: usize,
    next_list: usize,
    /// Rendering a top-level bullet list that follows a numbered list.
    bullets: bool,
}

impl Renderer {
    fn blocks<'a>(&mut self, node: &'a AstNode<'a>) -> String {
        let mut out = String::new();
        let mut previous_heading = false;
        for child in node.children() {
            if crate::cancel::cancelled() {
                break;
            }
            let is_list = matches!(child.data.borrow().value, NodeValue::List(_));
            let is_heading = matches!(child.data.borrow().value, NodeValue::Heading(_));
            let text = self.block(child);
            if text.is_empty() {
                continue;
            }
            if !out.is_empty() {
                out.push_str(if previous_heading && is_list {
                    "\n"
                } else {
                    "\n\n"
                });
            }
            out.push_str(&text);
            previous_heading = is_heading;
        }
        out
    }

    fn block<'a>(&mut self, node: &'a AstNode<'a>) -> String {
        match &node.data.borrow().value {
            NodeValue::Heading(h) => {
                self.next_list = 1;
                // Numbered heading levels (design.md §7.1): Chinese styles
                // number three levels below the title, Decimal H2–H4 (H1 is
                // its title). Deeper headings are standalone unnumbered
                // paragraphs that do not change the outline.
                let Some(level) = numbered_level(self.style, h.level, self.offset) else {
                    return inline(node);
                };
                self.heading_level = level;
                if level == 0 {
                    self.headings.clear();
                    return inline(node);
                }
                self.headings.truncate(level);
                self.headings.resize(level, 0);
                for count in &mut self.headings[..level - 1] {
                    if *count == 0 {
                        *count = 1;
                    }
                }
                self.headings[level - 1] += 1;
                format!("{}{}", marker(self.style, &self.headings), inline(node))
            }
            NodeValue::Paragraph => inline(node),
            NodeValue::List(list)
                if self.style == NumberingStyle::Decimal
                    && list.list_type == ListType::Bullet
                    && node.previous_sibling().is_some_and(|prev| {
                        matches!(&prev.data.borrow().value,
                            NodeValue::List(p) if p.list_type == ListType::Ordered)
                    }) =>
            {
                // Decimal: a bullet list right after a numbered list keeps
                // bullets instead of continuing the numbers (report v6 D-3).
                self.bullets = true;
                let text = self.list(node, &[], 1);
                self.bullets = false;
                text
            }
            NodeValue::List(_) => {
                let start = if self.style == NumberingStyle::Decimal {
                    self.next_list
                } else {
                    1
                };
                self.next_list = start + node.children().count();
                self.list(node, &[], start)
            }
            NodeValue::CodeBlock(code) => code
                .literal
                .strip_suffix('\n')
                .unwrap_or(&code.literal)
                .split('\n')
                .map(|line| format!("    {line}"))
                .collect::<Vec<_>>()
                .join("\n"),
            NodeValue::Table(_) => node
                .children()
                .map(|row| {
                    row.children()
                        .map(|cell| inline(cell).replace('\n', " "))
                        .collect::<Vec<_>>()
                        .join("\t")
                })
                .collect::<Vec<_>>()
                .join("\n"),
            NodeValue::HtmlBlock(html) => {
                strip_tags(&html.literal).trim_end_matches('\n').to_owned()
            }
            NodeValue::ThematicBreak => String::new(),
            _ => self.blocks(node),
        }
    }

    fn list<'a>(&mut self, node: &'a AstNode<'a>, path: &[usize], start: usize) -> String {
        node.children()
            .enumerate()
            .map(|(index, item)| {
                let mut item_path = path.to_vec();
                item_path.push(start + index);
                let indent = "    ".repeat(path.len());
                let marker = if self.bullets && path.is_empty() {
                    "• ".to_owned()
                } else {
                    list_marker(self.list_base + path.len(), start + index)
                };
                let mut out = format!("{indent}{marker}");
                let mut first = true;
                let mut next_nested = 1;
                for child in item.children() {
                    if matches!(child.data.borrow().value, NodeValue::List(_)) {
                        let nested_start = if self.style == NumberingStyle::Decimal {
                            next_nested
                        } else {
                            1
                        };
                        next_nested = nested_start + child.children().count();
                        out.push('\n');
                        out.push_str(&self.list(child, &item_path, nested_start));
                        first = false;
                    } else {
                        // A heading inside an item belongs to that item, not the
                        // document outline; its list marker already supplies numbering.
                        let text = if matches!(child.data.borrow().value, NodeValue::Heading(_)) {
                            inline(child)
                        } else {
                            self.block(child)
                        };
                        if text.is_empty() {
                            continue;
                        }
                        for (line_index, line) in text.split('\n').enumerate() {
                            if !first || line_index > 0 {
                                out.push('\n');
                                out.push_str(&indent);
                                out.push_str("    ");
                            }
                            out.push_str(line);
                            first = false;
                        }
                    }
                }
                if first {
                    out.trim_end_matches(' ').to_owned()
                } else {
                    out
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Outline level a heading is numbered at (0 = unnumbered title), or `None`
/// for a deeper, unnumbered heading.
fn numbered_level(style: NumberingStyle, heading: u8, offset: usize) -> Option<usize> {
    let level = if style == NumberingStyle::Decimal {
        usize::from(heading).saturating_sub(1)
    } else {
        usize::from(heading).saturating_sub(offset)
    };
    (level <= 3).then_some(level)
}

/// List base for the whole document: the rung after the deepest heading
/// scheme any outline heading uses, so no list ever shares a marker shape
/// with a heading (e.g. headings reach `1.` → every list starts at （1）).
fn document_list_base(levels: &[u8], style: NumberingStyle, offset: usize) -> usize {
    levels
        .iter()
        .filter_map(|&h| numbered_level(style, h, offset))
        .filter(|&level| level > 0)
        .filter_map(|level| heading_rung(style, level))
        .map(|rung| rung + 1)
        .max()
        .unwrap_or(0)
}

/// List numbering ladder (design.md「中文序号规则」): lists never take the
/// heading-owned 一、/（一） levels. Rungs: `1.` → `（1）` → `①` → `•`
/// (all deeper levels stay `•`).
const LIST_LADDER: usize = 3;

fn list_marker(rung: usize, n: usize) -> String {
    match rung {
        0 => format!("{n}. "),
        1 => format!("（{n}）"),
        2 if n <= 50 => circled(n),
        2 => format!("{n}) "),
        _ => "• ".into(),
    }
}

/// Which list-ladder rung (if any) a heading level's marker occupies.
fn heading_rung(style: NumberingStyle, level: usize) -> Option<usize> {
    let rung = match (style, level) {
        // 一、 / （一） are heading-only; H3 `1.` pushes lists to （1）.
        (NumberingStyle::Chinese, 1 | 2) => return None,
        (NumberingStyle::Chinese, _) => 0,
        // Compact: 一、 / 1. / （1）.
        (NumberingStyle::ChineseCompact, 1) => return None,
        (NumberingStyle::ChineseCompact, 2) => 0,
        (NumberingStyle::ChineseCompact, _) => 1,
        // Decimal headings (1. / 1.1 / …) and anything deeper all read as
        // the `1.` family.
        (NumberingStyle::Decimal, _) => 0,
    };
    Some(rung.min(LIST_LADDER - 1))
}

fn circled(n: usize) -> String {
    let base = match n {
        1..=20 => 0x2460,
        21..=35 => 0x3251 - 20,
        _ => 0x32b1 - 35,
    };
    char::from_u32(base + n as u32 - 1).unwrap().to_string()
}

fn marker(style: NumberingStyle, path: &[usize]) -> String {
    let n = *path.last().unwrap_or(&1);
    match (style, path.len()) {
        (NumberingStyle::Chinese | NumberingStyle::ChineseCompact, 1) => {
            format!("{}、", chinese(n))
        }
        (NumberingStyle::Chinese, 2) => format!("（{}）", chinese(n)),
        (NumberingStyle::Chinese, _) | (NumberingStyle::ChineseCompact, 2) => format!("{n}. "),
        (NumberingStyle::ChineseCompact, _) => format!("（{n}）"),
        _ => {
            let path = path
                .iter()
                .map(usize::to_string)
                .collect::<Vec<_>>()
                .join(".");
            if !path.contains('.') {
                format!("{path}. ")
            } else {
                format!("{path} ")
            }
        }
    }
}

fn chinese(n: usize) -> String {
    const DIGITS: [&str; 10] = ["零", "一", "二", "三", "四", "五", "六", "七", "八", "九"];
    if n == 0 {
        return DIGITS[0].into();
    }
    if n >= 10_000 {
        return n.to_string();
    }
    let mut out = String::new();
    let mut zero = false;
    for (unit, suffix) in [(1000, "千"), (100, "百"), (10, "十"), (1, "")] {
        let digit = n / unit % 10;
        if digit == 0 {
            if !out.is_empty() {
                zero = true;
            }
            continue;
        }
        if zero {
            out.push_str(DIGITS[0]);
            zero = false;
        }
        if !(unit == 10 && digit == 1 && out.is_empty()) {
            out.push_str(DIGITS[digit]);
        }
        out.push_str(suffix);
    }
    out
}

fn inline<'a>(node: &'a AstNode<'a>) -> String {
    node.children().map(inline_raw).collect()
}

fn inline_raw<'a>(node: &'a AstNode<'a>) -> String {
    match &node.data.borrow().value {
        NodeValue::Text(text) => text.clone(),
        NodeValue::Code(code) => code.literal.clone(),
        NodeValue::SoftBreak | NodeValue::LineBreak => "\n".into(),
        NodeValue::HtmlInline(html) => strip_tags(html),
        NodeValue::FootnoteReference(note) => format!("[{}]", note.ix),
        _ => node.children().map(inline_raw).collect(),
    }
}

fn strip_tags(html: &str) -> String {
    let mut out = String::new();
    let mut tag = String::new();
    let mut inside = false;
    let mut quote = None;
    for ch in html.chars() {
        if inside {
            if let Some(q) = quote {
                if ch == q {
                    quote = None;
                }
            } else if ch == '\'' || ch == '"' {
                quote = Some(ch);
            } else if ch == '>' {
                inside = false;
                if tag
                    .trim()
                    .trim_end_matches('/')
                    .trim()
                    .eq_ignore_ascii_case("br")
                {
                    out.push('\n');
                }
            }
            tag.push(ch);
        } else if ch == '<' {
            inside = true;
            tag.clear();
        } else {
            out.push(ch);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn chinese_large_counters() {
        for (n, expected) in [
            (1, "一"),
            (10, "十"),
            (11, "十一"),
            (20, "二十"),
            (101, "一百零一"),
            (1010, "一千零一十"),
            (9999, "九千九百九十九"),
            (10000, "10000"),
        ] {
            assert_eq!(chinese(n), expected);
        }
    }
}
