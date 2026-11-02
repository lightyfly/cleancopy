# Desktop acceptance evidence

Assessed **2026-11-02**. This report distinguishes the requested hotkey change
from whole-product acceptance. Linux native peers are independent X11 fixture
processes, **not installed gedit, Microsoft Word, or WPS**.

## Current validation

All four requested commands completed successfully after the final code edits:

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
xvfb-run -a -s '-screen 0 1280x800x24 -noreset' cargo test -p cleancopy-desktop -- --ignored --nocapture
```

- Workspace (`cargo test --workspace`, round 3 final): **205 passed, 0 failed, 7 ignored**.
  Per binary: core unit 8, cancel 3, desktop_html_detection 7, golden 1, html 5,
  numbered 12, rtf 9, rules 8, word_lists 4; clipboard unit 10 (+2 ignored),
  clipboard native 0 (+2 ignored), clipboard tests 0 (+1 ignored); cli 12;
  desktop library 47; config 30; engine 49; desktop_e2e 0 (+1 ignored);
  linux_smoke 0 (+1 ignored).
- Explicit ignored desktop run (the xvfb command above): **2 passed, 0 failed**
  (`desktop_runtime_matrix`, Linux native smoke), run 3× consecutively, all green.
  These are separate executions, not part of the 205 workspace passes. The
  clipboard crate's 5 ignored native tests were not exercised by that command.
- `cargo fmt --all -- --check` and `cargo clippy --workspace --all-targets -- -D warnings` passed.
- Raw final native output: [desktop-e2e-run.log](desktop-e2e-run.log).
  Workspace and lint output: [hotkey-workspace-tests.log](hotkey-workspace-tests.log),
  [hotkey-clippy.log](hotkey-clippy.log).

## Requested hotkey contract

| Behavior | Evidence and result |
|---|---|
| Primary Ctrl+Shift+V / macOS Cmd+Shift+V acts immediately | **Passed** in engine and Linux native scope. No pending press state, double-press detection, or 400 ms arbitration remains. Restoration still has its separate 150 ms safety deadline. |
| Silent ON, recognized App | **Passed**: App→format conversion immediately pastes; known non-Markdown input passes through unchanged. Native gedit-class and WINWORD.EXE-class peers received numbered text and semantic Word HTML respectively, without a mapped menu. |
| Silent ON, unknown App | **G-03 product decision (2026-11-02)**: ordinary plain text and text over 5 MiB paste the original directly, with no menu or conversion. Convertible Markdown uses the secondary menu with 记住此 App. Image/binary clipboard handling retains the unsupported-content menu. Explicit Full and silent-OFF still show a menu, with unsafe conversions disabled. A configured fixed fallback supplies an effective format. See [pm-fixes-v2.md](pm-fixes-v2.md) for current regression evidence. |
| Silent OFF | **Passed**: each completed primary invocation opens Full. Hotkeys while a menu or clipboard restoration transaction is busy remain ignored. |
| Independent full menu | **Passed**: Ctrl+Alt+V opens Full without first pasting, regardless of silent mode/content. macOS default Cmd+Option+V is configuration/UI support, not native macOS runtime verification. Original, formatted-text and undo actions remain available as appropriate. |
| Separate persisted settings and recorders | **Passed** in configuration/UI tests; both values round-trip and merge independently, recorders edit only their selected field and reject duplicate chords. Tray and first-use guidance distinguish smart paste from full menu. Native live reload of both chords passed. |
| Native shortcut safety | **Passed**: CapsLock/NumLock/held Shift; exact modifier matching; removed binding available to a second client; conflicting replacement reports BadAccess and retains both old bindings; partial new grab is released. |
| Restoration and undo | **Passed** within Linux fixtures/mock engine: payload bytes plus original native type/format restored; actual Ctrl+Z then original-source paste observed. Newer external clipboard copies are intentionally not overwritten. |

## Measured latency — round 3 final native runs (3 consecutive runs)

Menus now map first with a loading model and the X11 clipboard is read on a
worker thread; counts/type fill in afterwards (N-1). The menu window is
pre-created hidden and every menu variant is laid out during startup frames
(warm-up), which removed the first-press outlier.

| Operation | Run 1 | Run 2 | Run 3 | Budget | Result |
|---|---:|---:|---:|---:|---|
| Secondary ×20 incl. first press after launch: min / median / max | 11.56 / 22.76 / 34.96 | 12.15 / 24.67 / 36.67 | 11.26 / 24.25 / 28.08 | ≤100 ms | **Passed** |
| First press after launch (secondary) | 21.63 | 20.93 | 21.34 | ≤100 ms | **Passed** |
| 3 MB clipboard → visible secondary (unknown App) | 45.39 | 20.28 | 17.72 | ≤100 ms | **Passed** |
| 3 MB clipboard → visible Full (Ctrl+Alt+V) | 17.38 | 17.31 | 17.13 | ≤100 ms | **Passed** |
| 5 MB clipboard → visible Full (Ctrl+Alt+V) | 27.31 | 23.03 | 23.26 | ≤100 ms | **Passed** |
| Ctrl+Alt+V → visible Full (small) | — | — | 21.05 | Reported | Observed |
| Ctrl+Alt+V → visible undo menu | — | — | 23.12 | Reported | Observed |
| Silent OFF primary → Full, first / second | — | — | 21.62 / 21.64 | Reported | Observed |
| 10k characters → conversion + native paste | — | — | 17.81 | ≤300 ms | **Passed** |
| Large background-cancel source → visible Full | — | — | 35.24 | Reported | Observed (was 2226 ms) |
| Independent smoke Ctrl+Alt+V → visible Full | — | — | 16.68 | Reported | Observed |

All values in ms; "—" means only the run-3 log is quoted (runs 1–2 were
similar). Menu timings end when the native window is mapped **and** the
expected panel pixels are painted (the loading model for large clipboards);
the clipboard metadata arrives later (a 3 MB X11 read is still ~0.5–0.6 s, now
off the UI path). Escape during that read cancels it and the late result is
discarded (asserted in e2e). Earlier 164.96 ms secondary and 2226.19 ms
large-Full samples are historical, not current results. These are Linux/Xvfb
software-rendering observations, not hardware qualification.

## Broader design acceptance and limitations

| Original design criterion | Current status |
|---|---|
| Real ChatGPT/Gemini → Word/WPS styles, editable tables, bold | **Not verified**. Semantic renderer tests and Word-class fixture consumption do not establish installed-app compatibility. |
| Notepad/TextEdit numbering and deep nesting | **Not verified** against those installed apps. Renderer and Linux fixture evidence exists. |
| Menu/paste latency | **Passed within measured Linux fixture scope** above; specified hardware equivalence not established. |
| Unknown App + remembering | **Passed within popup-fallback Linux/mock scope** above. |
| Undo and exact clipboard restoration | **Passed within Linux/mock scope** above; real Word/WPS consumption timing and other OSes not verified. |
| Settings preview ≤50 ms; edit/close/reopen UI workflow | **Not verified**. Persistence tests and Settings/preview launch probes pass, but do not measure refresh latency or complete that UI workflow. |
| Permission denial/regrant and reauthorization guidance | **Not verified**. Native macOS/Windows desktop adapters remain unimplemented. |
| Reference visual fidelity and native platform symbols | **Not verified**. Captures and panel-pixel probes exist; no certified reference-image comparison or native macOS/Windows screenshot acceptance. |

Twelve native screenshots were regenerated by the final matrix under
[docs/screenshots](screenshots/). Menu shots are now captures of the menu window
itself (640×360) taken after the async metadata fill — the previous 1280×800
root captures silently omitted the composited menu window: `e2e-secondary`,
`e2e-remember-checked`, `e2e-full`, `e2e-undo`, `e2e-nonsilent-1`,
`e2e-nonsilent-2`, `e2e-background-before`, `e2e-background-progress`.
Root captures (1280×800): `e2e-settings`, `e2e-preview`, `e2e-tray`.
Comparison against the mockups: [visual-diff.md](visual-diff.md). No
pixel-perfect approval is claimed. No StatusNotifier host was available, so real
tray-host integration was not verified. The complete original eight-criterion
release gate is **not passed** merely because these items pass.

## Round 3 / report-v2 items

| Item | Status and evidence |
|---|---|
| 取消 stops background conversion | Cooperative cancellation token checked by numbered/plain/HTML/RTF renderers; worker result discarded. `tests/cancel.rs` (3), engine cancel tests, native 取消 click in e2e. comrak *parse* itself is not interruptible. |
| N-1 menus first, metadata async, <100 ms at any size | Done: loading model + background X11 read. Engine tests `full_menu_appears_before_the_clipboard_is_read_then_fills_in`, `unknown_app_secondary_menu_appears_first_and_fills_in`, `unknown_app_plain_text_closes_the_loading_menu_and_pastes_original`, `escape_during_loading_cancels_and_ignores_the_late_result`, `hotkeys_are_ignored_while_the_clipboard_is_loading`, `recognized_app_silent_paste_never_shows_a_loading_menu`; native 3/5 MB timings above. Markdown detection bounded to a 256 KB prefix. |
| 165 ms outlier / pre-created windows | The single menu window was already created hidden at startup; the outlier was first-frame glyph/layout work. Startup warm-up now lays out all menu variants while hidden. 20-run distribution above. |
| N-2 ChatGPT/Gemini code-block headers | `html_source.rs` lifts `<pre>`/`<code-block>`; header text becomes the fence language only. 7 unit tests + `chatgpt_code_block_header_never_leaks_into_code`. |
| N-3 Word/WPS nested lists | `<p>` inside `<li>` unwrapped, empty `<li>` dropped, explicit `list-style-type` per depth. `tests/word_lists.rs` (4). Not verified in installed Word/WPS. |
| N-4 visible-text char count | `visible_chars` strips tags/script/style/head and decodes entities. `html_source_menu_counts_visible_text_not_tags`. |
| Target lock at hotkey press | Target captured at press; own pid and transient popups (notification, popup/dropdown menu, tooltip, combo, dnd) rejected. `large_conversion_keeps_the_target_locked_at_hotkey_time`. |
| Visual pass | Design shadow/border, key caps, selected-row colours, chip summary; blur strategy in `ui/backdrop.rs` (Linux solid fallback verified; macOS NSVisualEffectView / Windows Mica via window-vibrancy compile-checked only, not run). Settings 960×680 rework (row tint, segmented controls, secondary list, no bold, CJK font priority). See visual-diff.md. |

See [desktop-verification.md](desktop-verification.md) for runtime method and
adjacent probes, [compat.md](compat.md) for compatibility scope, and
[distribution.md](distribution.md) for native build instructions (not executed
as part of this change).

## Onboarding
- First launch (config without `onboarding_done`) → daemon spawns `--onboarding`; 完成/跳过 persist the flag; tray 「使用引导…」 reopens it. e2e: `PASS onboarding: …`.
- Step 3 hotkey inside the input inserts the formatted sample (unit test `hotkey_paste_formats_but_plain_paste_stays_raw`; e2e capture onboarding-3.png).
- macOS Accessibility (AXIsProcessTrusted / prompt) is cross-compile checked only (aarch64-apple-darwin clippy), never run.

## Round v3 (2026-11-02, UTC+8)

- Onboarding: step 1 key-cap lines 「智能粘贴」/「打开完整菜单」 follow the configured hotkeys; step 3 green ✓ hint under the right box with a 12px gap to the button bar (measured on docs/screenshots/onboarding-3.png: green rows 285–295, bar line 308); detection follows any configured hotkey (`try_step_follows_a_non_v_configured_hotkey`).
- B-1 idle CPU (40s, release, Xvfb + xfwm4): 116.74% before → 0.25% after (event-driven loop: X11 socket poll + tray waker, 1s idle tick).
- B-2: 取消 fires on pointer press, works after a single move with no prior hover (`cancel_works_with_single_move_then_click_and_no_prior_hover`).
- B-3: target badge always painted while converting (`converting_always_paints_the_locked_target_badge`). The reported “Markdown” case was not reproduced; the long-label case failed before the fix.
- B-4: 「Markdown 原样」 from HTML sources uses ATX headings without closing `#` (`markdown_output_uses_atx_headings_without_closing_hashes`). Only the HTML-source path was affected.
- Item 3 / O-1: first secondary-menu trigger across fresh launches was 46.0–51.0ms (8 runs, median 48.9). After 60s idle: 44.3/45.4/48.1ms. With 8-core load: median 51.5, max 54.2. The menu maps 1.2–2.1ms into its frame. The 159ms spike was not reproduced.
- Item 6: Chinese numbering rule, see design.md §7.1 and tests in tests/numbered.rs.
- Item 7: e2e screenshots go to target/e2e-screenshots by default; `scripts/e2e-desktop.sh --update-screenshots` writes docs/screenshots.
- e2e: exit 0, 13 PASS lines (docs/desktop-e2e-run.log).

