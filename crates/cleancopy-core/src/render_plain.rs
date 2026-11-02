use crate::{
    parse,
    width::{display_width, pad_right},
};
use comrak::{
    nodes::{AstNode, ListType, NodeValue},
    Arena,
};

const RULE: &str = "────────";

pub fn render_plain(input: &str) -> String {
    let arena = Arena::new();
    let root = parse::parse(&arena, input);
    let mut output = blocks(root, &[]);
    let mut notes = Vec::new();
    for node in root.descendants() {
        if let NodeValue::FootnoteReference(reference) = &node.data.borrow().value {
            if !notes.iter().any(|(name, _)| name == &reference.name) {
                notes.push((reference.name.clone(), reference.ix));
            }
        }
    }
    notes.sort_by_key(|(_, ix)| *ix);
    for (name, ix) in notes {
        if let Some(definition) = root.children().find(|node| {
            matches!(&node.data.borrow().value, NodeValue::FootnoteDefinition(d) if d.name == name)
        }) {
            let text = blocks(definition, &[]);
            if !output.is_empty() { output.push_str("\n\n"); }
            output.push_str(&format!("[{ix}] {text}"));
        }
    }
    if !output.is_empty() && !output.ends_with('\n') {
        output.push('\n');
    }
    crate::native_newlines(&output)
}

fn inlines<'a>(node: &'a AstNode<'a>) -> String {
    node.children().map(inline).collect()
}

fn inline<'a>(node: &'a AstNode<'a>) -> String {
    match &node.data.borrow().value {
        NodeValue::Text(text) => text.clone(),
        NodeValue::Code(code) => code.literal.clone(),
        NodeValue::SoftBreak | NodeValue::LineBreak => "\n".into(),
        NodeValue::HtmlInline(html) => strip_tags(html),
        NodeValue::Link(link) => {
            let text = inlines(node);
            if text == link.url {
                text
            } else {
                format!("{text} ({})", link.url)
            }
        }
        NodeValue::Image(link) => format!("[图片: {}] ({})", inlines(node), link.url),
        NodeValue::FootnoteReference(reference) => format!("[{}]", reference.ix),
        _ => inlines(node),
    }
}

fn blocks<'a>(node: &'a AstNode<'a>, path: &[usize]) -> String {
    node.children()
        .take_while(|_| !crate::cancel::cancelled())
        .map(|child| block(child, path))
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn block<'a>(node: &'a AstNode<'a>, path: &[usize]) -> String {
    match &node.data.borrow().value {
        NodeValue::Paragraph => inlines(node),
        NodeValue::Heading(heading) => {
            let text = inlines(node);
            if heading.level == 1 {
                text
            } else {
                format!("【{text}】")
            }
        }
        NodeValue::ThematicBreak => RULE.into(),
        NodeValue::CodeBlock(code) => {
            let language = code.info.split_whitespace().next().unwrap_or("");
            let label = if language.is_empty() {
                String::new()
            } else {
                format!("[{language}]\n")
            };
            let mut text = format!("{RULE}\n{label}{}", code.literal);
            if !text.ends_with('\n') {
                text.push('\n');
            }
            text.push_str(RULE);
            text
        }
        NodeValue::BlockQuote => blocks(node, path)
            .split('\n')
            .map(|line| format!("> {line}"))
            .collect::<Vec<_>>()
            .join("\n"),
        NodeValue::List(list) => {
            let start = if list.list_type == ListType::Ordered {
                list.start
            } else {
                1
            };
            node.children()
                .enumerate()
                .map(|(index, item)| {
                    let mut item_path = path.to_vec();
                    item_path.push(start + index);
                    list_item(item, &item_path)
                })
                .collect::<Vec<_>>()
                .join(if list.tight { "\n" } else { "\n\n" })
        }
        NodeValue::Table(_) => table(node),
        NodeValue::FootnoteDefinition(_) => String::new(),
        NodeValue::HtmlBlock(html) => strip_tags(&html.literal).trim().to_owned(),
        _ => blocks(node, path),
    }
}

