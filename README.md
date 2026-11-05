# CleanCopy

**Copy Markdown once; offer rich text, readable plain text, and Markdown to your next paste.**

[简体中文](README.zh-CN.md)

CleanCopy is a local Rust CLI, plus a Linux X11 desktop app, that converts Markdown into multiple clipboard representations. The receiving application chooses which representation to paste; perfect output in every application is a goal, not a verified guarantee.

## Why

AI-generated Markdown is useful in a terminal or editor, but its headings, tables, and formatting markers are not always useful when pasted into a document or chat. [Claude Code issue #18170](https://github.com/anthropics/claude-code/issues/18170) is one motivation for this project. **GitHub stars and issue reaction counts must be rechecked before publication**; no popularity counts are asserted here.

CleanCopy runs locally without a service or API key. Conversion and clipboard access do not send document content to a server. Installing dependencies may require network access.

## Install from source

From this checkout, with a Rust toolchain and Cargo installed:

```sh
cargo install --path crates/cleancopy-cli
```

Ensure Cargo's binary directory (normally `$HOME/.cargo/bin`) is on `PATH`. The binary is named `cleancopy`, not `cleancopy-cli`. This is a source-install workflow; no published registry package, Homebrew/Scoop package, release installer, or downloadable binary is claimed here. See [distribution planning](docs/distribution.md) for the separate packaging workflow.

## Usage

```sh
cleancopy notes.md                    # HTML + plain text + custom Markdown
printf '# Hello\n\n- One\n- Two\n' | cleancopy
cleancopy                            # read current clipboard if stdin is a terminal
cleancopy - < notes.md               # explicitly read stdin
cleancopy --plain notes.md           # rendered text only
cleancopy --html notes.md            # HTML with rendered text fallback
cleancopy --markdown notes.md        # Markdown in the ordinary text channel
cleancopy --print plain notes.md     # print without writing the clipboard
cleancopy --print html notes.md
cleancopy --print markdown notes.md
cleancopy inspect                    # native format names and byte sizes
```

A file takes precedence over stdin. Without a file, redirected/piped stdin is read; only terminal stdin selects the current clipboard. Input files and stdin must be UTF-8. Mode options are mutually exclusive. `--print` never writes the clipboard, but still reads it if no file is supplied and stdin is a terminal.

| Mode | Ordinary text channel | Other representations |
| --- | --- | --- |
| Default (multi) | Rendered plain text | HTML and custom Markdown |
| `--plain` | Rendered plain text | None |
| `--html` | Rendered plain text | HTML |
| `--markdown` | Cleaned Markdown source | None |

Use `--markdown` for an editor that ignores custom Markdown types or converts pasted HTML undesirably. A plain-text paste shortcut in **multi mode** selects rendered text, not the original Markdown. Advertising a Markdown type does not guarantee an application reads it.

Exit codes: `0` success (including help/version), `1` empty/whitespace-only input, `2` clipboard or I/O failure, `3` invalid arguments. `inspect` reads available formats and their native byte sizes, not a guaranteed atomic clipboard snapshot; sizes can include native terminators/allocation padding.

## Rendering

The parser enables tables, tasks, strikethrough, autolinks, and footnotes. Plain text uses numbered lists, four-space nesting, `【H2–H6】` headings, `☐`/`☑` tasks, code separators, readable links, and numbered footnotes. Tables use Unicode display widths and become records above 60 columns. See the exact [plain-text rules](docs/plain-text-rules.md).

HTML is a complete minimal document with inline code/table styles and static task symbols. Raw HTML is escaped, not executed as markup; dangerous link/image destinations are blocked by comrak. There is no syntax highlighting, and the CLI has no RTF output (the desktop app's Word/WPS format adds RTF). Markdown passthrough removes terminal escape/control sequences (preserving tabs and line breaks), trims edge whitespace, and normalizes line endings; it is not byte-for-byte preservation or CommonMark reformatting. Plain text and Markdown use CRLF on Windows and LF elsewhere; HTML uses LF. Nonempty plain text ends in a newline; Markdown is trimmed.

## Native clipboard formats and verification

| Platform | Multi-mode formats | Evidence boundary |
| --- | --- | --- |
| macOS | `public.html`, `public.utf8-plain-text`, `net.daringfireball.markdown` | **Compile checked ONLY; not runtime tested** |
| Windows | `HTML Format` (CF_HTML, UTF-8 byte offsets), `CF_UNICODETEXT` (UTF-16), registered `text/markdown` | **Compile checked ONLY; not runtime tested** |
| Linux X11 | `text/html`, `text/markdown`, `UTF8_STRING`, `TEXT`, `text/plain`, `text/plain;charset=utf-8`, `text/plain;charset=UTF-8` | **Only runtime-tested platform**, using isolated Xvfb |

On Linux, a background holder serves clipboard requests after the CLI exits, until another owner replaces it. It needs a working X11 connection (`DISPLAY`). Native Wayland is unsupported; XWayland may provide an X11 connection, but bridging to native Wayland apps is unverified. Clipboard-manager handoff is not implemented.

Xvfb readback is not application paste testing. Word, WPS, WeChat, Feishu, Obsidian, Typora, and other desktop app behavior must be verified separately. See [compatibility status](docs/compat.md) and the detailed [verification record](docs/verification.md). The [design](docs/design.md) contains proposals and historical milestone targets, not proof of completed features or a release.

## Desktop app (Linux X11 only)

`cleancopy-app` is a background helper that converts at paste time. Build it from source; it is not packaged or published:

```sh
cargo install --path crates/cleancopy-desktop
cleancopy-app              # run the daemon
cleancopy-app --settings   # open settings
```

- **Shortcuts:** Ctrl+Shift+V is smart paste, using the format configured for the frontmost app; Ctrl+Alt+V opens the full paste menu. Both can be changed, and ordinary Ctrl+V is untouched.
- **Formats:** numbered plain text, Word/WPS rich text (HTML + RTF), unchanged Markdown, stripped plain text, and HTML source (off by default). Text over 2 MB gets no Word/WPS rich text; silent paste falls back to numbered plain text.
- **Sources:** Markdown or HTML on the clipboard, for example from ChatGPT, Gemini, or a browser. Ordinary text pastes unchanged by default.
- **Clipboard:** the original contents are restored after the target has read the paste, unless you copied something newer.
- **Other:** tray menu, onboarding, TOML settings with per-app rules, undo of the last conversion, and safe mode after repeated crashes.

It needs `libxkbcommon-x11` at runtime (`libxkbcommon-x11-0` on Debian/Ubuntu); without it the daemon crashes at startup. The hotkey daemon, native paste, and tray exist on Linux X11 only. Native Wayland is not supported. **macOS and Windows offer only the settings and preview UI.** Runtime evidence comes from isolated Xvfb, not from real Word/WPS paste testing. See the [desktop README](crates/cleancopy-desktop/README.md) and [acceptance notes](docs/acceptance.md).

## Claude Code and Codex integration

Install the CLI first and make sure the agent's shell can find `cleancopy` and access your desktop clipboard. A remote/container agent operates on its own display, not automatically your laptop's clipboard. Review integration files before using them; copying text changes your clipboard and may expose it to clipboard history software.

### Project-local Claude Code installation

Copy the integration templates into the **destination project**, not into global configuration. For example, from the CleanCopy checkout (replace the destination path):

```sh
PROJECT=/absolute/path/to/your-project
mkdir -p "$PROJECT/.claude/commands" "$PROJECT/.claude/skills/cleancopy"
cp integrations/claude-code/commands/copy.md "$PROJECT/.claude/commands/copy.md"
cp integrations/claude-code/skills/cleancopy/SKILL.md "$PROJECT/.claude/skills/cleancopy/SKILL.md"
```

Do not overwrite an existing command/skill without reviewing it. The command is intended for explicit `/copy` requests; the skill explains when to call the CLI, including `--plain` for text-only output. Integration file availability is separate from CLI functionality; these are manual installation examples, not an implemented `cleancopy integrate` command.

For Codex, review `integrations/codex/AGENTS.snippet.md` and manually merge its instructions into the destination project's `AGENTS.md`; do not replace an existing `AGENTS.md`. Independently of templates, either agent can write the desired answer to a UTF-8 file and run:

```sh
cleancopy /absolute/path/to/answer.md
# Or force readable text:
cleancopy --plain /absolute/path/to/answer.md
```

Do not interpolate untrusted answer text directly into a shell command. Prefer a file, or a quoted heredoc with a delimiter that cannot occur in the content. Permission prompts and shell execution remain controlled by the agent environment.

### Optional Stop hook — never enabled automatically

Automatic copying after every answer can overwrite unrelated clipboard contents. It is **opt-in only**, not part of the installation above. See the [integration verification notes](docs/verification.md) for the current hook-input findings. No ready-to-install Stop-hook template is supplied. If you implement one, verify the current Claude Code hook schema and how to obtain the last assistant response, then manually merge it into the destination project's `.claude/settings.json`, preserving existing settings. Hook event JSON is metadata, not necessarily Markdown: **do not pipe the raw event JSON into `cleancopy`**. A hook needs to extract the intended answer, handle missing content without overwriting the clipboard, and avoid recursive execution. No Stop hook is installed or enabled by CleanCopy itself.

## Development

```sh
cargo build --workspace
cargo test --workspace
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
```

Graphical clipboard and desktop tests are ignored by default because they replace clipboard contents; CI runs them on isolated Xvfb displays. See [CONTRIBUTING.md](CONTRIBUTING.md) for isolated Xvfb commands, test safety, and compatibility reports.

Workspace: `cleancopy-core` contains parsing/rendering, `cleancopy-clipboard` native access and CF_HTML, `cleancopy-cli` the CLI, and `cleancopy-desktop` the desktop app. Fixtures live in `fixtures/`; golden snapshots in `tests/snapshots/`.

## Roadmap (not implemented promises)

- Real macOS/Windows clipboard runtime and target-application paste testing.
- macOS and Windows desktop adapters (hotkeys, native paste, tray).
- Native Wayland support and clipboard-manager interoperability.
- CLI watch mode and an interactive integration installer.
- Syntax highlighting and editor/browser extensions.
- Reviewed release packaging and a real recorded demonstration after verification.

The CLI does not support `--watch`, `--normalize`, daemon/config/integrate commands, or automatic foreground-app selection; hotkeys, settings, and per-app rules belong to the desktop app. There is no demo recording included or implied here.

## License

Dual licensed under **MIT OR Apache-2.0**, at your option. See [LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE). Contributions are welcome under the same terms; see [CONTRIBUTING.md](CONTRIBUTING.md) and [CHANGELOG.md](CHANGELOG.md).