## Round v4 (2026-11-02, UTC+8)

Workspace gates at every commit: `cargo build --workspace --all-targets`,
`cargo test --workspace` (**238 passed, 0 failed, 7 ignored**),
`cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`.
`scripts/e2e-desktop.sh`: exit 0, 13 PASS lines (docs/desktop-e2e-run.log).

- H1 rule (PM): a single leading H1 is the document title (unnumbered; Word
  `MsoTitle` / RTF `Title` style); then H2=一、 H3=（一） H4=1.; several H1s →
  H1=一、. No H1 at all is treated like the title case. design.md §7.1.
- M-4: onboarding step 3 pauses the daemon's global grab (pid marker) so the
  try box receives Ctrl+Shift+V; 「更改快捷键」 opens Settings recording the paste hotkey.
- M-2: undo row no longer advertises Ctrl+Z (not a global binding).
- M-6: custom traffic-light title bar only on macOS; native decorations and
  per-window titles elsewhere (in-content strip removed in round v5).
- M-3 (WPS first-level bullets): **not reproduced** — WPS cannot be started on
  the isolated test display. Likely cause: Word-HTML `disc` imports as a Symbol-font
  glyph and no Symbol/Wingdings font is installed here. No code change.

### 10k characters → conversion + native paste (S-perf)

