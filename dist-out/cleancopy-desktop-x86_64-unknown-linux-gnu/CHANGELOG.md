# Changelog

All notable changes will be documented here. The workspace version is currently `0.1.0`; this file does not assert that a release has been published.

## [Unreleased]

### Added

- Rust workspace with core rendering, native clipboard, and CLI crates.
- comrak Markdown parsing with tables, task lists, strikethrough, autolinks, and footnotes.
- Readable plain text: hierarchical numbering, task symbols, code separators, Unicode-width tables with a record fallback above 60 columns, links, and endnotes.
- Inline-styled HTML and lightly cleaned Markdown passthrough.
- File, stdin, and terminal-triggered clipboard input; multi, plain, HTML, Markdown, print, and inspect modes.
- Native macOS NSPasteboard backend for HTML, plain text, and custom Markdown.
- Native Windows backend with UTF-16 text, CF_HTML byte-offset encoding, custom Markdown, and clipboard-open retries.
- Native Linux X11 selection owner, background CLI holder, all advertised text aliases, custom Markdown, and incremental large transfers.
- Renderer/golden tests, portable CF_HTML tests, and isolated X11 clipboard probes.
- English and Simplified Chinese READMEs, contribution guidance, exact plain-text rules, and MIT OR Apache-2.0 license texts.
- Claude Code command/skill and Codex instruction templates, with local-format verification recorded separately.
- cargo-dist 0.32.0 configuration and a manual, read-only distribution-plan workflow; no publishing automation is enabled.
- Complete application test matrix with untested cells and reproducible kitchen-sink Markdown/plain X11 representation checks.

### Verification boundary

- Linux X11 has runtime evidence from isolated Xvfb, including independent readback after CLI exit.
- macOS and Windows are **compile checked only**, not runtime verified. Real target-application paste compatibility remains unverified; see [compatibility](docs/compat.md) and [verification](docs/verification.md).
- Native Wayland, clipboard-manager handoff, watch mode, hotkeys, daemon/app rules, configuration, syntax highlighting, and an integration installer are not implemented.
- No published package, installer, or demonstration is claimed by this changelog.
