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
| Summary chip | `剪贴板 <b>Markdown · 1,284 字</b>` | `剪贴板` in `--sub` + summary in bold `--text` (was single `--sub` run without the label). |
| Tray panel | Panel ends 6px after the last item | Height 346 → 304; removed ~40 px of empty space. |

## Blur / translucency

Design: `rgba(255,255,255,.92)` + `backdrop-filter: blur(30px)`.
`ui/backdrop.rs` selects the material per platform: macOS
`NSVisualEffectView`, Windows 11 Mica, Linux solid fill. **Only the Linux solid
fallback is implemented and verified.** The macOS/Windows native hooks are not
wired (no native adapters exist yet), so every platform currently paints
opaque white.

## Remaining differences (intentional or not fixed)

- Mock floating menu sits over an editor scene with a gradient; ours is a
  standalone 640×360 menu window over `#EDEDF2`.
- Remember row: mock shows a muted `✓`; ours is a functional `☐/☑` checkbox.
- Tray mock shows `⌘,` / `⌘Q` caps for 设置/退出; those shortcuts do not exist on
  Linux, so no caps are drawn. Tray "recent" rows use sample data in preview.
- Row label weight renders slightly heavier than the mock with the installed
  Noto Sans CJK at 13 px; font metrics are system-dependent.
- Settings window not re-compared in this pass beyond the round-2 fixes.

## Screenshots (docs/screenshots/)

Menu window captures 640×360: `e2e-secondary.png`, `e2e-remember-checked.png`,
`e2e-full.png`, `e2e-undo.png`, `e2e-nonsilent-1.png`, `e2e-nonsilent-2.png`,
`e2e-background-before.png`, `e2e-background-progress.png`.
Root captures 1280×800: `e2e-preview.png`, `e2e-settings.png`, `e2e-tray.png`.