| Build | Measure | Result |
|---|---|---|
| aa99cd9 (round v3) e2e, debug | hotkey → native paste | 19.61 ms |
| HEAD e2e, debug, 10 runs | hotkey → native paste | 18.52–20.06 ms (median ≈18.8) |
| aa99cd9 core, release, realistic 10k Markdown | detect + numbered + Word HTML + RTF + plain | 3.35 ms median |
| HEAD core, same input | same | 3.29 ms median |

No regression between aa99cd9 and HEAD, so no bisect was needed; budget <100 ms met.

### Memory (release daemon worker process, Xvfb, VmRSS / VmHWM)

| Point | Before (aa99cd9 and pre-fix HEAD) | After (mmap fonts + malloc_trim) |
|---|---:|---:|
| Idle after launch | 199.2 MB | 119.6 MB |
| After 10k paste | 200.0 MB | 120.4 MB |
| After 5 MB paste | 301.5 MB (stays) | 135.5 MB (peak 222.2 MB) |

About 65 MB of the remainder is llvmpipe / LLVM software GL on this box.

### S-01 cancel reliability

- `scripts/e2e-desktop.sh` ×10: **10/10** exit 0, `PASS S-01` in every run.
- Manual 5 MB background conversion + 取消 click on Xvfb (release) ×10: **10/10** cancelled, no paste.
- One extra `--update-screenshots` run failed once on a timing assert
  (secondary menu 128 ms > 100 ms, run 11 of 20) and passed on rerun; reported, not hidden.

