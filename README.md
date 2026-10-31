# CleanCopy

Week-1 implementation of the [design](docs/design.md): a local Markdown-to-plain-text CLI and a lightly cleaned Markdown passthrough. Clipboard access is a stub; HTML rendering and later milestones are not implemented.

## Build and test

```sh
cargo build
cargo test
```

## Usage

```sh
cargo run -p cleancopy-cli -- --print plain fixtures/kitchen-sink.md
cargo run -p cleancopy-cli -- --print markdown < fixtures/kitchen-sink.md
# After building:
./target/debug/cleancopy --print plain notes.md
```

The optional file argument defaults to stdin; `-` explicitly selects stdin. Output uses native line endings; non-empty plain text ends with a trailing newline, while Markdown remains trimmed. Exit codes are `0` for success, `1` for empty input, `2` for I/O errors or unimplemented clipboard operations, and `3` for invalid arguments.

Plain text uses numbered unordered lists, four-space nested indentation with second-level `1.1` numbering and third-level `-` markers, checkboxes for tasks, bracketed H2–H6 headings, fenced-code separators, readable links and images, and numbered endnotes. Tables use Unicode display widths (CJK characters are two columns) and switch to record-style output above 60 columns. Code-block contents retain their whitespace and blank lines.

## Workspace

- `cleancopy-core`: comrak parsing with tables, tasks, strikethrough, autolinks and footnotes; plain-text and Markdown renderers.
- `cleancopy-clipboard`: shared payload and explicit not-implemented backend only.
- `cleancopy-cli`: clap-based `cleancopy` binary.
- `fixtures/` and `tests/snapshots/`: Chinese/English fixtures and accepted insta snapshots. Snapshot line endings are normalized for cross-platform tests.
