# Visual comparison — small egui alignment pass

## Scope and evidence

Compared the five permitted downscaled JPG references in `/tmp/vd`, opened individually: `mock-floating-menu.jpg`, `e2e-full.jpg`, `e2e-secondary.jpg`, `mock-settings.jpg`, and `e2e-settings.jpg`. No original PNGs or additional images were opened. Exact values and wording below come from `/workspace/cleancopy-design/base.css`, `floating-menu.html`, and `settings.html`, checked against the egui source.

These are existing screenshots, not post-change evidence. **Screenshots have not been re-captured; xvfb e2e has not been rerun in this pass.** Source-level alignment does not establish pixel-perfect visual parity.

## Small mismatches selected for correction

| Mismatch | Reference | Final status |
| --- | --- | --- |
| Settings muted labels used RGB `(145,145,153)` rather than `#A1A1A6`. | `base.css` `--mute`; settings section/pane labels and footer | Fixed in source: RGB `(161,161,166)`. |
| Settings current-App preview label used muted color and 11-point text rather than secondary text `#6E6E73` at the default 13-point size. | `settings.html` preview header; `base.css` `--sub` and body font size | Fixed in source: secondary color and default font size; separate muted separator. |
| Settings secondary-format rows were 36 points high rather than 38. | `settings.html` `.row { height:38px }` | Fixed in source: 38-point rows, preserving scrolling and controls. |
| Settings called the stripped format `纯文本 · 去除标记`, while the floating-menu reference and egui menu call it `纯文本 · 去符号`. | `floating-menu.html`, stripped-format item | Fixed in source: settings label aligned; conversion behavior unchanged. |

## Remaining / intentionally not changed

- Floating-menu canvas placement and background differ from the mock editor scene. The mock includes an editor window and gradient; egui uses a standalone menu canvas. Rebuilding that scene is outside small copy/color/spacing corrections.
- Browser menu translucency, blur, shadow, and boxed shortcut styling differ from the current egui presentation. No compositor or widget redesign in this pass.
- Settings uses native egui controls rather than identical HTML segmented controls, selectors, and checkbox/keycap styling. Selected-App styling also differs from the mock's full-row tint. These remain.
- Settings has separate Notion/Obsidian groups, additional format/reorder controls, two editable shortcuts, and onboarding/silent-mode controls. Preserve these functional differences rather than hide them to match a static mock.
- Clipboard counts, target badges, enabled/disabled actions, and platform-specific shortcut text are runtime-dependent; the mock's fixed sample values are not acceptance requirements.
- Font metrics, line wrapping, and overall visual parity remain unverified after changes without new screenshots.

## Verification

Initial notes were committed before code edits in `b9aa64a`. After the corrections:

- `cargo build --workspace --all-targets` — passed.
- `cargo test -p cleancopy-desktop` — passed: 107 tests passed, 2 native integration tests ignored by their existing annotations (`desktop_runtime_matrix` and Linux native smoke). These ignored tests were not run separately.
- `cargo clippy --workspace --all-targets -- -D warnings` — passed.
- The first `cargo fmt --all -- --check` found a line-wrapping discrepancy in the new preview label; ran `cargo fmt --all` before the successful build/test/clippy sequence.

Run logs are local temporary files: `/tmp/cleancopy-visual-build.log`, `/tmp/cleancopy-visual-test.log`, and `/tmp/cleancopy-visual-clippy.log` (not committed delivery artifacts).

No screenshots or prior verification artifacts are presented as a fresh run. No xvfb e2e or screenshot recapture was performed; final appearance remains unverified.
