# CleanCopy

Week-2 implementation of the [design](docs/design.md): a local Markdown-to-HTML/plain-text CLI and a lightly cleaned Markdown passthrough. macOS writes HTML, plain text, and Markdown in one pasteboard transaction. Linux provides HTML plus plain text through X11/XWayland; Windows clipboard access is still a stub.

## Build and test

```sh
cargo build
cargo test
# Optional clipboard tests (isolated X11 server, no desktop clipboard touched):
xvfb-run -a cargo test -p cleancopy-cli --test clipboard -- --ignored
xvfb-run -a cargo test -p cleancopy-clipboard --test native -- --ignored
```

## Usage

```sh
cargo run -p cleancopy-cli -- --print plain fixtures/kitchen-sink.md
cargo run -p cleancopy-cli -- --print markdown < fixtures/kitchen-sink.md
cargo run -p cleancopy-cli -- --print html fixtures/kitchen-sink.md
# After building:
./target/debug/cleancopy notes.md       # write all supported formats
./target/debug/cleancopy                # read clipboard when stdin is a terminal
./target/debug/cleancopy --plain < notes.md
./target/debug/cleancopy --html notes.md # HTML + plain fallback
./target/debug/cleancopy --markdown notes.md # Markdown via the text channel
./target/debug/cleancopy inspect
```

Without a file, piped stdin is used; when stdin is a terminal, the current clipboard text is used. `-` explicitly selects stdin. `--print plain|html|markdown` never writes the clipboard. Text output uses native line endings; non-empty plain text ends with a trailing newline, while Markdown remains trimmed. Exit codes are `0` for success, `1` for empty input, `2` for clipboard/I/O errors, and `3` for invalid arguments.

On Linux, a background holder process serves the clipboard after the command exits, until another application replaces it. Linux does not publish a custom Markdown target: `--markdown` puts Markdown in the plain-text channel. Native Wayland clipboard support is not enabled; use XWayland (`DISPLAY`). See [verification notes](docs/verification.md) for the arboard source audit and platform limitations.

Plain text uses numbered unordered lists, four-space nested indentation with second-level `1.1` numbering and third-level `-` markers, checkboxes for tasks, bracketed H2–H6 headings, fenced-code separators, readable links and images, and numbered endnotes. Tables use Unicode display widths (CJK characters are two columns) and switch to record-style output above 60 columns. Code-block contents retain their whitespace and blank lines.

## Workspace

- `cleancopy-core`: comrak parsing with tables, tasks, strikethrough, autolinks and footnotes; inline-styled HTML, plain-text, and Markdown renderers.
- `cleancopy-clipboard`: shared payload, macOS pasteboard backend, Linux arboard backend and X11 format inspection; Windows stub.
- `cleancopy-cli`: clap-based `cleancopy` binary.
- `fixtures/` and `tests/snapshots/`: Chinese/English fixtures and accepted insta snapshots. Snapshot line endings are normalized for cross-platform tests.
