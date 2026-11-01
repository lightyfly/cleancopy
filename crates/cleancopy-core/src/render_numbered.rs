use crate::parse;
use comrak::{
    nodes::{AstNode, NodeValue},
    Arena,
};
use serde::{Deserialize, Serialize};

/// Numbering patterns below the unnumbered H1 document title.
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
/// skipped heading ancestors are treated as 1. Separate list blocks restart at 1,
/// regardless of the source marker. Beyond named Chinese levels, decimal paths
/// provide unambiguous numbering without a nesting limit.
pub fn render_numbered(markdown: &str, style: NumberingStyle) -> String {
    let arena = Arena::new();
    let root = parse::parse(&arena, markdown);
    let mut renderer = Renderer {
        style,
        headings: vec![],
        heading_level: 0,
    };
    renderer.blocks(root)
}

struct Renderer {
    style: NumberingStyle,
    headings: Vec<usize>,
    heading_level: usize,
}

impl Renderer {
    fn blocks<'a>(&mut self, node: &'a AstNode<'a>) -> String {
        let mut out = String::new();
        let mut previous_heading = false;
        for child in node.children() {
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
                self.heading_level = usize::from(h.level).saturating_sub(1);
                if self.heading_level == 0 {
                    self.headings.clear();
                    return inline(node);
                }
                let level = self.heading_level;
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
            NodeValue::List(_) => self.list(node, &[]),
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

    fn list<'a>(&mut self, node: &'a AstNode<'a>, path: &[usize]) -> String {
        node.children()
            .enumerate()
            .map(|(index, item)| {
                let mut item_path = path.to_vec();
                item_path.push(index + 1);
                let mut full_path = self.headings.clone();
                full_path.extend_from_slice(&item_path);
                let indent = "    ".repeat(path.len());
                let mut out = format!("{indent}{}", marker(self.style, &full_path));
                let mut first = true;
                for child in item.children() {
                    if matches!(child.data.borrow().value, NodeValue::List(_)) {
                        out.push('\n');
                        out.push_str(&self.list(child, &item_path));
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

fn marker(style: NumberingStyle, path: &[usize]) -> String {
    let n = *path.last().unwrap_or(&1);
    match (style, path.len()) {
        (NumberingStyle::Chinese | NumberingStyle::ChineseCompact, 1) => {
            format!("{}、", chinese(n))
        }
        (NumberingStyle::Chinese, 2) => format!("（{}）", chinese(n)),
        (NumberingStyle::Chinese, 3) | (NumberingStyle::ChineseCompact, 2) => format!("{n}. "),
        (NumberingStyle::Chinese, 4) => format!("（{n}）"),
        (NumberingStyle::ChineseCompact, 3) => format!("({n}) "),
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
    let mut out = String::new();
    for child in node.children() {
        let text = inline_raw(child);
        // Match the preview's Chinese emphasis boundary without touching code
        // literals, ordinary prose spaces, or spaces around Latin words.
        if matches!(
            child.data.borrow().value,
            NodeValue::Strong | NodeValue::Emph
        ) && out.ends_with(' ')
            && out.trim_end_matches(' ').chars().last().is_some_and(is_cjk)
            && text.chars().next().is_some_and(is_cjk)
        {
            out.truncate(out.trim_end_matches(' ').len());
        }
        out.push_str(&text);
    }
    out
}

fn is_cjk(ch: char) -> bool {
    matches!(ch, '\u{3400}'..='\u{4dbf}' | '\u{4e00}'..='\u{9fff}' | '\u{20000}'..='\u{3134f}')
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
