use crate::parse;
use comrak::{
    nodes::{AstNode, ListDelimType, ListType, NodeList, NodeValue, TableAlignment},
    Arena,
};
use std::{collections::HashMap, fmt::Write};

/// Render Markdown as a self-contained, ASCII-only RTF document.
///
/// Lists have native numbering metadata, tables remain editable, and headings
/// carry Word's built-in style IDs and outline levels. Links and images retain
/// their labels and destinations as inert text, never executable RTF fields.
/// Raw HTML is likewise shown literally. Unicode uses signed UTF-16 units with
/// one fallback character per unit, including both halves of surrogate pairs.
pub fn render_rtf(input: &str) -> String {
    let arena = Arena::new();
    let root = parse::parse(&arena, input);
    let mut out = String::from(
        "{\\rtf1\\ansi\\ansicpg1252\\deff0\\uc1\n\
         {\\fonttbl{\\f0\\fswiss Arial;}{\\f1\\fmodern Courier New;}}\n\
         {\\colortbl;\\red245\\green245\\blue245;}\n\
         {\\stylesheet\
         {\\s0\\f0\\fs22 Normal;}\
         {\\s1\\styrsid1\\sbasedon0\\snext0\\keepn\\b\\fs36\\outlinelevel0 Heading 1;}\
         {\\s2\\styrsid2\\sbasedon0\\snext0\\keepn\\b\\fs30\\outlinelevel1 Heading 2;}\
         {\\s3\\styrsid3\\sbasedon0\\snext0\\keepn\\b\\fs26\\outlinelevel2 Heading 3;}}\n",
    );
    let lists: Vec<_> = root
        .descendants()
        .filter(|n| matches!(n.data.borrow().value, NodeValue::List(_)))
        .collect();
    let mut ids = HashMap::new();
    if !lists.is_empty() {
        out.push_str("{\\*\\listtable\n");
        for (index, node) in lists.iter().enumerate() {
            if crate::cancel::cancelled() {
                break;
            }
            let id = index + 1;
            ids.insert(*node as *const AstNode<'_> as usize, id);
            let mut levels: Vec<NodeList> = node
                .ancestors()
                .filter_map(|n| match n.data.borrow().value {
                    NodeValue::List(list) => Some(list),
                    _ => None,
                })
                .collect();
            levels.reverse();
            let current = *levels.last().expect("list includes itself");
            write!(out, "{{\\list\\listtemplateid{id}\\listhybrid").unwrap();
            // RTF supports nine levels. Deeper Markdown is retained at level 8.
            for level in 0..9 {
                let list = if level == 8 && levels.len() > 9 {
                    current
                } else {
                    levels.get(level).copied().unwrap_or(current)
                };
                let ordered = list.list_type == ListType::Ordered;
                let start = if ordered { list.start } else { 1 };
                let nfc = if ordered { 0 } else { 23 };
                write!(out, "{{\\listlevel\\levelnfc{nfc}\\levelnfcn{nfc}\\leveljc0\\leveljcn0\\levelfollow0\\levelstartat{start}").unwrap();
                if ordered {
                    let delimiter = if list.delimiter == ListDelimType::Paren {
                        ')'
                    } else {
                        '.'
                    };
                    write!(
                        out,
                        "{{\\leveltext\\'02\\'0{level}{delimiter};}}{{\\levelnumbers\\'01;}}"
                    )
                    .unwrap();
                } else {
                    out.push_str("{\\leveltext\\'01\\u8226?;}{\\levelnumbers;}");
                }
                let indent = (level + 1) * 720;
                write!(out, "\\f0\\fi-360\\li{indent}\\jclisttab\\tx{indent}}}").unwrap();
            }
            writeln!(out, "\\listid{id}}}").unwrap();
        }
        out.push_str("}\n{\\*\\listoverridetable");
        // A distinct list/override per AST list prevents unrelated lists and
        // repeated nested lists from accidentally sharing counters.
        for id in 1..=lists.len() {
            if crate::cancel::cancelled() {
                break;
            }
            write!(
                out,
                "{{\\listoverride\\listid{id}\\listoverridecount0\\ls{id}}}"
            )
            .unwrap();
        }
        out.push_str("}\n");
    }
    out.push_str("\\viewkind4\\paperw12240\\paperh15840\\margl1440\\margr1440\n");
    let mut renderer = Renderer { out, ids };
    renderer.block(root, Context::default());
    renderer.out.push_str("}\n");
    renderer.out
}

#[derive(Clone, Copy)]
struct Marker {
    id: usize,
    level: usize,
    number: usize,
    list: NodeList,
}

#[derive(Clone, Copy, Default)]
struct Context {
    indent: usize,
    marker: Option<Marker>,
    checked: Option<bool>,
}

struct Renderer {
    out: String,
    ids: HashMap<usize, usize>,
}

impl Renderer {
    fn paragraph(&mut self, ctx: Context, style: &str) {
        write!(
            self.out,
            "{{\\pard\\plain\\s0\\f0\\fs22\\li{}\\sa120{style}",
            ctx.indent
        )
        .unwrap();
        if let Some(marker) = ctx.marker {
            write!(
                self.out,
                "\\ls{}\\ilvl{}\\fi-360\\tx{}",
                marker.id, marker.level, ctx.indent
            )
            .unwrap();
            self.out.push_str("{\\listtext\\f0 ");
            if marker.list.list_type == ListType::Ordered {
                let delimiter = if marker.list.delimiter == ListDelimType::Paren {
                    ')'
                } else {
                    '.'
                };
                write!(self.out, "{}{delimiter}", marker.number).unwrap();
            } else {
                self.out.push_str("\\u8226?");
            }
            self.out.push_str("\\tab}");
        }
        self.out.push(' ');
        if let Some(checked) = ctx.checked {
            text(&mut self.out, if checked { "☑ " } else { "☐ " });
        }
    }

    fn children<'a>(&mut self, node: &'a AstNode<'a>) {
        for child in node.children() {
            self.inline(child);
        }
    }

    fn block<'a>(&mut self, node: &'a AstNode<'a>, ctx: Context) {
        if crate::cancel::cancelled() {
            return;
        }
        match &node.data.borrow().value {
            NodeValue::Paragraph | NodeValue::Heading(_) => {
                let style = match &node.data.borrow().value {
                    NodeValue::Heading(h) => match h.level {
                        1 => "\\s1\\b\\fs36\\keepn\\outlinelevel0",
                        2 => "\\s2\\b\\fs30\\keepn\\outlinelevel1",
                        3 => "\\s3\\b\\fs26\\keepn\\outlinelevel2",
                        _ => "\\b\\fs24\\keepn",
                    },
                    _ => "",
                };
                self.paragraph(ctx, style);
                self.children(node);
                self.out.push_str("\\par}\n");
            }
            NodeValue::CodeBlock(code) => {
                self.paragraph(ctx, "\\f1\\fs20\\cb1\\highlight1\\shading10000\\cbpat1");
                // The parser adds a terminal newline; the paragraph already
                // supplies it, so do not manufacture an extra blank code line.
                text(
                    &mut self.out,
                    code.literal.strip_suffix('\n').unwrap_or(&code.literal),
                );
                self.out.push_str("\\par}\n");
            }
            NodeValue::HtmlBlock(html) => {
                self.paragraph(ctx, "");
                text(&mut self.out, html.literal.trim_end_matches('\n'));
                self.out.push_str("\\par}\n");
            }
            NodeValue::List(list) => {
                let id = self.ids[&(node as *const AstNode<'_> as usize)];
                let depth = node
                    .ancestors()
                    .filter(|n| matches!(n.data.borrow().value, NodeValue::List(_)))
                    .count()
                    - 1;
                for (index, item) in node.children().enumerate() {
                    let checked = match item.data.borrow().value {
                        NodeValue::TaskItem(value) => Some(value.is_some()),
                        _ => None,
                    };
                    let mut item_ctx = Context {
                        indent: ctx.indent + 720,
                        marker: Some(Marker {
                            id,
                            level: depth.min(8),
                            number: list.start.saturating_add(index),
                            list: *list,
                        }),
                        checked,
                    };
                    let mut children = item.children().peekable();
                    if !children.peek().is_some_and(|n| {
                        matches!(
                            n.data.borrow().value,
                            NodeValue::Paragraph
                                | NodeValue::Heading(_)
                                | NodeValue::CodeBlock(_)
                                | NodeValue::HtmlBlock(_)
                        )
                    }) {
                        self.paragraph(item_ctx, "");
                        self.out.push_str("\\par}\n");
                        item_ctx.marker = None;
                        item_ctx.checked = None;
                    }
                    for child in children {
                        self.block(child, item_ctx);
                        item_ctx.marker = None;
                        item_ctx.checked = None;
                    }
                }
            }
            NodeValue::Table(table) => {
                let columns = table.num_columns.max(1);
                let width = 9360usize.saturating_sub(ctx.indent).max(columns);
                for row in node.children() {
                    let header = matches!(row.data.borrow().value, NodeValue::TableRow(true));
                    write!(self.out, "{{\\trowd\\trgaph108\\trleft{}", ctx.indent).unwrap();
                    if header {
                        self.out.push_str("\\trhdr");
                    }
                    for column in 1..=columns {
                        self.out.push_str("\\clbrdrt\\brdrs\\brdrw10\\clbrdrb\\brdrs\\brdrw10\\clbrdrl\\brdrs\\brdrw10\\clbrdrr\\brdrs\\brdrw10");
                        write!(self.out, "\\cellx{}", ctx.indent + width * column / columns)
                            .unwrap();
                    }
                    self.out.push('\n');
                    for (column, cell) in row.children().enumerate() {
                        self.out.push_str("{\\pard\\plain\\intbl\\f0\\fs22");
                        self.out.push_str(match table.alignments.get(column) {
                            Some(TableAlignment::Center) => "\\qc",
                            Some(TableAlignment::Right) => "\\qr",
                            _ => "\\ql",
                        });
                        if header {
                            self.out.push_str("\\b");
                        }
                        self.out.push(' ');
                        self.children(cell);
                        self.out.push_str("\\cell}");
                    }
                    self.out.push_str("\\row}\n");
                }
            }
            NodeValue::BlockQuote | NodeValue::MultilineBlockQuote(_) => {
                for child in node.children() {
                    self.block(
                        child,
                        Context {
                            indent: ctx.indent + 360,
                            ..ctx
                        },
                    );
                }
            }
            NodeValue::ThematicBreak => {
                self.paragraph(ctx, "\\brdrb\\brdrs\\brdrw10");
                self.out.push_str("\\par}\n");
            }
            NodeValue::FootnoteDefinition(note) => {
                self.paragraph(ctx, "");
                text(&mut self.out, &format!("[^{}]", note.name));
                self.out.push_str("\\par}\n");
                for child in node.children() {
                    self.block(child, ctx);
                }
            }
            _ => {
                for child in node.children() {
                    self.block(child, ctx);
                }
            }
        }
    }

    fn inline<'a>(&mut self, node: &'a AstNode<'a>) {
        match &node.data.borrow().value {
            NodeValue::Text(value)
            | NodeValue::HtmlInline(value)
            | NodeValue::Raw(value)
            | NodeValue::EscapedTag(value) => text(&mut self.out, value),
            NodeValue::SoftBreak => self.out.push(' '),
            NodeValue::LineBreak => self.out.push_str("\\line "),
            NodeValue::Code(code) => {
                self.out.push_str("{\\f1\\fs20\\cb1\\highlight1 ");
                text(&mut self.out, &code.literal);
                self.out.push('}');
            }
            NodeValue::Strong
            | NodeValue::Emph
            | NodeValue::Strikethrough
            | NodeValue::Superscript
            | NodeValue::Subscript
            | NodeValue::Underline => {
                self.out.push_str(match node.data.borrow().value {
                    NodeValue::Strong => "{\\b ",
                    NodeValue::Emph => "{\\i ",
                    NodeValue::Strikethrough => "{\\strike ",
                    NodeValue::Superscript => "{\\super ",
                    NodeValue::Subscript => "{\\sub ",
                    _ => "{\\ul ",
                });
                self.children(node);
                self.out.push('}');
            }
            NodeValue::Link(link) | NodeValue::Image(link) => {
                self.children(node);
                if !link.url.is_empty() {
                    text(&mut self.out, &format!(" ({})", link.url));
                }
                if !link.title.is_empty() {
                    text(&mut self.out, &format!(" \"{}\"", link.title));
                }
            }
            NodeValue::FootnoteReference(note) => text(&mut self.out, &format!("[^{}]", note.name)),
            NodeValue::Math(math) => text(&mut self.out, &math.literal),
            _ => self.children(node),
        }
    }
}

fn text(out: &mut String, value: &str) {
    let normalized = value.replace("\r\n", "\n").replace('\r', "\n");
    for unit in normalized.encode_utf16() {
        match unit {
            0x5c => out.push_str("\\\\"),
            0x7b => out.push_str("\\{"),
            0x7d => out.push_str("\\}"),
            0x0a => out.push_str("\\line "),
            0x09 => out.push_str("\\tab "),
            0x20..=0x7e => out.push(char::from_u32(u32::from(unit)).unwrap()),
            _ => write!(out, "\\u{}?", unit as i16).unwrap(),
        }
    }
}
