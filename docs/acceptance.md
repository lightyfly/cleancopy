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

## Round v5-final (report-v5, 2026-11-02, UTC+8)

Gates at the memory commit: build all targets, `cargo test --workspace` **248 passed,
0 failed, 7 ignored**, fmt --check, clippy -D warnings.
`scripts/e2e-desktop.sh --update-screenshots`: exit 0, 14 PASS lines; screenshots
refreshed and inspected (only `e2e-background-progress.png` changed in bytes).

- **M-1 list base:** the list base is global, one rung below the deepest heading
  numbering scheme anywhere in the document. Ladder `1.` → `（1）` → `①` → `•`
  (`a.` removed). Decimal H5 and below are not numbered: bold in rich output,
  un-numbered in plain text. Tests include `list_base_is_global_to_the_deepest_heading_scheme`
  (a list under H3 followed by H4) and the updated `numbered_levels` property.
  The tester's adjacency checker reported 0 problems on all 12 outputs (4 cases × 3 styles).
- **N-7 nbsp:** `&nbsp;`, `&#160;`, `&#xa0;` and U+00A0 survive HTML→Markdown,
  one space each (`nbsp_runs_are_kept_one_space_each`).
- **M-3 bullets:** Word/WPS HTML uses `list-style-type:none` with literal •/◦/▪
  plus a nbsp, so bullets don't depend on the font. **Verified on the HTML output only, not in WPS.**
- **M-4 try-it grab:** the onboarding try step grabs the hotkey only while one
  of our windows has focus (`_NET_WM_PID` walk; PointerRoot falls back to the
  pointer window). The grab is released as soon as focus is lost, with no 500 ms hold.
  - Fast presses: 50/50 in each of 4 runs without the daemon and in each of 2 runs with it.
  - Focus loss (3 runs): the daemon took the hotkey back in 150, 419 and 138 ms,
    bounded by its ~250 ms marker poll. Pasting into the other app worked, and
    after refocusing the box formatted 2/2.
  - Esc on any onboarding step closes the window without marking onboarding done.
- **C-1 supervisor:** after 5 worker crashes the supervisor (~5 MB RSS) enters safe mode:
  - It sends a desktop notification 「CleanCopy 遇到问题，已暂停格式化」.
  - The tray menu offers 重新启动 / 退出 CleanCopy.
  - Both hotkeys paste the original clipboard text unchanged.
  Measured under dbus-run-session with a mock notification server:
  - The notification text matched exactly.
  - 3/3 presses pasted the original Markdown verbatim.
  - Tray 重新启动 started one new worker ("hotkey ready").
- **Memory** (5 MB `v3-cancel5m.md`, release worker, the tester's method of
  VmRSS/VmHWM of the worker process with one paste; supervisor 5.2 MB extra):

  | choice | before peak | before steady | after peak | after steady (+3/+33/+93 s) |
  |---|---|---|---|---|
  | Word/WPS (key 3) | 994 MB | 296 MB | **378 MB** (378–381 over 4 runs) | **131 MB** |
  | Markdown (key 1) | 222 MB | 135 MB | **209 MB** | **121 MB** |

  The worker sits at 120 MB RSS idle (GL/LLVM/fonts) before any paste.
  - **Chunked parsing:** inputs ≥ 1 MiB are parsed in ~64 KiB runs of whole top-level blocks.
    Documents with reference/footnote definitions, raw HTML or indented fences
    are still parsed whole, so they keep the old peak.
  - **Identical output:** `tests/chunked.rs` shows chunked output is byte-identical
    for every renderer, and it was also identical on the 5 MB file (79 chunks).
  - **Steady-state fixes:**
    - fixed glibc mmap threshold;
    - Undo stores identical payloads once;
    - the X11 owner shares identical target bytes;
    - `last_write` keeps a fingerprint instead of a copy.
  - **Trade-off:** the Word paste of the 5 MB file took 4.1–4.5 s, against 3.6–3.8 s before.
- **M-2:** unchanged as instructed.

## v6 (report-v6)

- **Large text (item 1):** clipboard > 2 MB (`MAX_RICH_BYTES`) never builds Word/WPS rich text.
  Silent mode with a Word/WPS target pastes 中文编号纯文本 and shows a low-urgency toast
  "内容较大，已按纯文本粘贴"; the menu greys the Word/WPS item with "超过 2MB". 5 MB cap unchanged.
  Tests: `silent_word_target_over_2mb_pastes_numbered_plain_text_with_toast`,
  `menu_greys_word_rich_text_over_2mb_with_note`.
- **Memory reconciliation:** the tester's 907 MB reproduced exactly with their method
  (v3-5m.md, key 3, VmHWM): 907,248 kB HWM. Cause: that input (tables + many nested lists) yields
  ~100 MB RTF, 66 MB of it per-list `\listtable` entries; my earlier 378 MB came from v3-cancel5m.md
  (16 MB RTF). After aad4a19 (fingerprint instead of retained snapshot, no encode copy) rich text at
  2 MB (v3-2m.md, 2,097,053 B, same method): HWM 338,640 kB, steady 125 MB, paste after 2,546 ms —
  ≤ 400 MB, so the threshold stays 2 MB.
- **Safe mode (item 2):** "CleanCopy 遇到问题，已暂停格式化。", action "重新启动", one automatic
  retry after 5 min, reopening Settings restarts. Live (debug, mock notification server, retry
  overridden to 8 s): auto retry restarted after 8,134 ms; `--settings` restarted in 210 ms;
  action restarted in 207 ms. Test `notification_text_and_restart_request`.
