# CleanCopy desktop — Stage C

This crate supplies the headless paste engine and the `cleancopy-app` desktop UI.
On Linux, the default launch runs the X11 global-hotkey daemon, native clipboard
adapter, nonactivating egui menu, and pure-Rust `ksni` StatusNotifier tray. A missing
tray host or session bus is logged without disabling the hotkey. No GTK is needed.
Windows/macOS native desktop adapters are not implemented; those platforms retain
the settings and preview UI. Existing CLI behavior is unchanged.

## Run the UI

```sh
cargo run -p cleancopy-desktop --bin cleancopy-app
# Open settings with an isolated persistent file:
cargo run -p cleancopy-desktop --bin cleancopy-app -- --settings --config /tmp/cleancopy.toml
# Run the Linux daemon using that file:
cargo run -p cleancopy-desktop --bin cleancopy-app -- --config /tmp/cleancopy.toml
# Session-only demo data (also ignores --config):
cargo run -p cleancopy-desktop --bin cleancopy-app -- --preview menu
cargo run -p cleancopy-desktop --bin cleancopy-app -- --preview menu-secondary
cargo run -p cleancopy-desktop --bin cleancopy-app -- --preview settings
cargo run -p cleancopy-desktop --bin cleancopy-app -- --preview tray
```

Normal settings edits save immediately with atomic replacement and field-level
merging with daemon updates; errors appear in an error window with a retry button.
Previews never load or save user settings. Silent mode defaults **on**. Preview
paste/undo effects are simulated; the default Linux daemon performs real actions.
The tray preview displays menu content without registering a second tray item.
Native Ctrl+Shift+V currently remains fixed even if the saved hotkey label changes.

CJK fonts are loaded at runtime, preferring Noto Sans CJK under
`/usr/share/fonts`, with fallback discovery under that directory and
`~/.local/share/fonts`. No system font files are embedded or checked in. If no
font is found, egui's built-in fonts remain available, but CJK coverage may be
incomplete.

### Headless preview capture

Build the app and development-only X11 screenshot helper first:

```sh
cargo build -p cleancopy-desktop --bin cleancopy-app --example screenshot
xvfb-run -a -s "-screen 0 1280x800x24" sh -c '
  target/debug/cleancopy-app --preview settings &
  app=$!
  sleep 2
  window=$(xdotool search --sync --onlyvisible --name "^CleanCopy$" | sort -n | tail -n 1)
  target/debug/examples/screenshot /tmp/settings.png "$window"
  kill "$app"
  wait "$app" || true
'
```

Repeat with `menu`, `menu-secondary`, and `tray` to capture all views. The checked-in
captures are under `docs/screenshots/`. The helper uses X11 GetImage, so no GTK,
ImageMagick, or system development-package installation is required. Xvfb,
xdotool and a working OpenGL/Mesa runtime are needed for this capture recipe.

The resolved feature graph, not just the manifest, must remain X11-only:

```sh
cargo tree -p cleancopy-desktop -e features
```

See `vendor/egui-winit/PATCH.md` for the feature-only upstream patch that removes
Wayland clipboard dependencies from the X11 build.

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
rules mutate configuration in memory; the native driver persists changes using
`Config::save_changes`, merging fields edited since its last saved snapshot with
external settings changes under a shared file lock.

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
`Platform`; effects return the adapter's associated error type. Its `now` method
supplies nondecreasing Unix milliseconds to `hotkey_pressed()` and `tick()`.
Alternatively, feed explicitly timestamped `Event`s to `handle`; schedule timers
using `next_deadline`. The same timebase must be used for persisted pause/history
values. `MenuKey` events resolve numeric accelerators (1–5 by default), Enter for
the initial default action, and Escape to cancel; native navigation can dispatch
`MenuChoice` directly for the currently highlighted action.
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

The supplied `docs/desktop/*.html` and `base.css` are the visual references.
Linux supports WM_CLASS/foreground discovery, lossless native clipboard snapshots,
XTEST paste/undo, passive-hotkey conflict reporting, and pointer-positioned menus.
Native Wayland, Windows/macOS desktop adapters, caret positioning, native Word/WPS
acceptance, HTML-only import, mixed-image placeholders, and background conversion
UI remain outside this stage.

Ordinary engine tests use a mock platform and explicit timestamps. The ignored
Linux integration tests own an isolated Xvfb server and real native clipboard
peers. Run the stage-C end-to-end matrix (requires Xvfb, xfwm4, xdotool):

```sh
scripts/e2e-desktop.sh
```

See `docs/desktop-verification.md` for runtime evidence and screenshots. Run
workspace checks from the repository root:

```sh
cargo build
cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```
