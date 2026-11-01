# CleanCopy desktop — Stage A

This crate is a **headless library**, not a running desktop application. The
future `cleancopy-app` binary will supply native platform adapters and the GUI.
There is deliberately no egui/eframe dependency, tray, global hotkey registration,
window, or simulated keyboard input in this stage. Existing CLI behavior is
unchanged.

## Product decisions

- Silent mode defaults **on**. A single shortcut resolves the frontmost app's
  configured format. Unknown apps open the secondary format menu; the remember
  checkbox records a stable app identifier, never its display name.
- A second shortcut within **400 ms** opens the full original/formatted/undo
  menu, rather than pasting. Silent single presses therefore deliberately wait
  400 ms. This supersedes the original design's double-press-to-execute behavior
  and its default-disabled silent mode. When silent mode is disabled, the first
  press opens a menu immediately.
- The native shortcut defaults to Cmd+Shift+V on macOS (configured as
  `Super+Shift+V`) and Ctrl+Shift+V elsewhere. Ordinary Cmd/Ctrl+V is untouched.
- Clipboard formats are raw bytes, not reconstructed from plain text. A native
  adapter must enumerate and preserve **all** original formats, expose canonical
  UTF-8 text formats to the engine, and translate generated HTML/RTF to its
  platform clipboard representations. The existing clipboard crate's write-only
  payload API is not a lossless snapshot adapter.
- Restoration is delayed after dispatching paste, not performed synchronously
  before the target can read it. A later native implementation must validate the
  chosen delay against real target applications; dispatch is not proof that an
  application has consumed clipboard data.

## Configuration

`config::Config` supports TOML load/save, platform paths from `dirs`, ordered and
individually enabled secondary formats, a popup or fixed-format fallback, timed
pause, and the two most recent paste records. Missing files load defaults;
malformed or unreadable files return errors. Saving writes a temporary file in
the destination directory, syncs it, and atomically replaces the destination.

Default rules include TextEdit/notepad/gedit → numbered plain text, Word/WPS →
Word rich text, and Notion/Obsidian → unchanged Markdown. Matching uses exact,
ASCII-case-insensitive bundle ID, executable basename, or X11 WM_CLASS. Bundle
IDs outrank executable names, which outrank WM_CLASS. Menu changes and remembered
rules mutate configuration in memory; a future application driver must persist
changes using `Config::save` and surface save failures.

## Renderers

The platform-independent APIs live in `cleancopy-core`:

- `render_numbered(input, NumberingStyle)` supports `Chinese`, `ChineseCompact`,
  and `Decimal`. H1 is unnumbered, lists continue at the level below their
  enclosing heading, nested lists indent four spaces, tables use tabs, and code
  content retains its whitespace. The exact settings HTML preview is tested.
  Output uses LF with no terminal newline. H1 resets document counters; separate
  list blocks restart; skipped heading ancestors start at one. Beyond the named
  Chinese patterns, decimal paths retain arbitrarily deep numbering.
- `render_plain` remains the existing symbol-stripping renderer.
- `render_word_html` retains native heading tags with Word heading metadata;
  `render_rtf` includes heading styles, native list definitions, editable tables,
  Unicode escaping, and gray monospaced code. Real Word/WPS paste interoperability
  still needs native acceptance testing.
- `is_markdown` detects parsed formatting constructs, not bare URLs or unmatched
  punctuation. Ordinary text is labeled “普通文本” and defaults to raw paste.
- “Markdown 原样” preserves source bytes rather than normalizing Markdown.

## Engine driver contract

Construct `engine::Engine::new(config, platform)` with an implementation of
`Platform`; effects return the adapter's associated error type. Feed timestamped
`Event`s to `handle` and schedule `Timer` events using `next_deadline`. Timestamps
are nondecreasing Unix milliseconds, matching persisted pause/history values.
At identical timestamps, dispatch hotkeys before timers so the inclusive 400 ms
double-press boundary is deterministic. Menu callbacks carry their menu ID;
stale callbacks cannot execute a later menu's action.

The engine waits **150 ms** after paste dispatch before attempting clipboard
restoration. Failed restoration retains the snapshot and schedules another
attempt 150 ms later. If the clipboard no longer equals the bytes written by the
engine, restoration is skipped rather than overwriting a newer user copy. New
paste transactions are blocked until the pending restoration completes or is
safely skipped. Native clipboard writes must be atomic on error: `Err` means no
clipboard mutation. The driver must continue servicing timers after failures
and when paused, and must not exit while a restoration is pending without a
recovery policy.

Foreground identity is checked before input injection, including after clipboard
writes. Menus must not activate another app. Undo is allowed only for the same
identified app and only once: it sends Undo, pastes the captured original, and
then restores the clipboard content captured immediately before that undo.
This does not prove that the app's editing state is unchanged; a native UI should
clearly describe “undo last conversion” as an immediate follow-up action.

Remembered choices and history are available through `Engine::config()`. The
caller is responsible for saving changed configuration and reporting errors.
The headless engine does not spawn threads, sleep, register hotkeys, access the
real clipboard, or retry input injection.

## Scope and remaining native work

The supplied `docs/desktop/*.html` and `base.css` remain authoritative for future
GUI visuals. Stage A does not implement GUI layout, accessibility permissions,
focus/caret discovery, clipboard ownership lifetimes, HTML-only clipboard import,
mixed-image placeholders, background conversion/progress UI, or native hotkey
conflict checks. Those require a platform driver and later desktop stages.

Tests use a mock platform and explicit timestamps; they do not mutate the real
system clipboard. Run workspace checks from the repository root:

```sh
cargo build
cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```