- **Decimal (item 3):** a bullet list directly after a numbered list keeps "• " and does not consume
  numbers. Test `decimal_bullet_list_after_numbered_list_keeps_bullets`.
- **Undo (item 4):** test `undo_item_is_greyed_after_undoing` passes (undo record consumed, item
  disabled). WPS undo path reviewed in code only (Ctrl+Z, write original, paste); WPS was not run.
- **WPS HTML lists (item 5):** real `<ul>/<li>` with `list-style-type` disc/circle/square by depth,
  no inline glyphs (supersedes the v5 M-3 note above). Test
  `bullets_are_real_list_markers_not_inline_glyphs`. Verified on output only; Symbol-font marker
  rendering in WPS (v5 M-3) not re-checked.
- Gates: 253 pass / 0 fail / 7 ignored, clippy ok, fmt ok. e2e (`--update-screenshots`): 1 passed.

## v7 (report-v7)

- **D-A (WPS got raw Markdown for 2–5 MB):** confirmed in code. `finish_paste` armed the restore at
  paste + `RESTORE_DELAY_MS` (150 ms) and `Timer` restored unconditionally at that deadline, whether
  or not the target had requested the data yet; a reader that starts later than 150 ms gets the
  original. Fix: `Platform::restore_ready(elapsed, bytes)`, asked from 150 ms every 20 ms.
  X11: wait until ≥1 data SelectionRequest has been answered, no INCR transfer is active and the
  owner has been quiet 100 ms; stop at 5 s; stop at once on SelectionClear (newer copy → restore
  skipped by the fingerprint check). Other platforms: 150 ms + 400 ms/MB, max 3 s.
  Tests: `restore_wait_grows_with_size_and_is_capped`, `x11_restore_waits_for_completed_reads`.
- **Slow-reader verification** (`/tmp/slowwps`, WM_CLASS wps, Xvfb :93 + xfwm4, silent mode, starts
  reading 400 ms after Ctrl+V, TARGETS then UTF8_STRING after 200 ms, 20 ms sleep per INCR chunk),
  release builds, 2 rounds each:

  | input | 91b95a0 | this fix | read done / restored (ms after Ctrl+V) |
  |---|---|---|---|
  | 3 KB | original 2/2 | converted 2/2 | 603 / 723–725 |
  | 2m-exact (rich) | original 2/2 | converted 2/2 | 823 / 1120–1128 |
  | 3m | original 2/2 | converted 2/2 | 909–913 / 1048–1053 |
  | 5m | original 2/2 | converted 2/2 | 1059–1074 / 1217–1248 |

  Fast reader, 3 KB, 5 rounds (Ctrl+V → restore): before 142–145 ms, after 139–145 ms (no slowdown).
  Never-reading target: restored at 5,081 ms. Newer copy 1.3 s after paste: kept, restore skipped.
  The clipboard is restored after every paste. Limitation: any reader (e.g. a clipboard manager)
  counts as a completed request. **Real WPS not tested** (no isolated WPS available).
- **D-B:** a note replaces the shortcut hint and is right-aligned in grey at measured width; the
  main label "Word / WPS 富文本" is elided with "…" first if space is short. Screenshot (3 MB):
  `docs/screenshots/menu-word-over-2mb-note.png`.
- Gates: 255 pass / 0 fail / 7 ignored, clippy ok, fmt ok. e2e `--update-screenshots`: 1 passed
  (45.5 s vs 17 s in v6: e2e targets that never read the clipboard now wait for the 5 s timeout).

## v8 (report-v8)

- D-A v8: real WPS starts reading ~1.9 s after Ctrl+V and was still transferring at 5 s when v7 restored. New X11 rule (x11_restore_ready in crates/cleancopy-desktop/src/linux.rs): newer copy -> stop at once (restore skipped); 30 s absolute cap; 5 s timeout only if no SelectionRequest ever arrived; once reading started, restore only after transfer complete + 100 ms idle; no progress for > 5 s mid-transfer -> reader gone, restore. TransferProgress gained `requested`. Unit test x11_restore_waits_for_completed_reads extended.
- e2e: the harness's restore polling read the clipboard every 5 ms, so the owner was never idle 100 ms and every restore hit the timeout (45.5 s run). Restore polls now run every 200 ms. New e2e step: target reads 2 s after Ctrl+V with 1.5 s per INCR chunk; 1,260,076 B read in 13.4 s, converted text, original restored at 13.5 s. Full e2e: 30.08 s (1 passed), of which the slow-reader step is ~13.5 s, so the rest is ~16.6 s (derived, not separately timed).
- Slow-reader matrix (release, Xvfb :93, silent mode), 2 rounds each, all converted, clipboard restored after each read:

  | profile | input | read done ms | restored ms |
  |---|---|---|---|
  | 400 ms delay, 20 ms/chunk | 2m-exact | 823-826 | 1104-1115 |
  | same | 3m | 910-911 | 1035-1037 |
  | same | 5m | 1067-1069 | 1226-1229 |
  | 1.9 s delay, 300 ms/chunk (WPS-like) | 2m-exact | 5861-5866 | 5866-5923 |
  | same | 3m | 7066-7071 | 7066-7071 |
  | same | 5m | 9171-9174 | 9171-9175 |

  (restored == read done within 1 ms means the probe saw the new owner at its first check.)
- Small paste, fast reader, 3 KB, 5 rounds, Ctrl+V to restore: 140-144 ms (v7 before: 142-145 ms).
- Real WPS not tested (cannot be run in isolation here).
- Gates: 255 pass / 0 fail / 7 ignored, clippy ok, fmt ok.
