# Application compatibility

Evidence recorded **2026-11-01**. H = expected HTML; P = expected plain text;
M = expected Markdown. These are expectations from [design §11](design.md#11-测试矩阵),
not observed format choices. **Untested** means no paste into that application
was performed. `—` follows the design's non-applicable cells, not a test result.

## Application matrix

| Target application | Expected | macOS | Windows | Linux | What to check |
| --- | --- | --- | --- | --- | --- |
| Microsoft Word | H | Untested | Untested | — | Table borders; monospaced code |
| WPS Writer | H | Untested | Untested | Untested | HTML support unknown |
| Feishu documents | H | Untested | Untested | Untested | Test web and desktop separately |
| Feishu / DingTalk chat | H or P | Untested | Untested | Untested | Record chosen format |
| Outlook / Apple Mail / Gmail web | H | Untested | Untested | Untested | Test each separately; Outlook CSS limitations |
| Google Docs / Notion | H | Untested | Untested | Untested | Notion may recognize Markdown in plain text |
| WeChat (PC) | P | Untested | Untested | — | Whether HTML is preferred remains unknown |
| QQ / WeCom | P | Untested | Untested | Untested | Test each separately |
| Notepad / TextEdit plain mode | P | Untested | Untested | Untested (gedit) | TextEdit rich mode may choose HTML |
| Terminal / iTerm2 / Windows Terminal / GNOME Terminal | P | Untested | Untested | Untested | Real paste and multiline confirmation not exercised |
| VS Code | M (via P channel) | Untested | Untested | Untested | Default text paste vs Paste As behavior |
| Obsidian | M | Untested | Untested | Untested | HTML-to-Markdown conversion may be lossy |
| Typora | M | Untested | Untested | Untested | HTML-to-Markdown conversion may differ from source |
| Logseq / SiYuan / Joplin | M | Untested | Untested | Untested | Test each separately for conversion losses |
| Slack | H or P | Untested | Untested | Untested | Record chosen format |

No matrix cell is marked successful on the strength of a CLI, renderer, or X11
selection test. There are no application version numbers to report because no
GUI target application was tested.

## Platform evidence (not application compatibility)

- **Linux X11:** runtime-tested on an isolated Xvfb display, including independent
  reader requests after the CLI exits, all advertised aliases, mode replacement,
  UTF-8 Chinese/emoji and large INCR transfers. See [verification](verification.md).
- **macOS:** compile-checked only (x86_64 and aarch64); no NSPasteboard runtime or
  paste result. These are earlier W3 checks, not new W4 desktop tests.
- **Windows:** compile-checked only (x86_64 MSVC); portable CF_HTML unit tests run
  on Linux do not establish Windows runtime behavior.
- **Native Wayland:** unsupported. XWayland requires `DISPLAY`; bridging into
  native Wayland applications is untested. Clipboard-manager handoff is untested.

## W4 locally executed representation checks

Reproduce from the repository root with Rust, Python 3, `diff`, and Xvfb:

```sh
cargo build --workspace
cargo build --workspace --example x11_read
xvfb-run -a -s '-screen 0 1280x1024x24 -noreset' python3 scripts/w4-compat-probe.py
```

The probe uses `fixtures/kitchen-sink.md` (headings, nested lists, tables, code,
links, Chinese and English), writes with `--markdown`, then retrieves
`UTF8_STRING` through the independent `x11_read` process after the CLI exits.
It saves those bytes to a temporary `.md` file and invokes `diff -u`.

Observed results:

- Raw fixture diff: **exit 1**, solely because CleanCopy trims the final LF.
  The content of the last line is unchanged; diff reports `No newline at end of
  file`. This is the implemented Markdown-cleaning contract, not byte-for-byte
  identity with the untrimmed fixture.
- Compared with the trimmed fixture and `--print markdown`: **exact equality**,
  1,460 bytes, SHA-256
  `a0b617555224305cfeaa20f405a22dc6b00eb33d94217f45175dcecef711cd56`.
- `--plain` → independent `UTF8_STRING` readback: **exact equality** with
  `--print plain`, 1,483 bytes, SHA-256
  `0bb953cd781638ac687a39af929bb058d4267c741c419cfa0f9deabd97cfa7d6`.
  It includes numbered lists and a trailing LF. This checks the representation
  available for terminal paste, **not actual terminal paste handling**, bracketed
  paste, multiline safety prompts, or command execution.

The initial run used `xvfb-run -a` with a one-second startup delay; the `-noreset`
form above keeps the isolated server alive across gaps between X11 clients.
Initial ignored-test runs without `-noreset` intermittently failed during X11
connection setup (`failed to read whole buffer` / `failed to fill whole buffer`).
A bounded comparison reproduced failures with default Xvfb (native test 37/40,
CLI test 28/30 passed) and none with `-noreset` (40/40 and 30/30 passed).
An independent atom-lifetime probe confirmed the default server resets after
its last client disconnects. Both ignored Cargo tests and both CLI probes then
passed together with `-noreset`. No Rust code was changed to mask server resets.
No real desktop clipboard was modified by these checks.

## Complete the manual editor validation

1. Record OS, application version, web browser if applicable, and paste settings.
2. Copy `fixtures/kitchen-sink.md` in default Multi mode; paste normally into the
   editor, save the Markdown, and diff against the fixture. Record any HTML
   conversion losses, including tables and code whitespace.
3. Repeat using `cleancopy --markdown fixtures/kitchen-sink.md`; account only for
   documented line-ending/outer-whitespace/ANSI cleanup, not unexplained edits.
4. Compare plain-text paste (often Cmd/Ctrl+Shift+V; application-dependent) and
   any HTML-auto-conversion setting. Exact setting names have not been verified.
5. Update only the application/OS cell actually tested, with ✅/⚠️/❌ and the
   observed format or uncertainty. Attach diffs and `cleancopy inspect` output.

For Markdown editors, `--markdown` puts source in the ordinary text channel and
omits HTML; it does not require support for a custom Markdown clipboard type.
In Multi mode the ordinary text channel contains **rendered plain text**, not
Markdown source. Therefore forcing plain-text paste of Multi mode does not
recover original Markdown. Use `--markdown` when source fidelity matters.
For WeChat, try `--plain`; proportional fonts may still misalign aligned tables.
Automatic per-app rules, watch mode and hotkeys remain future work.
