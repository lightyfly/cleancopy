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
- `cleancopy-app` desktop app for Linux X11: global smart-paste and full-menu hotkeys, per-app format rules, floating menu, tray, settings, onboarding, undo of the last conversion, and TOML configuration.
- Numbered plain text, Word/WPS HTML, and RTF renderers, plus HTML clipboard sources (ChatGPT, Gemini, browsers) converted to Markdown.
- Clipboard snapshot and restore: on X11 the restore waits until the paste target has read the data; elsewhere it uses a size-based wait.
- Crash supervisor and safe mode, with a plain-text fallback for clipboards over 2 MB.
- CI job running the ignored native tests on isolated Xvfb displays.

### Verification boundary

- Linux X11 has runtime evidence from isolated Xvfb, including independent readback after CLI exit.
- macOS and Windows are **compile checked only**, not runtime verified. Real target-application paste compatibility remains unverified; see [compatibility](docs/compat.md) and [verification](docs/verification.md).
- The desktop daemon, hotkeys, and native paste exist on Linux X11 only; macOS and Windows have the settings and preview UI, with no desktop adapter.
- Native Wayland, clipboard-manager handoff, CLI watch mode, syntax highlighting, and an integration installer are not implemented.
- No published package, installer, or demonstration is claimed by this changelog.
