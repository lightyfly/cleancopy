use cleancopy_core::render_rtf;

fn balanced(rtf: &str) {
    let bytes = rtf.as_bytes();
    let mut depth = 0isize;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => {
                i += 1;
                if i < bytes.len() && matches!(bytes[i], b'\\' | b'{' | b'}') {
                    i += 1;
                    continue;
                }
            }
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                assert!(depth >= 0, "early group closure: {rtf}");
            }
            _ => {}
        }
        i += 1;
    }
    assert_eq!(depth, 0, "unbalanced RTF: {rtf}");
}

#[test]
fn complete_ascii_document_even_when_empty() {
    for input in ["", "plain text", "中文 😀", "{\\rtf1 injected}"] {
        let rtf = render_rtf(input);
        assert!(rtf.starts_with("{\\rtf1\\ansi\\ansicpg1252\\deff0\\uc1"));
        assert!(rtf.ends_with("}\n"));
        assert!(rtf.is_ascii());
        balanced(&rtf);
        assert_eq!(rtf, render_rtf(input));
    }
}

#[test]
fn built_in_heading_styles_and_scoped_inline_formatting() {
    let rtf = render_rtf(
        "# First\n\n## Second\n\n### Third\n\n#### Fourth\n\n**bold *nested*** normal ~~gone~~",
    );
    for (style, level, name) in [
        (1, 0, "Heading 1"),
        (2, 1, "Heading 2"),
        (3, 2, "Heading 3"),
    ] {
        assert!(rtf.contains(&format!("{{\\s{style}\\styrsid{style}\\sbasedon0\\snext0")));
        assert!(rtf.contains(&format!("\\outlinelevel{level} {name};")));
    }
    for heading in ["First", "Second", "Third", "Fourth"] {
        assert!(rtf.contains(&format!(" {heading}\\par}}")));
    }
    assert!(rtf.contains("{\\b bold {\\i nested}} normal {\\strike gone}"));
    assert!(rtf.contains("\\s1\\b\\fs36\\keepn\\outlinelevel0"));
    balanced(&rtf);
}

#[test]
fn unicode_is_signed_utf16_and_rtf_syntax_is_inert() {
    let rtf = render_rtf("```\n中文 😀 \\{\\b evil} \\cell\n\tend\u{1}\u{7f}\n```");
    assert!(rtf.contains("\\u20013?\\u25991?"));
    assert!(rtf.contains("\\u-10179?\\u-8704?"));
    assert!(rtf.contains("\\\\\\{\\\\b evil\\}"));
    assert!(rtf.contains("\\\\cell"));
    assert!(rtf.contains("\\tab end\\u1?\\u127?"));
    assert!(!rtf.contains('\u{1}'));
    balanced(&rtf);
}

#[test]
fn lists_have_native_metadata_nested_levels_starts_and_restarts() {
    let rtf = render_rtf("7. seven\n8. eight\n   - child\n\n     3. nested three\n     4. nested four\n9. nine\n\nseparator\n\n2) restart\n3) next");
    assert!(rtf.contains("{\\*\\listtable"));
    assert!(rtf.contains("{\\*\\listoverridetable"));
    assert!(rtf.contains("\\levelnfc0\\levelnfcn0"));
    assert!(rtf.contains("\\levelnfc23\\levelnfcn23"));
    for start in [7, 3, 2] {
        assert!(rtf.contains(&format!("\\levelstartat{start}")), "{rtf}");
    }
    for (id, level) in [(1, 0), (2, 1), (3, 2), (4, 0)] {
        assert!(rtf.contains(&format!(
            "\\listoverride\\listid{id}\\listoverridecount0\\ls{id}"
        )));
        assert!(rtf.contains(&format!("\\ls{id}\\ilvl{level}")), "{rtf}");
    }
    for number in ["7.", "8.", "9.", "3.", "4.", "2)", "3)"] {
        assert!(
            rtf.contains(&format!("{{\\listtext\\f0 {number}\\tab}}")),
            "missing {number}: {rtf}"
        );
    }
    balanced(&rtf);
}

