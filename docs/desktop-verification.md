# Desktop verification

## Desktop acceptance matrix (2026-11-02)

Executed successfully from the repository root:

```sh
xvfb-run -a -s '-screen 0 1280x800x24 -noreset' cargo test -p cleancopy-desktop -- --ignored --nocapture
```

The ignored process harness launches the real `cleancopy-app --config`, xfwm4,
and independent `examples/target.rs` peers on its own isolated 1280×800 Xvfb.
It builds the application and example executables first, so the command does
not depend on prebuilt or stale peers. Dependencies: Cargo, Xvfb/xvfb-run,
xfwm4, xdotool, and an X11 software-rendering environment.

Results: `desktop_runtime_matrix` **1 passed, 0 failed** (8.75 s), and
`hotkey_pastes_formatted_text_and_restores_all_native_bytes` **1 passed,
0 failed** (2.44 s). Nonignored tests were filtered out; this is not a full
workspace regression run. Raw output is saved in `desktop-e2e-run.log`.

| Case | Actual result | Runtime evidence |
| --- | --- | --- |
| (a) Single press, plain app | PASS | A gedit-class native peer received `Title\n\n一、Heading\n\nbold`; no menu was mapped during the observed conversion. |
| (b) Word-like app | PASS | WINWORD.EXE-class peer requested HTML and received semantic H1/Word heading markup and `<strong>bold</strong>`. This is not certification against installed Microsoft Word or WPS. |
| (c) Clipboard restoration | PASS | All four payload targets restored byte-exactly, including opaque binary, native 16-bit and native 32-bit data, with original property types/formats. The fifth advertised target is TARGETS; its exact membership and native metadata are checked, not its unspecified atom ordering. |
| (d) Unknown app / remember | PASS | Secondary panel opened without taking target focus; native mouse clicked the remember checkbox, key 2 chose numbered text, rule persisted, and the next press pasted silently. |
| (e) Double press | PASS | Two rapid native hotkeys opened the full menu with secondary panel collapsed; Escape dismissed it. |
| (f) Silent mode off | PASS | Two successive single presses each opened the full menu after saving `silent_mode=false`. |
| (g) Undo conversion | PASS | Native menu click triggered Ctrl+Z followed by original-source paste; peer event log and resulting document bytes were checked. |
| (h) Latency reporting | PASS for measurement; FAIL for two design budgets | Actual end-to-end observations are listed below. |

Final-run timings (include input-process and polling overhead):

- Unknown-app single press → menu mapped: **430.17 ms**, exceeds ≤100 ms.
- Double press → full menu mapped: **84.67 ms**.
- Double press → undo menu mapped: **85.78 ms**.
- Nonsilent single press → full menu mapped: **47.47 / 47.82 ms**.
- 10,000 characters, single press → conversion plus native paste: **431.31 ms**,
  exceeds ≤300 ms. Output bytes and subsequent restoration also passed.

The silent single-press path waits 400 ms to arbitrate double press. These
measurements include that wait; they are not isolated renderer benchmarks.
Production timing behavior was not changed and the unmet budgets remain open.

Harness repairs: removed an unused import (no compiler error was reproduced on
rustc 1.99.1); corrected H1/H2 and trailing-newline expectations to match the
specified renderer; normalized only TARGETS ordering; moved the secondary-panel
pixel probe away from its highlighted row. No payload-byte checks were relaxed.

Eight native root-window PNG captures, all 1280×800:

- `screenshots/e2e-secondary.png`
- `screenshots/e2e-remember-checked.png`
- `screenshots/e2e-full.png`
- `screenshots/e2e-undo.png`
- `screenshots/e2e-nonsilent-1.png`
- `screenshots/e2e-nonsilent-2.png`
- `screenshots/e2e-settings.png`
- `screenshots/e2e-preview.png`

Settings and preview launch probes also passed. Adjacent native smoke probes
passed for CapsLock+NumLock, held Shift, a competing hotkey grab (BadAccess),
nonfocus pointer-positioned menu, and Escape. No StatusNotifier host was
available, so tray integration was not validated. Image Read returned base64
rather than a usable visual rendering: captures, live panel-pixel checks and
native actions are evidence, but manual visual acceptance of every label is
not claimed.

