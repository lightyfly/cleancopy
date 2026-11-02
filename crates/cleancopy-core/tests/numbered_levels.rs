//! Property test (report v4 M-1): across random outlines of H1–H6 headings,
//! paragraphs and nested lists, no outline level ever uses the same numbering
//! form as the level directly above it, and H4–H6 are never numbered in the
//! Chinese styles.

use cleancopy_core::{render_numbered, NumberingStyle};
use proptest::prelude::*;

#[derive(Clone, Debug)]
enum Block {
    Heading(u8),
    Paragraph,
    /// Nesting depth of each item; each depth is at most previous + 1.
    List(Vec<usize>),
}

fn block() -> impl Strategy<Value = Block> {
    prop_oneof![
        (1u8..=6).prop_map(Block::Heading),
        Just(Block::Paragraph),
        prop::collection::vec(0usize..4, 1..6).prop_map(|raw| {
            let mut depths = Vec::new();
            let mut prev = 0usize;
            for (i, d) in raw.into_iter().enumerate() {
                let d = if i == 0 { 0 } else { d.min(prev + 1) };
                depths.push(d);
                prev = d;
            }
            Block::List(depths)
        }),
    ]
}

#[derive(Debug, PartialEq, Clone, Copy)]
enum Form {
    ChineseComma, // 一、
    ChineseParen, // （一）
    Decimal,      // 1.
    Paren,        // （1）
    Circled,      // ①
    Alpha,        // a.
    Bullet,       // ·
    Path(usize),  // 1.1 / 1.1.1 (segment count)
    None,
}

fn form(line: &str) -> Form {
    let t = line.trim_start();
    let first = t.chars().next().unwrap_or(' ');
    if t.starts_with("· ") {
        return Form::Bullet;
    }
    if ('\u{2460}'..='\u{2473}').contains(&first) || ('\u{3251}'..='\u{32bf}').contains(&first) {
        return Form::Circled;
    }
    if let Some(rest) = t.strip_prefix('（') {
        let inner: String = rest.chars().take_while(|c| *c != '）').collect();
        if inner.chars().all(|c| c.is_ascii_digit()) && !inner.is_empty() {
            return Form::Paren;
        }
        if !inner.is_empty() {
            return Form::ChineseParen;
        }
    }
    let cn: String = t
        .chars()
        .take_while(|c| "零一二三四五六七八九十百千".contains(*c))
        .collect();
    if !cn.is_empty() && t[cn.len()..].starts_with('、') {
        return Form::ChineseComma;
    }
    let head: String = t.chars().take_while(|c| !c.is_whitespace()).collect();
    if head.contains('.') && head.trim_end_matches('.').contains('.') {
        return Form::Path(head.trim_end_matches('.').split('.').count());
    }
    if head.ends_with('.') {
        let n = head.trim_end_matches('.');
        if n.chars().all(|c| c.is_ascii_digit()) && !n.is_empty() {
            return Form::Decimal;
        }
        if n.chars().all(|c| c.is_ascii_lowercase()) && !n.is_empty() {
            return Form::Alpha;
        }
    }
    Form::None
}

fn check(blocks: &[Block], style: NumberingStyle) -> Result<(), TestCaseError> {
    let mut md = String::new();
    // (outline level, is heading H4+, kind) per rendered line.
    let mut expected: Vec<(Option<usize>, bool, bool)> = Vec::new();
    let mut deepest = 0usize; // deepest numbered heading level in the chain
    for b in blocks {
        match b {
            Block::Heading(h) => {
                md.push_str(&format!("{} h\n\n", "#".repeat(usize::from(*h))));
                let level = match style {
                    NumberingStyle::Decimal => usize::from(*h) - 1,
                    _ if *h > 3 => {
                        expected.push((None, true, true));
                        continue;
                    }
                    _ => usize::from(*h),
                };
                deepest = level;
                expected.push(((level > 0).then_some(level), false, true));
            }
            Block::Paragraph => {
                md.push_str("p\n\n");
                expected.push((None, false, false));
            }
            Block::List(depths) => {
                for d in depths {
                    md.push_str(&format!("{}- x\n", "  ".repeat(*d)));
                    expected.push((Some(deepest + 1 + d), false, false));
                }
                md.push_str("\n<!-- end -->\n\n");
            }
        }
    }
    let out = render_numbered(&md, style);
    let lines: Vec<&str> = out.lines().filter(|l| !l.trim().is_empty()).collect();
    prop_assert_eq!(lines.len(), expected.len(), "{}\n---\n{}", md, out);
    let mut at_level: Vec<Option<(Form, bool)>> = Vec::new();
    for (line, (level, deep_heading, heading)) in lines.iter().zip(&expected) {
        let f = form(line);
        if *deep_heading {
            prop_assert_eq!(f, Form::None, "H4+ numbered: {:?}\n{}", line, out);
            continue;
        }
        let Some(k) = *level else { continue };
        if *heading {
            // A heading's parent is the enclosing heading, never a list that
            // happened to precede it (design.md §7.1 rule 6).
            at_level.truncate(k);
            for slot in &mut at_level {
                if slot.is_some_and(|(_, h)| !h) {
                    *slot = None;
                }
            }
        }
        prop_assert_ne!(f, Form::None, "unnumbered: {:?}\n{}", line, out);
        // `· ` bullets below the four-rung ladder are not a numbering form.
        if k >= 2 && f != Form::Bullet {
            if let Some(Some((parent, _))) = at_level.get(k - 1) {
                prop_assert_ne!(
                    f,
                    *parent,
                    "level {} repeats its parent: {:?}\n{}",
                    k,
                    line,
                    out
                );
            }
        }
        if style != NumberingStyle::Decimal {
            prop_assert!(
                !matches!(f, Form::Path(_)),
                "path numbering: {:?}\n{}",
                line,
                out
            );
        }
        at_level.resize(k + 1, None);
        at_level.truncate(k + 1);
        at_level[k] = Some((f, *heading));
    }
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(400))]
    #[test]
    fn adjacent_levels_never_share_a_numbering_form(
        blocks in prop::collection::vec(block(), 1..14)
    ) {
        for style in [NumberingStyle::Chinese, NumberingStyle::ChineseCompact, NumberingStyle::Decimal] {
            check(&blocks, style)?;
        }
    }
}

#[test]
fn report_v4_numbering_sample_has_no_repeats() {
    let md = "# 文档标题\n\n- 顶层A\n\n## 第一章\n\n- 甲\n  - 乙1\n\n### 小节一\n\n1. 步骤一\n2. 步骤二\n\n#### 细则一\n\n- 细项\n  - 细子项\n\n##### 子则\n\n- 深项\n\n### 小节二\n\n- 列表后紧跟小标题\n#### 紧跟的四级\n- 四级下列表\n";
    let out = render_numbered(md, NumberingStyle::Chinese);
    assert_eq!(
        out,
        "一、文档标题\n1. 顶层A\n\n（一）第一章\n1. 甲\n    （1）乙1\n\n1. 小节一\n（1）步骤一\n（2）步骤二\n\n细则一\n（1）细项\n    ①细子项\n\n子则\n（1）深项\n\n2. 小节二\n（1）列表后紧跟小标题\n\n紧跟的四级\n（1）四级下列表"
    );
    let compact = render_numbered(md, NumberingStyle::ChineseCompact);
    assert!(!compact.contains("1.1.1"), "{compact}");
    assert!(!compact.contains("(1)"), "half-width paren: {compact}");
}
