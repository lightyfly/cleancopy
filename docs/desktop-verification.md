# Desktop verification

## Immediate smart paste and independent Full menu — 2026-11-02

**Verdict: PASS for the requested change in Linux/X11 fixture scope.** This is
not macOS/Windows native runtime or installed Word/WPS compatibility acceptance.

### Contract

- Primary Ctrl+Shift+V (macOS default Cmd+Shift+V) acts immediately. Silent ON
  uses the effective App→format rule, or opens Secondary with 记住此 App when
  popup fallback supplies no format. Unknown non-Markdown/image/empty content
  also opens Secondary; unavailable conversions stay disabled and Original
  remains the safe fallback.
- Silent OFF opens Full on every completed primary invocation.
- Ctrl+Alt+V (macOS default Cmd+Option+V) opens Full independently, without a
  preliminary paste. No double-press path or 400 ms arbitration remains.
- Settings has independent persisted recorders; tray and first-use guidance
  distinguish the two actions. Busy menu/restoration transactions remain guarded.

### Reproducible commands and actual results

Executed from the repository root after final code edits:

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
xvfb-run -a -s '-screen 0 1280x800x24 -noreset' cargo test -p cleancopy-desktop -- --ignored --nocapture
```

- Formatting and strict workspace/all-target Clippy: success.
- Workspace: **161 passed, 0 failed, 6 ignored**. Desktop library **31**,
  config integration **30**, engine integration **31**.
- Explicit ignored desktop run: `desktop_runtime_matrix` **1 passed**, 7.02 s;
  `hotkey_pastes_formatted_text_and_restores_all_native_bytes` **1 passed**,
  4.45 s. Total **2 passed, 0 failed**, separate from the workspace count.
- Raw outputs: [desktop-e2e-run.log](desktop-e2e-run.log),
  [hotkey-workspace-tests.log](hotkey-workspace-tests.log),
  [hotkey-clippy.log](hotkey-clippy.log).

The runtime harness builds and launches the real `cleancopy-app --config`,
xfwm4 and independent `examples/target.rs` fixture/target peers on isolated
Xvfb. Native xdotool events drive the public keyboard/pointer interface; target
receipt files and native X11 window/pixel observations establish results.
Prerequisites: Cargo, Xvfb/xvfb-run, xfwm4, xdotool, X11/XTEST and software GL.
The smoke uses the native Linux example runner and separate X11 peers.

### Observations of the running application

| Action | Observed result |
|---|---|
| Primary in gedit-class peer | Exact numbered text received; no menu mapped. |
| Primary in WINWORD.EXE-class peer | Semantic Word heading HTML and bold received. These are fixtures, not installed applications. |
| Primary in unknown peer | Painted Secondary, focus retained. Native checkbox click + key 2 saved rule; following primary silently pasted. |
| Ctrl+Alt+V | Painted Full with formatted panel collapsed, no preliminary paste, target focus retained; Escape dismissed. |
| Undo through native menu | Actual Ctrl+Z then original-source paste; event ordering/document bytes checked. |
| Silent OFF, two completed primary invocations | Full mapped on each invocation. |
| 10k-character primary paste | Converted output received by native target; original clipboard later restored. |
| Settings/preview launch | Both mapped and captured; launch probes only, not edit/reopen or latency acceptance. |
| 🔍 CapsLock + NumLock and held Shift | Correct single conversion; no leaked held Shift in injected paste. |
| 🔍 Live reload of Ctrl+Shift+F8 and Ctrl+Super+F9 | Both independently active; Full never issued an extra paste. |
| 🔍 Extra modifiers / released old binding | Extra Shift/Super combinations did not activate; old Ctrl+Alt+V could be grabbed by an independent client and did not open daemon menu. |
| 🔍 Conflicting replacement binding | BadAccess reported, previous two bindings still worked, partially added F10 grab released. |
| 🔍 Clipboard preservation | All payload bytes and original native property type/format restored, including opaque format-8, native format-16/32. TARGETS exact membership checked, unordered atom listing normalized only. |

### Final-run real measurements

```text
MEASURE unknown-app→secondary-visible secondary: 50.85 ms
MEASURE Ctrl+Alt+V→full-menu-visible full: 39.38 ms
MEASURE Ctrl+Alt+V→full-menu-visible undo: 31.22 ms
MEASURE Ctrl+Shift+V→full-menu-visible nonsilent-1: 33.55 ms
MEASURE Ctrl+Shift+V→full-menu-visible nonsilent-2: 52.20 ms
MEASURE 10k-char hotkey→convert+native-paste: 39.02 ms
MEASURE Ctrl+Alt+V→full-menu-visible: 34.72 ms
```

Unknown-secondary **≤100 ms** and 10k convert/paste **≤300 ms** are now asserted
and passed. Menu timings end only after mapping **and expected painted panel
pixels**, not merely a window ID. Paste ends at native target receipt. Input
process/polling overhead is included. Individual Xvfb samples are not percentile
benchmarks or qualification of M1/i5 hardware. Earlier 430.17/431.31 ms failures
were caused by the removed arbitration delay and are superseded, not hidden.

### Findings and verification repairs

- An initial live-rebinding probe used Ctrl+Alt+F9. XKB consumed its KeyPress as
  `XF86Switch_VT_9` even under Xvfb (xev saw release but no press). Changed the
  probe to Ctrl+Super+F9; all rebinding/rollback checks remain. This is an OS
  reserved-chord caveat, not a claimed supported VT-switch binding.
- A subsequent old-binding probe initially counted the target peer's own
  Ctrl+Alt+V handling as a daemon paste: that peer intentionally accepts any
  Ctrl+V without Shift. The corrected probe proves release by independently
  grabbing the old chord before injecting it, then checks no menu/no extra
  paste. No production failure or byte assertion was suppressed.
- No StatusNotifier host existed; daemon continued and logged it. Native tray
  host integration remains unverified. xfwm software-GL, accessibility/session
  manager warnings were nonfatal and are retained in the raw log.
- Native macOS/Windows automation adapters remain unimplemented. Defaults,
  persistence and UI labels are not evidence of working native hotkeys there.
- Captures and painted-panel probes are evidence of visible contents, not
  certified pixel-perfect reference comparisons. Real ChatGPT/Gemini→Word/WPS,
  permission denial/regrant, preview ≤50 ms and full Settings edit/reopen
  workflow remain outside this verification.

### Screenshot evidence

All final-matrix root captures are 1280×800:

- [Unknown secondary](screenshots/e2e-secondary.png)
- [Remember selected](screenshots/e2e-remember-checked.png)
- [Full menu](screenshots/e2e-full.png)
- [Undo](screenshots/e2e-undo.png)
- [Silent OFF first](screenshots/e2e-nonsilent-1.png) /
  [second](screenshots/e2e-nonsilent-2.png)
- [Settings](screenshots/e2e-settings.png) /
  [preview](screenshots/e2e-preview.png)

The independent smoke also writes `/tmp/cleancopy-native-menu.png` (temporary
local evidence, not a committed deliverable).

### Native runner

```sh
cargo run -p cleancopy-desktop --example linux -- /tmp/cleancopy-config.toml
```

Omit the configuration path for defaults. `CleanCopy Linux hotkey ready`
indicates initial native grabs and hidden host-window setup completed. X11 or
XWayland with XTEST is required. Both configured bindings reload from disk;
conflicts are checked and retain the old registrations.

Historical stage-C work also fixed a clipboard owner-retirement race by
synchronizing/draining accepted requests and completing accepted INCR transfers
within a bounded grace period. Its ignored clipboard regressions belong to the
clipboard package and were not rerun by this desktop-only ignored command.
See [acceptance.md](acceptance.md) for broader release gates; do not infer
whole-product acceptance from these scoped successes.
