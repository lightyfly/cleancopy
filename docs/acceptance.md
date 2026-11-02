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

- Workspace: **161 passed, 0 failed, 6 ignored** across all binaries/doc tests.
  Desktop library: 31; configuration integration: 30; engine integration: 31.
- Explicit ignored desktop run: **2 passed, 0 failed**. These are separate
  executions, not part of the 161 workspace passes. Other packages' four
  ignored clipboard tests were not exercised by the desktop-only command.
- Formatting and strict all-target workspace Clippy succeeded.
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

## Measured latency — final successful native run

| Operation | Actual elapsed | Budget | Result |
|---|---:|---:|---|
| Unknown App → visible secondary menu | **50.85 ms** | ≤100 ms | **Passed** |
| Ctrl+Alt+V → visible Full | **39.38 ms** | Reported | Observed |
| Ctrl+Alt+V → visible undo menu | **31.22 ms** | Reported | Observed |
| Silent OFF primary → Full, first / second | **33.55 / 52.20 ms** | Reported | Observed |
| 10k characters → conversion + native paste | **39.02 ms** | ≤300 ms | **Passed** |
| Independent smoke Ctrl+Alt+V → visible Full | **34.72 ms** | Reported | Observed |

The harness prints real `MEASURE` lines and asserts both requested budgets.
Menu timings include native window mapping **and expected painted panel pixels**;
paste timing ends when the target peer records received clipboard bytes. Timings
include input-process/polling overhead. These are individual Linux/Xvfb software
rendering observations, not percentiles or an M1/i5 hardware qualification.
Removing arbitration resolved the previous 430.17 ms menu / 431.31 ms paste
failures; those historical numbers are not current results.

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

Eight native screenshots were regenerated by the final matrix under
[docs/screenshots](screenshots/): `e2e-secondary`, `e2e-remember-checked`,
`e2e-full`, `e2e-undo`, `e2e-nonsilent-1`, `e2e-nonsilent-2`, `e2e-settings`,
and `e2e-preview` (all `.png`, 1280×800). No pixel-perfect manual approval is
claimed. No StatusNotifier host was available, so real tray-host integration
was not verified. The complete original eight-criterion release gate is **not
passed** merely because this requested hotkey change passes.

See [desktop-verification.md](desktop-verification.md) for runtime method and
adjacent probes, [compat.md](compat.md) for compatibility scope, and
[distribution.md](distribution.md) for native build instructions (not executed
as part of this change).