## Historical desktop stage C — items 1 and 2

Scope: visual glyph/alignment fixes and the opt-in Linux X11 platform. No SNI tray registration, default-daemon change, or stage-C item-5 matrix/performance suite is included. This session and its agents did not run `git commit`. An external repository change moved HEAD from `47d5a98` to `742899e` (`WIP stage C (interrupted): Linux hotkey/foreground/paste backend, smoke test, examples`) during verification; it was not reverted or rewritten.

## Run the native platform

```sh
cargo run -p cleancopy-desktop --example linux -- /tmp/cleancopy-config.toml
```

The optional positional argument is a configuration path; omit it to use defaults. The runner prints `CleanCopy Linux hotkey ready` after registering its native hotkey and hiding the initial host window. It requires X11/XWayland with XTEST. A competing grab fails with a checked `BadAccess` error. The existing application/preview entry point is unchanged.

## Workspace checks (2026-11-01)

Executed from the workspace root:

```sh
cargo fmt
cargo build
cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo check --target x86_64-pc-windows-msvc
cargo check --target aarch64-apple-darwin
```

Observed results:

- `cargo build`: finished successfully.
- `cargo test`: **151 passed, 0 failed, 5 ignored**, summed across all test binaries and doc tests. Desktop library: **23 passed**, including **5 Linux platform unit tests**.
- `cargo fmt --check`: success, no output.
- Workspace Clippy, all targets, warnings denied: finished successfully.
- Windows MSVC and macOS ARM64 workspace checks: both finished successfully. These are compilation checks, not runtime verification. The new native desktop adapter is Linux-only (`cfg(target_os = "linux")`); no Windows/macOS desktop Platform adapter is supplied by this change. Existing portable engine and previews remain available.

## Native runtime smoke

```sh
cargo test -p cleancopy-desktop --test linux_smoke -- --ignored --nocapture
```

The test owns an isolated Xvfb server, xfwm4, the native menu runner, and independent X11 fixture/target/read peers. It drives the actual global hotkey with `xdotool key ctrl+shift+v`, rather than directly invoking the engine.

The first completed run reported:

```text
native Ctrl+Shift+V → 13 formatted bytes; restored all 4 native targets byte-exactly; CapsLock+NumLock+held Shift, BadAccess conflict, pointer-positioned nonfocus menu/Escape passed
```

It checks opaque format-8 bytes, native format-16 and format-32 bytes, atom target/type names, and the exact TARGETS set. It also checks that silent paste does not show the menu, menu display preserves the target's focus, the window has override-redirect set, and double-press places it near pointer coordinates (350, 200).

A repeated run exposed a real owner-retirement race: a selection request routed before replacement could lose its response when the old owner closed. The owner now synchronizes with the server, drains queued requests, and finishes accepted INCR transfers within a bounded three-second grace period. A deterministic ignored regression covers queued requests and INCR under both explicit stop and SelectionClear. All three clipboard graphical tests passed on isolated Xvfb. After the fix, the strict desktop smoke passed again: **1 passed, 0 failed**, 2.44 seconds (`/tmp/cleancopy-c-smoke-after-fix.log`). No timeout failures were suppressed in the smoke. Three additional post-fix repeats each passed (2.38s, 2.39s, 2.38s; `/tmp/cleancopy-linux-final-{1,2,3}.log`).

## Screenshots

The actual Xvfb previews were recaptured at:

- `screenshots/menu.png` — 960×600
- `screenshots/menu-secondary.png` — 960×600
- `screenshots/settings.png` — 1120×720
- `screenshots/tray.png` — 720×520

All four PNGs and the reference images were opened with the image Read tool. The tool returned base64 payloads instead of usable rendered images in this session, so an honest pixel-level visual comparison could not be certified. PNG decoding/dimensions are valid, the installed symbol font covers all requested glyphs, and explicit software rendering produced identical menu pixels. These checks do not replace visual acceptance.

Native menu runtime evidence is also captured by the smoke at `/tmp/cleancopy-native-menu.png` (temporary local evidence, not a committed deliverable).