fn list_item<'a>(item: &'a AstNode<'a>, path: &[usize]) -> String {
    let indent = "    ".repeat(path.len() - 1);
    let marker = match &item.data.borrow().value {
        NodeValue::TaskItem(checked) => {
            if checked.is_some() {
                "☑".into()
            } else {
                "☐".into()
            }
        }
        _ if path.len() >= 3 => "-".into(),
        _ if path.len() == 1 => format!("{}.", path[0]),
        _ => path
            .iter()
            .map(usize::to_string)
            .collect::<Vec<_>>()
            .join("."),
    };
    let mut result = String::new();
    let mut first = true;
    for child in item.children() {
        let text = block(child, path);
        if text.is_empty() {
            continue;
        }
        if matches!(child.data.borrow().value, NodeValue::List(_)) {
            if !result.is_empty() {
                result.push('\n');
            }
            result.push_str(&text);
        } else {
            if !first {
                result.push_str("\n\n");
            }
            for (line_index, line) in text.split('\n').enumerate() {
                if line_index > 0 {
                    result.push('\n');
                }
                if first && line_index == 0 {
                    result.push_str(&format!("{indent}{marker} {line}"));
                } else {
                    result.push_str(&format!("{indent}    {line}"));
                }
            }
        }
        first = false;
    }
    if first {
        format!("{indent}{marker}")
    } else {
        result
    }
}

fn table<'a>(node: &'a AstNode<'a>) -> String {
    let rows: Vec<Vec<String>> = node
        .children()
        .map(|row| {
            row.children()
                .map(|cell| inlines(cell).replace('\n', " "))
                .collect()
        })
        .collect();
    let Some(headers) = rows.first() else {
        return String::new();
    };
    let widths: Vec<usize> = (0..headers.len())
        .map(|col| {
            rows.iter()
                .map(|row| display_width(&row[col]))
                .max()
                .unwrap_or(0)
                .max(2)
        })
        .collect();
    let total = widths.iter().sum::<usize>() + widths.len().saturating_sub(1) * 2;
    if total > 60 {
        return rows
            .iter()
            .skip(1)
            .enumerate()
            .map(|(index, row)| {
                let fields = headers
                    .iter()
                    .zip(row)
                    .map(|(key, value)| format!("{key}：{value}"))
                    .collect::<Vec<_>>()
                    .join("；");
                format!("{}. {fields}", index + 1)
            })
            .collect::<Vec<_>>()
            .join("\n");
    }
    let mut lines = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        lines.push(
            row.iter()
                .zip(&widths)
                .enumerate()
                .map(|(col, (cell, width))| {
                    if col + 1 == widths.len() {
                        cell.clone()
                    } else {
                        pad_right(cell, *width)
                    }
                })
                .collect::<Vec<_>>()
                .join("  "),
        );
        if index == 0 {
            lines.push(
                widths
                    .iter()
                    .map(|width| "─".repeat(*width))
                    .collect::<Vec<_>>()
                    .join("  "),
            );
        }
    }
    lines.join("\n")
}

fn strip_tags(html: &str) -> String {
    let mut result = String::new();
    let mut tag = String::new();
    let mut inside = false;
    let mut quote = None;
    for character in html.chars() {
        if inside {
            if let Some(delimiter) = quote {
                if character == delimiter {
                    quote = None;
                }
            } else if character == '\'' || character == '"' {
                quote = Some(character);
            } else if character == '>' {
                inside = false;
                if tag
                    .trim()
                    .trim_end_matches('/')
                    .trim()
                    .eq_ignore_ascii_case("br")
                {
                    result.push('\n');
                }
            }
            tag.push(character);
        } else if character == '<' {
            inside = true;
            tag.clear();
        } else {
            result.push(character);
        }
    }
    result
}
