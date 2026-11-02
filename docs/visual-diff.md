# Visual comparison — round 3

Compared the re-captured native screenshots (round-3 final e2e run) against
`/workspace/cleancopy-design/floating-menu.png` and `tray.png`, opened as
downscaled JPGs. Values come from `base.css` / `floating-menu.html` /
`tray.html` and are pinned in source (`ui/menus.rs`, `ui/backdrop.rs`).

**Harness fix:** previous `e2e-*` menu screenshots were root-window captures
in which xfwm4 omitted the composited menu window, so they showed only the
fixture windows. The harness now captures the menu window itself and waits for
the async metadata fill before shooting. Earlier "visual" evidence based on
those files was therefore not evidence of the menu at all.

## Fixed in this pass (design value → implementation)

| Element | Design | Implementation |
|---|---|---|
| Menu shadow | `0 12px 40px rgba(0,0,0,.14), 0 2px 6px rgba(0,0,0,.06)` | Two `egui::Shadow`s: offset [0,12] blur 40 α36; offset [0,2] blur 6 α15 (unit test `menu_shadow_matches_design_css`). Was a single hard offset rect α12. |
| Menu border | `1px rgba(0,0,0,.08)`, radius 12 | 1px black α20, radius 12 (was `--line`). |
| Key caps | `.kbd span`: min 18×18, padding 0 4, 1px `--line`, radius 4, `#FAFAFB`, 500 11px `--mute`, gap 3 | Painted caps with exactly these values; hints like `Ctrl+Z` / `Ctrl+Alt+V` / `⌘Z` split into caps (`hints_split_into_design_key_caps`). Was plain text. |
| Selected row | `--sel #5B5BD6`, radius 6, white text, icon/hint `rgba(255,255,255,.8)`, caps `rgba(255,255,255,.15)` fill + `.3` border | Same values (icon was pure white; caps new). |
| Summary chip | `剪贴板 <b>Markdown · 1,284 字</b>` | `剪贴板` in `--sub` + summary in regular-weight `--text` (bold removed per the no-bold rule). |
| Tray panel | Panel ends 6px after the last item | Height 346 → 304; removed ~40 px of empty space. |

## Blur / translucency

Design: `rgba(255,255,255,.92)` + `backdrop-filter: blur(30px)`.
`ui/backdrop.rs` selects the material per platform: macOS
`NSVisualEffectView`, Windows 11 Mica, Linux solid fill. macOS/Windows are
wired through `window-vibrancy` 0.8 (`backdrop::apply` at window creation,
transparent viewport, transparent clear colour, panel fill α235). They were
**only compile-checked** from Linux: `cargo check`/`cargo clippy -D warnings`
for `x86_64-pc-windows-msvc` and `aarch64-apple-darwin` succeed. Nothing was
linked or run on macOS/Windows, so the runtime look is **not verified**. Linux
keeps the opaque solid fill (verified by screenshots).

## Remaining differences (intentional or not fixed)

- Mock floating menu sits over an editor scene with a gradient; ours is a
  standalone 640×360 menu window over `#EDEDF2`.
- Remember row: mock shows a muted `✓`; ours is a functional `☐/☑` checkbox.
- Tray mock shows `⌘,` / `⌘Q` caps for 设置/退出; those shortcuts do not exist on
  Linux, so no caps are drawn. Tray "recent" rows use sample data in preview.
- Menu item labels are Proportional 13 px regular, painted once (unit test
  `item_labels_are_painted_once_in_regular_weight`), identical to settings body
  text. They still look heavier than the browser mock because epaint 0.31
  boosts glyph coverage with a fixed 0.55 gamma (`ImageData::srgba_pixels`),
  which eframe does not expose; dense CJK makes it most visible.
- Settings (960×680, compared against settings.png): sections all visible
  without scrolling; left column scrolls independently. Dropdowns are now
  borderless right-aligned text + chevron (gray only on hover/open); 静默模式 is
  a switch in the first 快捷键 row; the 首次使用 note was removed (no onboarding
  screen exists yet to hold it). Remaining: the Markdown source pane is
  not syntax-coloured; section titles are 400 not 500 (no Medium CJK face is
  installed on this Linux box, and bold is forbidden); 二级菜单 shows all 5
  formats (mock shows 4); 唤出快捷键 is shown as two editable shortcut rows.
- Word preview headings/emphasis still render with the bold family because
  that is the pasted content's formatting, not UI chrome.

## Screenshots (docs/screenshots/)

Menu window captures 640×360: `e2e-secondary.png`, `e2e-remember-checked.png`,
`e2e-full.png`, `e2e-undo.png`, `e2e-nonsilent-1.png`, `e2e-nonsilent-2.png`,
`e2e-background-before.png`, `e2e-background-progress.png`.
Root captures 1280×800: `e2e-preview.png`, `e2e-settings.png`, `e2e-tray.png`.

## Palette (PM option A) and onboarding

Body text is now #2B2B33; secondary SUB (98,98,106) and MUTED (106,106,114).
`ui::palette::tests::text_colours_meet_wcag_aa_on_every_background` asserts
≥ 4.5:1 on every background (printed with `--nocapture`):

| colour | white | pane #FAFAFB | key cap #F7F7F8 | track #F0F0F3 | selected row | tray #EDEDF2 |
|---|---|---|---|---|---|---|
| TEXT #2B2B33 | 14.04 | 13.46 | 13.11 | 12.34 | 12.41 | 12.03 |
| SUB (98,98,106) | 6.04 | 5.79 | 5.65 | 5.31 | 5.34 | 5.18 |
| MUTED (106,106,114) | 5.36 | 5.14 | 5.01 | 4.71 | 4.74 | 4.59 |

White on ACCENT: 5.37. Note: the previous SUB (110,110,115) failed AA on the track/tray
(4.46/4.35) and the previous MUTED (161,161,166) was ~2.2–2.6, so meeting AA meant
secondary text got *darker*, not lighter; only body text got lighter.

Onboarding (docs/screenshots/onboarding-1..3.png): no HTML mockup exists. design.md §1.1
describes step 3 as "choose numbering style"; per PM spec step 3 is "try it" instead.