## Round v5 (2026-11-02, UTC+8)

Gates: build all targets, `cargo test --workspace` **239 passed, 0 failed, 7 ignored**,
fmt --check, clippy -D warnings (also clippy for aarch64-apple-darwin and
x86_64-pc-windows-msvc). `scripts/e2e-desktop.sh --update-screenshots`: exit 0, 14 PASS lines.

- Title strip: on Linux/Windows the 44 px in-content title strip of Settings and
  onboarding is gone; only the native title bar shows the title. Content starts
  at the top with its existing padding (Settings panes 16 px, onboarding body 18 px).
  Window content height shrinks by 44 px (Settings 960×636, onboarding 480×316).
  macOS keeps the strip and traffic lights (unchanged, `TITLE_STRIP` = 44).
  e2e onboarding clicks moved up 44 px (footer y 289, try box y 156).
  Screenshots refreshed and inspected (e2e-settings, onboarding-1/2/3).
- 「试一试」 fast hotkey: on Linux the onboarding window grabs the paste hotkey
  on X11 itself while step 3 is shown and focused, so the press arrives with
  its exact modifiers. Before this, egui only saw a modifier-less Paste and
  inserted raw Markdown. The grab survives the short focus-out that an activated
  grab causes, and is released 500 ms after a real focus loss, on leaving the
  step, or on exit. Unit test `native_hotkey_formats_only_on_the_try_step_and_focuses_the_box`;
  e2e now uses an instant `xdotool key ctrl+shift+v` and checks the box text.
  Fast-press check on Xvfb (10 chords in one `xdotool key --delay 1`, 20 with
  `--delay 0`, 20 spaced 250 ms): **50/50 formatted, 0 raw** in each of 4 runs
  (200/200) without a daemon, and 50/50 in each of 2 runs with the daemon
  paused/restoring around it. Plain Ctrl+V in the box still pastes raw text.
  Windows/macOS still use the egui path, so a single-frame press there is not fixed or verified.