#[test]
fn loose_list_continuations_are_not_new_items_and_nested_counters_restart() {
    let rtf =
        render_rtf("1. first\n\n   continuation\n\n   5. five\n\n2. second\n\n   5. five again\n");
    assert_eq!(rtf.matches("{\\listtext").count(), 4);
    assert_eq!(rtf.matches("{\\listtext\\f0 5.\\tab}").count(), 2);
    assert!(rtf.contains(" continuation\\par}"));
    assert!(rtf.contains("\\ls2\\ilvl1"));
    assert!(rtf.contains("\\ls3\\ilvl1"));
    balanced(&rtf);
}

#[test]
fn editable_tables_preserve_alignment_empty_cells_and_formatting() {
    let rtf = render_rtf(
        "| left | center | right |\n| :--- | :---: | ---: |\n| **bold** | | `code` |\n\nafter",
    );
    assert_eq!(rtf.matches("{\\trowd").count(), 2);
    assert_eq!(rtf.matches("\\cellx3120").count(), 2);
    assert_eq!(rtf.matches("\\cellx6240").count(), 2);
    assert_eq!(rtf.matches("\\cellx9360").count(), 2);
    assert_eq!(rtf.matches("\\cell}").count(), 6);
    assert_eq!(rtf.matches("\\row}").count(), 2);
    assert_eq!(rtf.matches("\\trhdr").count(), 1);
    assert!(rtf.contains("\\qc \\cell}"));
    assert!(rtf.contains("\\ql {\\b bold}\\cell}"));
    assert!(rtf.contains("\\qr {\\f1\\fs20\\cb1\\highlight1 code}"));
    assert!(rtf.contains("\\s0\\f0\\fs22\\li0\\sa120 after\\par}"));
    balanced(&rtf);
}

#[test]
fn code_has_gray_background_preserved_whitespace_and_normalized_newlines() {
    let input = "`inline` normal\r\n\r\n```rust\r\n  a\r\n\tb\r\n```";
    let rtf = render_rtf(input);
    assert!(rtf.contains("\\red245\\green245\\blue245"));
    assert!(rtf.contains("{\\f1\\fs20\\cb1\\highlight1 inline} normal"));
    assert!(rtf.contains("\\cbpat1   a\\line \\tab b\\par}"));
    assert!(!rtf.contains("rust"));
    assert!(!rtf.contains('\r'));
    balanced(&rtf);
}

#[test]
fn preserves_tasks_quotes_links_images_html_footnotes_and_breaks() {
    let rtf = render_rtf("- [ ] todo\n- [x] done\n\n> quoted **text**\n\n[a](https://example.com \"title\") ![alt](image.png)\n\n<span>raw</span>\n\nline  \nbreak\n\nnote[^n]\n\n[^n]: footnote\n\n---");
    for content in [
        "\\u9744? todo",
        "\\u9745? done",
        "quoted {\\b text}",
        "a (https://example.com) \"title\"",
        "alt (image.png)",
        "<span>raw</span>",
        "line\\line break",
        "[^n]",
        "footnote",
        "\\brdrb\\brdrs",
    ] {
        assert!(rtf.contains(content), "missing {content}: {rtf}");
    }
    assert!(!rtf.contains("\\field"));
    balanced(&rtf);
}

#[test]
fn deep_lists_are_retained_and_clamped_to_rtf_limit() {
    let input = (0..12)
        .map(|i| format!("{}- item{i}\n", "  ".repeat(i)))
        .collect::<String>();
    let rtf = render_rtf(&input);
    assert!(rtf.contains("\\ilvl8"));
    assert!(!rtf.contains("\\ilvl9"));
    for i in 0..12 {
        assert!(rtf.contains(&format!(" item{i}\\par}}")));
    }
    balanced(&rtf);
}
