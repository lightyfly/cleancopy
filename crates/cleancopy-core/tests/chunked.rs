//! Large documents are parsed one chunk of top-level blocks at a time to
//! bound memory (parse::chunks). Chunked output must be byte-identical to
//! whole-document output for every renderer.

use cleancopy_core::{
    parse::{chunks, with_chunk_sizes},
    render_html, render_numbered, render_plain, render_rtf, render_word_html, NumberingStyle,
};

const DOC: &str = "# 标题\n\n导语 **粗体** 与 _斜体_。\n\n## 第一节\n\n- a\n- b\n\n  续段\n\n- c\n\n段落一\n\n1. one\n2. two\n\n后续\n\n3. three\n\n### 小节\n\n```rust\nfn main() {\n\n    // blank line inside fence\n}\n```\n\n> 引用\n\n> 另一个\n\n| a | b |\n|---|:-:|\n| 1 | 2 |\n\n---\n\n#### 四级\n\n- [x] done\n- [ ] todo\n\n## 第二节\n\n文字 `code` [链接](https://example.com)\n\nSetext\n======\n\n1) paren\n2) list\n\n    indented code\n\n结尾\n";

fn render_all(input: &str) -> Vec<String> {
    let mut out = vec![
        render_plain(input),
        render_html(input),
        render_word_html(input),
        render_rtf(input),
    ];
    for style in [
        NumberingStyle::Chinese,
        NumberingStyle::ChineseCompact,
        NumberingStyle::Decimal,
    ] {
        out.push(render_numbered(input, style));
    }
    out
}

fn assert_same(input: &str) {
    let whole = with_chunk_sizes(usize::MAX, usize::MAX, || render_all(input));
    let chunked = with_chunk_sizes(0, 1, || {
        assert!(chunks(input).len() > 1, "test input must actually split");
        render_all(input)
    });
    for (w, c) in whole.iter().zip(&chunked) {
        assert_eq!(w, c);
    }
}

#[test]
fn chunked_rendering_matches_whole_document() {
    assert_same(DOC);
    assert_same(&DOC.repeat(3));
    // No single leading H1: the title rule must still see the whole document.
    assert_same(&format!("{DOC}\n# 第二个一级\n\n正文\n"));
    assert_same(&DOC.replacen("# 标题\n\n", "", 1));
}

#[test]
fn fixtures_render_identically_when_chunked() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures");
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "md") {
            let text = std::fs::read_to_string(&path).unwrap();
            let whole = with_chunk_sizes(usize::MAX, usize::MAX, || render_all(&text));
            let chunked = with_chunk_sizes(0, 1, || render_all(&text));
            assert_eq!(whole, chunked, "{}", path.display());
        }
    }
}

#[test]
fn splits_only_at_safe_block_boundaries() {
    let parts = with_chunk_sizes(0, 1, || chunks(DOC));
    assert_eq!(parts.concat(), DOC);
    for part in &parts[1..] {
        let list = ["- ", "1. ", "2. ", "3. ", "1) ", "2) "];
        assert!(!part.starts_with(' '), "{part:?}");
        assert!(!list.iter().any(|m| part.starts_with(m)), "{part:?}");
    }
    assert!(!parts.iter().any(|p| p.starts_with("    // blank")));
}

#[test]
fn document_wide_constructs_disable_chunking() {
    for doc in [
        "a\n\nb [x]\n\n[x]: https://example.com\n",
        "a\n\nb[^1]\n\n[^1]: note\n",
        "a\n\n<div>\n\nraw\n\n</div>\n",
        "- a\n\n      ```\n\n      x\n      ```\n\nb\n",
    ] {
        assert_eq!(with_chunk_sizes(0, 1, || chunks(doc)), vec![doc]);
    }
    // Small documents are never split with the default thresholds.
    assert_eq!(chunks(DOC), vec![DOC]);
}
