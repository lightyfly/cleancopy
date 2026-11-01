# Desktop stage C — items 1 and 2

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
