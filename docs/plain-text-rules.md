# Plain-text rendering rules

This document describes the **current implementation**, not every option proposed in [design §7](design.md#7-纯文本渲染规则). Rules are fixed: heading styles, bullet styles, table thresholds, and quote styles are not configurable. Sources of truth are `crates/cleancopy-core/src/render_plain.rs`, `parse.rs`, and `width.rs`, with assertions in `crates/cleancopy-core/tests/rules.rs` and golden snapshots in `tests/snapshots/`.

## Parsing and output boundaries

- Parsing uses comrak with tables, task lists, strikethrough, autolinks, and footnotes enabled.
- Nonempty sibling blocks are separated by one blank line. This does not collapse literal blank lines inside code blocks.
- Soft and hard Markdown line breaks both become newlines; prose is not reflowed.
- Nonempty rendered output ends in a newline. Windows uses CRLF; other platforms use LF. Code whitespace is retained apart from this final platform line-ending conversion.
- Empty/whitespace-only or comment-only input can render to an empty string. The CLI rejects empty/whitespace-only **input** with exit code 1; nonempty Markdown that renders empty is not the same case.
- This is Markdown-to-readable-text rendering, not sanitization or an HTML-to-text browser engine. Links and text can still contain untrusted content. Inspect pasted shell commands before executing them.

## Headings and inline elements

| Element | Output |
| --- | --- |
| H1 | Bare heading text |
| H2–H6 | `【Heading text】` |
| Bold, emphasis, strikethrough | Contents, without formatting markers |
| Inline code | Literal text without backticks |
| Horizontal rule | `────────` (eight box-drawing characters) |
| Link | `label (URL)`; only one copy if rendered label equals URL exactly |
| Image | `[图片: alt] (URL)`; this label is fixed even for English input |

Link/image titles are omitted. Autolinks whose labels equal their destinations appear as just the URL. Unlike the HTML renderer, plain-text rendering does not filter dangerous URL schemes. Removing strikethrough markers preserves deleted words, which can change how a reader interprets them.

## Lists and tasks

Both ordered and unordered lists are numbered:

- First level: `1.`, `2.`, etc.
- Second level: hierarchical markers such as `1.1`, with **no trailing dot**.
- Third level and deeper: `-`.
- Each nesting level adds four ordinary spaces.
- Ordered-list start values are respected at each level. Unordered lists start at 1. Independent lists restart their numbering.
- Continuation lines and subsequent non-list blocks in an item receive four additional spaces.
- Tight items use single newlines; loose items retain blank-line separation.
- Tasks use `☐` or `☑` **instead of** numeric markers, including nested tasks.

Example:

```text
1. Fruit
    1.1 Apple
        - Red
2. Bread

☐ Buy milk
☑ Make tea
```

## Quotes

Each rendered line, including blank separator lines, is prefixed with `> `. Nested block quotes accumulate prefixes. Quotation-mark alternatives proposed in the design are not implemented.

## Code blocks

Code is enclosed by `────────` lines. If the fence has an info string, its first whitespace-separated word becomes `[language]`. No info string means no language label. Code is not highlighted.

```text
────────
[rust]
fn main() {
    println!("hello");
}
────────
```

Literal indentation, trailing spaces, and blank lines in the parsed code content are retained. A newline is inserted before the closing separator if necessary. Fence metadata after the first word is not printed. Platform-native newline conversion still applies; this is not byte-for-byte preservation of the source file.

## Tables

Cells use inline plain-text rendering; embedded newlines are replaced with spaces. Column width is the maximum Unicode display width of header and body values, with a minimum of two per column. CJK characters count as two columns according to `unicode-width`.

The total width is:

```text
sum(column widths) + 2 × (number of columns − 1)
```

**At most 60 columns:** left-aligned columns separated by two spaces, with a `─` header-separator row. The last column is not padded. Markdown alignment directives are ignored.

```text
Name  Count
────  ─────
Rust  2
Go    3
```

**More than 60 columns:** each body row becomes a numbered record:

```text
1. Name：Rust；Description：A long description
2. Name：Go；Description：Another description
```

Record labels use full-width `：` and `；`; numbering starts at 1. A wide header-only table has no body records and therefore renders nothing. There is no option to force record mode or change the threshold. Display-width alignment does not guarantee visual alignment in proportional fonts, including many chat interfaces.

## Footnotes

References become `[1]`, `[2]`, etc., numbered by **first reference order**, not definition order or label. Referenced definitions are appended once, in number order, separated by blank lines:

```text
A claim.[1]

[1] Supporting detail.
```

Definition bodies retain block structure. Continuation blocks are not indented beneath the `[n]` prefix. Unreferenced definitions are omitted; undefined references remain literal Markdown text.

## Raw HTML limitations

Tags are removed while textual contents remain. Bare `<br>` / `<br/>` variants are recognized case-insensitively (with whitespace allowed) as newlines. A `<br>` with attributes is not recognized as a break. Other tags do not add spacing or paragraph boundaries; HTML-block results are trimmed.

The tag stripper is quote-aware but does not itself decode HTML entities or remove script/style contents. For example, stripping tags does not make arbitrary embedded HTML safe or meaningful as text. The HTML renderer has a different policy: it escapes raw HTML rather than using this stripping behavior.

## Other representations and planned options

These rules apply to `--print plain`, `--plain`, and the plain fallback of multi/HTML clipboard modes. They do **not** describe `--markdown`: that mode preserves Markdown syntax while removing terminal escapes/control sequences, trimming edges, and normalizing newlines. Multi-mode plain-text paste is rendered text, not Markdown source; use `--markdown` when source is wanted.

Configurable heading/quote/bullet styles, forced table modes, `--normalize`, syntax highlighting, Markdown detection/watch mode, and app-specific rendering rules remain planned, not supported switches. See [README](../README.md) for modes and [compatibility](compat.md) for the distinction between backend readback and application paste testing.
