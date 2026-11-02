//! Presentation-only floating menus. The caller owns engine dispatch and native windows.
//!
//! Events use the placeholder timestamp 1000; replace it with the engine clock before
//! dispatch. A native host must keep the paste menu nonactivating (egui cannot do that).
use crate::{
    config::{Config, Format},
    engine::{Choice, Event, MenuKey, MenuKind, MenuModel},
};
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, Vec2};

const ACCENT: Color32 = super::palette::ACCENT;
const TEXT: Color32 = super::palette::TEXT;
const SUB: Color32 = super::palette::SUB;
const MUTED: Color32 = super::palette::MUTED;
const LINE: Color32 = Color32::from_rgb(230, 230, 234);
const ROW: f32 = 30.0;
const TIME: u64 = 1000;

/// Transient highlight/expansion state, reset whenever the engine menu ID changes.
#[derive(Default)]
pub struct MenuState {
    id: Option<u64>,
    main: usize,
    format: usize,
    expanded: bool,
    in_submenu: bool,
    remember: bool,
}

impl MenuState {
    /// Draw a standalone menu canvas, returning only model-authorized engine events.
    /// `secondary` opens the format panel immediately; `MenuKind::Secondary` does too.
    pub fn show(&mut self, ctx: &egui::Context, model: &MenuModel, secondary: bool) -> Vec<Event> {
        if self.id != Some(model.id) {
            self.id = Some(model.id);
            self.main = match model.default_choice {
                Choice::Original => 0,
                Choice::Format(_) => 1,
                Choice::Undo => 2,
            };
            self.format = model
                .formats
                .iter()
                .position(|f| f.enabled && Choice::Format(f.format) == model.default_choice)
                .or_else(|| model.formats.iter().position(|f| f.enabled))
                .unwrap_or(0);
            self.expanded = secondary || model.kind == MenuKind::Secondary;
            self.in_submenu = self.expanded && self.main == 1;
            self.remember = model.remember;
        }
        let mut events = Vec::new();
        self.keyboard(ctx, model, &mut events);
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(Color32::from_rgb(237, 237, 242)))
            .show(ctx, |ui| {
                ui.style_mut().override_font_id = Some(FontId::proportional(13.0));
                ui.style_mut().visuals = egui::Visuals::light();
                let bounds = ui.max_rect();
                let sub_height = 40.0
                    + ROW * model.formats.len() as f32
                    + if model.show_remember { 42.0 } else { 0.0 };
                let (main, sub) = menu_rects(bounds, sub_height);
                panel(ui, main);
                let summary = Rect::from_min_size(
                    main.min + Vec2::new(16.0, 8.0),
                    Vec2::new(main.width() - 32.0, 30.0),
                );
                // Content restrictions take priority over decorative target tags.
                let summary_text = if model.loading {
                    model.content_label.clone()
                } else {
                    format!(
                        "{} · {} 字",
                        model.content_label,
                        grouped(model.source_chars)
                    )
                };
                // `.chip`: "剪贴板" in --sub, then the summary in --text.
                // Product rule: regular weight only, no bold UI text.
                let strong = FontId::proportional(12.0);
                let mut job = egui::text::LayoutJob::default();
                job.append(
                    "剪贴板  ",
                    0.0,
                    egui::TextFormat::simple(FontId::proportional(12.0), SUB),
                );
                job.append(
                    &summary_text,
                    0.0,
                    egui::TextFormat::simple(strong, if model.loading { SUB } else { TEXT }),
                );
                let galley = ui.painter().layout_job(job);
                let badge_width = if model.show_remember { 84.0 } else { 94.0 };
                // During a conversion the target locked at hotkey time must stay
                // visible (report v3 B-3); the summary is clipped instead.
                let show_badge =
                    model.converting || galley.size().x + badge_width + 8.0 <= summary.width();
                let summary_clip = Rect::from_min_max(
                    summary.min,
                    Pos2::new(
                        summary.right() - if show_badge { badge_width + 8.0 } else { 0.0 },
                        summary.bottom(),
                    ),
                );
                ui.painter().with_clip_rect(summary_clip).galley(
                    summary.left_center() - Vec2::new(0.0, galley.size().y / 2.0),
                    galley,
                    SUB,
                );
                if show_badge {
                    let badge = Rect::from_min_size(
                        Pos2::new(summary.right() - badge_width, summary.top() + 5.0),
                        Vec2::new(badge_width, 20.0),
                    );
                    let (label, color, bg) = if model.target_unrecognized {
                        (
                            "未识别目标".to_owned(),
                            Color32::from_rgb(178, 94, 9),
                            Color32::from_rgb(255, 244, 229),
                        )
                    } else {
                        (
                            if model.app.name.is_empty() {
                                "已识别目标".into()
                            } else {
                                model.app.name.clone()
                            },
                            ACCENT,
                            Color32::from_rgb(238, 238, 252),
                        )
                    };
                    ui.painter().rect_filled(badge, 5.0, bg);
                    ui.painter().with_clip_rect(badge.shrink(3.0)).text(
                        badge.center(),
                        Align2::CENTER_CENTER,
                        label,
                        FontId::proportional(11.0),
                        color,
                    );
                }
                line(
                    ui,
                    main.left() + 14.0,
                    main.right() - 14.0,
                    main.top() + 45.0,
                );
                let format_enabled = !model.converting && model.formats.iter().any(|f| f.enabled);
                for (index, (icon, label, hint, enabled)) in [
                    (
                        "⌁",
                        "粘贴原文",
                        "1",
                        model.original_enabled && !model.converting,
                    ),
                    ("✦", "粘贴格式化文本", "›", format_enabled),
                    (
                        "↺",
                        "撤销上次转换",
                        if cfg!(target_os = "macos") {
                            "⌘Z"
                        } else {
                            "Ctrl+Z"
                        },
                        model.undo_enabled && !model.converting,
                    ),
                ]
                .into_iter()
                .enumerate()
                {
                    let rect = row_rect(main, 51.0 + index as f32 * ROW);
                    let response = row(
                        ui,
                        rect,
                        ("main", model.id, index),
                        icon,
                        label,
                        hint,
                        self.main == index,
                        enabled,
                        false,
                    );
                    if enabled && pointer_moved(ctx) && response.hovered() {
                        self.main = index;
                        self.in_submenu = false;
                        if index == 1 {
                            self.expanded = true;
                        }
                    }
                    if response.clicked() && enabled {
                        self.main = index;
                        if index == 1 {
                            self.expanded = true;
                            self.in_submenu = true;
                        } else {
                            events.push(choice_event(
                                model,
                                if index == 0 {
                                    Choice::Original
                                } else {
                                    Choice::Undo
                                },
                            ));
                        }
                    }
                }
                line(
                    ui,
                    main.left() + 14.0,
                    main.right() - 14.0,
                    main.top() + 146.0,
                );
                if model.converting {
                    let progress = Rect::from_min_size(
                        main.min + Vec2::new(16.0, 151.0),
                        Vec2::new(main.width() - 100.0, 3.0),
                    );
                    ui.painter().rect_filled(progress, 1.5, LINE);
                    let phase = ctx.input(|input| input.time) as f32 % 1.0;
                    let segment = Rect::from_min_size(
                        progress.min + Vec2::new(phase * progress.width() * 0.7, 0.0),
                        Vec2::new(progress.width() * 0.3, 3.0),
                    );
                    ui.painter().rect_filled(segment, 1.5, ACCENT);
                    text(
                        ui,
                        main.min + Vec2::new(16.0, 165.0),
                        "正在转换… · esc 取消",
                        11.0,
                        SUB,
                    );
                    let cancel = Rect::from_min_size(
                        main.min + Vec2::new(main.width() - 70.0, 149.0),
                        Vec2::new(54.0, 24.0),
                    );
                    text(ui, cancel.left_center(), "取消", 12.0, ACCENT);
                    // Hit-test the raw press position instead of egui's hover
                    // state: when the menu maps under a still pointer, a single
                    // move + click arrive in one frame and `clicked()` can miss
                    // it (report v3 B-2). Cancelling on press is intentional.
                    let pressed_here = ctx.input(|input| {
                        input.events.iter().any(|event| {
                            matches!(event, egui::Event::PointerButton {
                                pos,
                                button: egui::PointerButton::Primary,
                                pressed: true,
                                ..
                            } if cancel.contains(*pos))
                        })
                    });
                    let clicked = ui
                        .interact(cancel, ui.id().with("cancel-conversion"), Sense::click())
                        .clicked();
                    if (pressed_here || clicked) && !events.contains(&Event::Cancel { t: TIME }) {
                        events.push(Event::Cancel { t: TIME });
                    }
                    ctx.request_repaint_after(std::time::Duration::from_millis(16));
                } else {
                    text(
                        ui,
                        main.min + Vec2::new(16.0, 160.0),
                        "↑↓ 选择 · ↵ 粘贴 · esc 取消",
                        11.0,
                        MUTED,
                    );
                    ui.painter().text(
                        main.min + Vec2::new(main.width() - 16.0, 160.0),
                        Align2::RIGHT_CENTER,
                        "CleanCopy",
                        FontId::proportional(11.0),
                        MUTED,
                    );
                }
                if self.expanded {
                    panel(ui, sub);
                    text(
                        ui,
                        sub.min + Vec2::new(16.0, 21.0),
                        "选择目标格式",
                        11.0,
                        MUTED,
                    );
                    for (index, item) in model.formats.iter().enumerate() {
                        let (icon, label) = format_label(item.format);
                        let response = row(
                            ui,
                            row_rect(sub, 34.0 + index as f32 * ROW),
                            ("format", model.id, index),
                            icon,
                            label,
                            &item.shortcut.to_string(),
                            self.main == 1 && self.format == index,
                            item.enabled && !model.converting,
                            item.muted,
                        );
                        if !model.converting
                            && item.enabled
                            && pointer_moved(ctx)
                            && response.hovered()
                        {
                            self.main = 1;
                            self.format = index;
                            self.in_submenu = true;
                        }
                        if !model.converting && item.enabled && response.clicked() {
                            events.push(choice_event(model, Choice::Format(item.format)));
                        }
                    }
                    if model.show_remember {
                        let y = 34.0 + model.formats.len() as f32 * ROW;
                        line(
                            ui,
                            sub.left() + 14.0,
                            sub.right() - 14.0,
                            sub.top() + y + 4.0,
                        );
                        let response = row(
                            ui,
                            row_rect(sub, y + 10.0),
                            ("remember", model.id, 0),
                            if self.remember { "☑" } else { "☐" },
                            "记住此 App 的选择",
                            "",
                            false,
                            true,
                            true,
                        );
                        if response.clicked() {
                            self.remember = !self.remember;
                            events.push(Event::Remember {
                                t: TIME,
                                id: model.id,
                                checked: self.remember,
                            });
                        }
                    }
                }
            });
        events
    }

    fn keyboard(&mut self, ctx: &egui::Context, model: &MenuModel, events: &mut Vec<Event>) {
        use egui::Key;
        let pressed = |key| ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, key));
        if pressed(Key::Escape) {
            events.push(Event::MenuKey {
                t: TIME,
                id: model.id,
                key: MenuKey::Escape,
            });
            return;
        }
        if model.converting || model.loading {
            return;
        }
        for (key, digit) in [
            (Key::Num1, 1),
            (Key::Num2, 2),
            (Key::Num3, 3),
            (Key::Num4, 4),
            (Key::Num5, 5),
            (Key::Num6, 6),
            (Key::Num7, 7),
            (Key::Num8, 8),
            (Key::Num9, 9),
        ] {
            if pressed(key) {
                let choice = if digit == 1 && model.original_enabled {
                    Some(Choice::Original)
                } else {
                    model
                        .formats
                        .iter()
                        .find(|f| f.enabled && f.shortcut == digit)
                        .map(|f| Choice::Format(f.format))
                };
                if let Some(choice) = choice {
                    events.push(choice_event(model, choice));
                }
                return;
            }
        }
        if pressed(Key::ArrowLeft) {
            self.in_submenu = false;
            self.expanded = false;
        }
        if pressed(Key::ArrowRight) && self.main == 1 && model.formats.iter().any(|f| f.enabled) {
            self.expanded = true;
            self.in_submenu = true;
        }
        let down = pressed(Key::ArrowDown);
        let up = pressed(Key::ArrowUp);
        if down || up {
            if self.in_submenu {
                let enabled: Vec<_> = model.formats.iter().map(|f| f.enabled).collect();
                self.format = next_enabled(self.format, &enabled, down);
            } else {
                let enabled = [
                    model.original_enabled,
                    model.formats.iter().any(|f| f.enabled),
                    model.undo_enabled,
                ];
                self.main = next_enabled(self.main, &enabled, down);
            }
        }
        if pressed(Key::Enter) {
            if self.in_submenu {
                if let Some(item) = model.formats.get(self.format).filter(|f| f.enabled) {
                    events.push(choice_event(model, Choice::Format(item.format)));
                }
            } else {
                match self.main {
                    0 if model.original_enabled => {
                        events.push(choice_event(model, Choice::Original))
                    }
                    1 if model.formats.iter().any(|f| f.enabled) => {
                        self.expanded = true;
                        self.in_submenu = true;
                    }
                    2 if model.undo_enabled => events.push(choice_event(model, Choice::Undo)),
                    _ => {}
                }
            }
        }
    }
}

/// Native integration, persistence, and pause duration are responsibilities of the host.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayAction {
    ShowMenu,
    Undo,
    SetEnabled(bool),
    Pause,
    Settings,
    Onboarding,
    Quit,
}

/// Draw the tray popover. Status changes return `SetEnabled`; `Pause` is reserved
/// for the one-hour pause row. The host dispatches and persists each action.
pub fn tray(ctx: &egui::Context, config: &Config, enabled: &mut bool) -> Option<TrayAction> {
    let mut action = None;
    egui::CentralPanel::default()
        .frame(egui::Frame::NONE.fill(Color32::from_rgb(237, 237, 242)))
        .show(ctx, |ui| {
            let bounds = ui.max_rect();
            let rect = fit_rect(
                Rect::from_min_size(bounds.min + Vec2::splat(16.0), Vec2::new(300.0, 304.0)),
                bounds,
            );
            panel(ui, rect);
            let status = row_rect(rect, 8.0);
            ui.painter().circle_filled(
                status.min + Vec2::new(13.0, 15.0),
                4.0,
                if *enabled {
                    Color32::from_rgb(48, 194, 90)
                } else {
                    MUTED
                },
            );
            text(
                ui,
                status.min + Vec2::new(27.0, 15.0),
                if *enabled {
                    "CleanCopy 运行中"
                } else {
                    "CleanCopy 已暂停"
                },
                13.0,
                TEXT,
            );
            let switch = Rect::from_min_size(
                Pos2::new(status.right() - 40.0, status.top() + 6.0),
                Vec2::new(30.0, 18.0),
            );
            ui.painter()
                .rect_filled(switch, 9.0, if *enabled { ACCENT } else { MUTED });
            ui.painter().circle_filled(
                Pos2::new(
                    if *enabled {
                        switch.right() - 9.0
                    } else {
                        switch.left() + 9.0
                    },
                    switch.center().y,
                ),
                7.0,
                Color32::WHITE,
            );
            if ui
                .interact(status, ui.id().with("tray-status"), Sense::click())
                .on_hover_text("启用 / 暂停 CleanCopy")
                .clicked()
            {
                *enabled = !*enabled;
                action = Some(TrayAction::SetEnabled(*enabled));
            }
            line(
                ui,
                rect.left() + 14.0,
                rect.right() - 14.0,
                rect.top() + 45.0,
            );
            text(
                ui,
                rect.min + Vec2::new(16.0, 63.0),
                "最近粘贴",
                11.0,
                MUTED,
            );
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|v| v.as_millis() as u64)
                .unwrap_or(0);
            if config.recent_pastes.is_empty() {
                text(
                    ui,
                    rect.min + Vec2::new(16.0, 88.0),
                    "暂无粘贴记录",
                    12.0,
                    SUB,
                );
            }
            for (index, recent) in config.recent_pastes.iter().take(2).enumerate() {
                let y = rect.top() + 86.0 + index as f32 * 23.0;
                let label = match recent.format {
                    Some(Format::WordRichText) => "标题/列表/表格",
                    Some(Format::ChineseNumbered) => "中文序号",
                    Some(Format::Markdown) => "Markdown",
                    Some(Format::PlainStripped) => "纯文本 · 去符号",
                    Some(Format::HtmlSource) => "HTML 源码",
                    None => "原文",
                };
                let clip = Rect::from_min_max(
                    Pos2::new(rect.left() + 16.0, y - 10.0),
                    Pos2::new(rect.right() - 80.0, y + 10.0),
                );
                ui.painter().with_clip_rect(clip).text(
                    clip.left_center(),
                    Align2::LEFT_CENTER,
                    format!(
                        "{} → {}",
                        if recent.app.name.is_empty() {
                            "未知 App"
                        } else {
                            &recent.app.name
                        },
                        label
                    ),
                    FontId::proportional(12.0),
                    SUB,
                );
                let minutes = now.saturating_sub(recent.timestamp) / 60_000;
                let ago = if minutes == 0 {
                    "刚刚".into()
                } else if minutes < 60 {
                    format!("{minutes} 分钟前")
                } else if minutes < 1440 {
                    format!("{} 小时前", minutes / 60)
                } else {
                    format!("{} 天前", minutes / 1440)
                };
                ui.painter().text(
                    Pos2::new(rect.right() - 16.0, y),
                    Align2::RIGHT_CENTER,
                    ago,
                    FontId::proportional(11.0),
                    MUTED,
                );
            }
            line(
                ui,
                rect.left() + 14.0,
                rect.right() - 14.0,
                rect.top() + 126.0,
            );
            for (index, (icon, label, hint, selected)) in [
                (
                    "✦",
                    "完整粘贴菜单",
                    config.full_menu_hotkey.as_str(),
                    TrayAction::ShowMenu,
                ),
                ("↺", "撤销上次转换", "", TrayAction::Undo),
                ("‖", "暂停 1 小时", "", TrayAction::Pause),
                ("⚙", "设置…", "", TrayAction::Settings),
                ("?", "使用引导…", "", TrayAction::Onboarding),
                ("", "退出 CleanCopy", "", TrayAction::Quit),
            ]
            .into_iter()
            .enumerate()
            {
                let y = 134.0 + index as f32 * ROW + if index >= 3 { 12.0 } else { 0.0 };
                let response = row(
                    ui,
                    row_rect(rect, y),
                    ("tray", 0, index),
                    icon,
                    label,
                    hint,
                    false,
                    true,
                    false,
                );
                if response.clicked() {
                    if selected == TrayAction::Pause {
                        *enabled = false;
                    }
                    action = Some(selected);
                }
            }
            line(
                ui,
                rect.left() + 14.0,
                rect.right() - 14.0,
                rect.top() + 230.0,
            );
        });
    action
}

fn choice_event(model: &MenuModel, choice: Choice) -> Event {
    Event::MenuChoice {
        t: TIME,
        id: model.id,
        choice,
    }
}
fn pointer_moved(ctx: &egui::Context) -> bool {
    ctx.input(|i| i.pointer.delta() != Vec2::ZERO)
}
fn next_enabled(current: usize, enabled: &[bool], forward: bool) -> usize {
    if enabled.is_empty() {
        return 0;
    }
    for offset in 1..=enabled.len() {
        let index = if forward {
            (current + offset) % enabled.len()
        } else {
            (current % enabled.len() + enabled.len() - offset) % enabled.len()
        };
        if enabled[index] {
            return index;
        }
    }
    current
}
fn menu_rects(bounds: Rect, sub_height: f32) -> (Rect, Rect) {
    let main = fit_rect(
        Rect::from_min_size(bounds.min + Vec2::new(32.0, 40.0), Vec2::new(320.0, 176.0)),
        bounds,
    );
    let right = main.right() + 4.0;
    let x = if right + 248.0 <= bounds.right() {
        right
    } else {
        main.left() - 252.0
    };
    let sub = fit_rect(
        Rect::from_min_size(
            Pos2::new(x, main.top() + 42.0),
            Vec2::new(248.0, sub_height),
        ),
        bounds,
    );
    (main, sub)
}
fn fit_rect(rect: Rect, bounds: Rect) -> Rect {
    let size = rect.size().min(bounds.size());
    let min = Pos2::new(
        rect.left().clamp(bounds.left(), bounds.right() - size.x),
        rect.top().clamp(bounds.top(), bounds.bottom() - size.y),
    );
    Rect::from_min_size(min, size)
}
fn row_rect(panel: Rect, y: f32) -> Rect {
    Rect::from_min_size(
        panel.min + Vec2::new(6.0, y),
        Vec2::new((panel.width() - 12.0).max(0.0), ROW),
    )
}
/// `.menu` from the design: `box-shadow: 0 12px 40px rgba(0,0,0,.14),
/// 0 2px 6px rgba(0,0,0,.06)`, 1px `rgba(0,0,0,.08)` border, 12px radius.
pub(crate) const MENU_SHADOWS: [egui::Shadow; 2] = [
    egui::Shadow {
        offset: [0, 12],
        blur: 40,
        spread: 0,
        color: Color32::from_black_alpha(36), // .14 × 255
    },
    egui::Shadow {
        offset: [0, 2],
        blur: 6,
        spread: 0,
        color: Color32::from_black_alpha(15), // .06 × 255
    },
];
pub(crate) const MENU_BORDER: Color32 = Color32::from_black_alpha(20); // .08 × 255
/// The design's panel is `rgba(255,255,255,.92)` over a 30px backdrop blur;
/// see `ui::backdrop` for the per-platform material (Linux: solid fallback).
pub(crate) const MENU_FILL: Color32 = super::backdrop::Backdrop::for_platform().panel_fill();
fn panel(ui: &egui::Ui, rect: Rect) {
    for shadow in MENU_SHADOWS {
        ui.painter().add(shadow.as_shape(rect, 12.0));
    }
    ui.painter().rect_filled(rect, 12.0, MENU_FILL);
    ui.painter().rect_stroke(
        rect,
        12.0,
        Stroke::new(1.0_f32, MENU_BORDER),
        egui::StrokeKind::Inside,
    );
}
/// Split a shortcut hint into design `.kbd span` key caps.
pub(crate) fn key_caps(hint: &str) -> Vec<String> {
    if hint.is_empty() || hint == "›" {
        return Vec::new();
    }
    if hint.contains('+') && hint.len() > 1 {
        return hint.split('+').map(str::to_owned).collect();
    }
    if hint.starts_with(['⌘', '⇧', '⌥', '⌃']) {
        return hint.chars().map(String::from).collect();
    }
    vec![hint.to_owned()]
}
/// Paint key caps right-aligned at `right`; returns the total width.
/// `.kbd span`: min 18×18, padding 0 4px, 1px `--line` border, 4px radius,
/// `#FAFAFB` fill, 500 11px `--mute` text, 3px gap. Selected rows use
/// `rgba(255,255,255,.15)` fill, `rgba(255,255,255,.3)` border, white text.
fn paint_key_caps(
    painter: &egui::Painter,
    right: f32,
    center_y: f32,
    caps: &[String],
    on: bool,
) -> f32 {
    let font = FontId::proportional(11.0);
    let (fill, stroke, color) = if on {
        (
            Color32::from_white_alpha(38),
            Color32::from_white_alpha(77),
            Color32::WHITE,
        )
    } else {
        (Color32::from_rgb(250, 250, 251), LINE, MUTED)
    };
    let mut x = right;
    for cap in caps.iter().rev() {
        let galley = painter.layout_no_wrap(cap.clone(), font.clone(), color);
        let width = (galley.size().x + 8.0).max(18.0);
        let rect =
            Rect::from_min_size(Pos2::new(x - width, center_y - 9.0), Vec2::new(width, 18.0));
        painter.rect_filled(rect, 4.0, fill);
        painter.rect_stroke(
            rect,
            4.0,
            Stroke::new(1.0_f32, stroke),
            egui::StrokeKind::Inside,
        );
        painter.galley(rect.center() - galley.size() / 2.0, galley, color);
        x -= width + 3.0;
    }
    right - (x + 3.0)
}
fn line(ui: &egui::Ui, x1: f32, x2: f32, y: f32) {
    ui.painter().line_segment(
        [Pos2::new(x1, y), Pos2::new(x2, y)],
        Stroke::new(1.0_f32, LINE),
    );
}
fn text(ui: &egui::Ui, pos: Pos2, label: &str, size: f32, color: Color32) {
    ui.painter().text(
        pos,
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(size),
        color,
    );
}
#[allow(clippy::too_many_arguments)]
fn row(
    ui: &mut egui::Ui,
    rect: Rect,
    id: (&str, u64, usize),
    icon: &str,
    label: &str,
    hint: &str,
    selected: bool,
    enabled: bool,
    muted: bool,
) -> egui::Response {
    let response = ui.interact(
        rect,
        ui.id().with(id),
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    let highlighted = enabled && (selected || response.hovered());
    let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
    if highlighted {
        painter.rect_filled(rect, 6.0, ACCENT);
    }
    let color = if !enabled {
        MUTED
    } else if highlighted {
        Color32::WHITE
    } else if muted {
        SUB
    } else {
        TEXT
    };
    // `.item .ic`: --sub normally, rgba(255,255,255,.8) on the selected row.
    let icon_color = if !enabled {
        MUTED
    } else if highlighted {
        Color32::from_white_alpha(204)
    } else {
        SUB
    };
    painter.text(
        rect.min + Vec2::new(18.0, 15.0),
        Align2::CENTER_CENTER,
        icon,
        FontId::proportional(13.0),
        icon_color,
    );
    let caps = key_caps(hint);
    let hint_width = if !caps.is_empty() {
        paint_key_caps(
            &painter,
            rect.right() - 10.0,
            rect.center().y,
            &caps,
            highlighted,
        ) + 12.0
    } else if hint.is_empty() {
        0.0
    } else {
        (hint.chars().count() as f32 * 6.0 + 12.0).min(rect.width() * 0.45)
    };
    let label_clip = Rect::from_min_max(
        rect.min + Vec2::new(36.0, 0.0),
        Pos2::new(
            (rect.right() - hint_width - 12.0).max(rect.left() + 36.0),
            rect.bottom(),
        ),
    );
    painter.with_clip_rect(label_clip).text(
        rect.min + Vec2::new(36.0, 15.0),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(13.0),
        color,
    );
    if !hint.is_empty() && caps.is_empty() {
        painter.text(
            rect.right_center() - Vec2::new(10.0, 0.0),
            Align2::RIGHT_CENTER,
            hint,
            FontId::proportional(11.0),
            if highlighted { Color32::WHITE } else { MUTED },
        );
    }
    response
}
pub(crate) fn format_label(format: Format) -> (&'static str, &'static str) {
    match format {
        Format::ChineseNumbered => ("¶", "纯文本 · 中文序号"),
        Format::WordRichText => ("W", "Word / WPS 富文本"),
        Format::Markdown => ("#", "Markdown 原样"),
        Format::PlainStripped => ("T", "纯文本 · 去符号"),
        Format::HtmlSource => ("<>", "HTML 源码"),
    }
}
fn grouped(value: usize) -> String {
    let digits = value.to_string();
    let mut result = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            result.push(',');
        }
        result.push(digit);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hints_split_into_design_key_caps() {
        assert_eq!(key_caps("1"), ["1"]);
        assert_eq!(key_caps("Ctrl+Z"), ["Ctrl", "Z"]);
        assert_eq!(key_caps("Ctrl+Alt+V"), ["Ctrl", "Alt", "V"]);
        assert_eq!(key_caps("⌘Z"), ["⌘", "Z"]);
        assert!(key_caps("›").is_empty());
        assert!(key_caps("").is_empty());
    }
    #[test]
    fn menu_shadow_matches_design_css() {
        // 0 12px 40px rgba(0,0,0,.14), 0 2px 6px rgba(0,0,0,.06)
        assert_eq!(MENU_SHADOWS[0].offset, [0, 12]);
        assert_eq!(MENU_SHADOWS[0].blur, 40);
        assert_eq!(MENU_SHADOWS[0].color.a(), (0.14_f32 * 255.0).round() as u8);
        assert_eq!(MENU_SHADOWS[1].offset, [0, 2]);
        assert_eq!(MENU_SHADOWS[1].blur, 6);
        assert_eq!(MENU_SHADOWS[1].color.a(), (0.06_f32 * 255.0).round() as u8);
        assert_eq!(MENU_BORDER.a(), (0.08_f32 * 255.0).round() as u8);
    }
    #[test]
    fn navigation_wraps_and_skips_disabled() {
        assert_eq!(next_enabled(0, &[true, false, true], true), 2);
        assert_eq!(next_enabled(0, &[true, false, true], false), 2);
        assert_eq!(next_enabled(2, &[true, false, true], true), 0);
        assert_eq!(next_enabled(1, &[false, false], true), 1);
        assert_eq!(next_enabled(0, &[], false), 0);
    }
    #[test]
    fn panels_fit_and_submenu_flips() {
        let bounds = Rect::from_min_size(Pos2::ZERO, Vec2::new(960.0, 600.0));
        let (main, sub) = menu_rects(bounds, 202.0);
        assert_eq!(main.width(), 320.0);
        assert_eq!(sub.width(), 248.0);
        assert!(sub.left() > main.right());
        let shifted = Rect::from_min_size(Pos2::new(400.0, 0.0), Vec2::new(570.0, 300.0));
        let (main, sub) = menu_rects(shifted, 202.0);
        assert!(bounds.contains_rect(main));
        assert!(shifted.contains_rect(sub));
        assert!(sub.left() <= main.left());
        let tiny = Rect::from_min_size(Pos2::ZERO, Vec2::new(200.0, 150.0));
        let (main, sub) = menu_rects(tiny, 250.0);
        assert!(tiny.contains_rect(main));
        assert!(tiny.contains_rect(sub));
    }
    #[test]
    fn keyboard_right_enter_and_escape() {
        let ctx = egui::Context::default();
        let model = MenuModel {
            id: 7,
            kind: MenuKind::Full,
            app: Default::default(),
            formats: vec![crate::engine::FormatItem {
                format: Format::Markdown,
                shortcut: 2,
                enabled: true,
                muted: false,
            }],
            default_choice: Choice::Format(Format::Markdown),
            content_label: "Markdown".into(),
            source_chars: 42,
            converting: false,
            original_enabled: true,
            undo_enabled: false,
            show_remember: true,
            target_unrecognized: true,
            loading: false,
            remember: false,
        };
        let mut state = MenuState {
            main: 1,
            ..Default::default()
        };
        for (key, expected) in [
            (egui::Key::ArrowRight, None),
            (
                egui::Key::Enter,
                Some(choice_event(&model, Choice::Format(Format::Markdown))),
            ),
            (
                egui::Key::Escape,
                Some(Event::MenuKey {
                    t: TIME,
                    id: 7,
                    key: MenuKey::Escape,
                }),
            ),
        ] {
            let input = egui::RawInput {
                events: vec![egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                ..Default::default()
            };
            let mut events = Vec::new();
            let _ = ctx.run(input, |_| state.keyboard(&ctx, &model, &mut events));
            assert_eq!(events, expected.into_iter().collect::<Vec<_>>());
        }
    }
    #[test]
    fn tray_status_and_timed_pause_return_distinct_actions() {
        let ctx = egui::Context::default();
        let config = Config::default();
        let mut enabled = true;
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(720.0, 520.0))),
                ..Default::default()
            },
            |ctx| {
                let _ = tray(ctx, &config, &mut enabled);
            },
        );
        for (position, expected, expected_enabled) in [
            (Pos2::new(80.0, 39.0), TrayAction::SetEnabled(false), false),
            (Pos2::new(80.0, 39.0), TrayAction::SetEnabled(true), true),
            (Pos2::new(80.0, 225.0), TrayAction::Pause, false),
        ] {
            for pressed in [true, false] {
                let input = egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(720.0, 520.0))),
                    events: vec![
                        egui::Event::PointerMoved(position),
                        egui::Event::PointerButton {
                            pos: position,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                    ..Default::default()
                };
                let mut action = None;
                let _ = ctx.run(input, |ctx| action = tray(ctx, &config, &mut enabled));
                if !pressed {
                    assert_eq!(action, Some(expected));
                    assert_eq!(enabled, expected_enabled);
                }
            }
        }
    }

    #[test]
    fn tray_displays_the_full_menu_shortcut_not_the_paste_shortcut() {
        let ctx = egui::Context::default();
        let config = Config {
            hotkey: "Ctrl+Shift+P".into(),
            full_menu_hotkey: "Ctrl+Alt+M".into(),
            ..Default::default()
        };
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(720.0, 520.0))),
                ..Default::default()
            },
            |ctx| {
                let _ = tray(ctx, &config, &mut true);
            },
        );
        let text: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::epaint::Shape::Text(text) => Some(text.galley.job.text.as_str()),
                _ => None,
            })
            .collect();
        // The shortcut is drawn as key caps: Ctrl · Alt · M, never P.
        let caps = text.join("|");
        assert!(caps.contains("M|Alt|Ctrl"), "{caps}"); // painted right-to-left
        assert!(!caps.contains("P|Shift"), "{caps}");
    }

    fn render_model() -> MenuModel {
        MenuModel {
            id: 123,
            kind: MenuKind::Full,
            app: Default::default(),
            formats: vec![],
            default_choice: Choice::Original,
            content_label: "Markdown".into(),
            source_chars: 1234,
            converting: false,
            original_enabled: true,
            undo_enabled: true,
            show_remember: true,
            target_unrecognized: true,
            loading: false,
            remember: false,
        }
    }

    #[test]
    fn item_labels_are_painted_once_in_regular_weight() {
        let ctx = egui::Context::default();
        super::super::fonts::install(&ctx);
        let model = render_model();
        let mut state = MenuState::default();
        let mut output = None;
        for _ in 0..3 {
            output = Some(ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(960.0, 600.0))),
                    ..Default::default()
                },
                |ctx| {
                    state.show(ctx, &model, false);
                },
            ));
        }
        let mut hits = Vec::new();
        for shape in output.unwrap().shapes {
            if let egui::epaint::Shape::Text(text) = shape.shape {
                if text.galley.job.text == "粘贴原文" {
                    hits.push(text.galley.job.sections[0].format.font_id.clone());
                }
            }
        }
        eprintln!("label shapes: {hits:?}");
        assert_eq!(hits.len(), 1, "{hits:?}");
        assert_eq!(hits[0].family, egui::FontFamily::Proportional);
    }

    #[test]
    fn summary_renders_model_count_and_hides_badge_before_clipping_warning() {
        let ctx = egui::Context::default();
        super::super::fonts::install(&ctx);
        for (label, chars, badge) in [
            ("Markdown", 1234, true),
            ("文本过大，只能粘贴原文", 6_000_000, false),
        ] {
            let mut model = render_model();
            model.content_label = label.into();
            model.source_chars = chars;
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(960.0, 600.0))),
                    ..Default::default()
                },
                |ctx| {
                    MenuState::default().show(ctx, &model, false);
                },
            );
            let mut found = false;
            let mut found_badge = false;
            for shape in output.shapes {
                if let egui::epaint::Shape::Text(text) = shape.shape {
                    if text.galley.job.text == format!("剪贴板  {label} · {} 字", grouped(chars))
                    {
                        assert!(shape
                            .clip_rect
                            .contains_rect(Rect::from_min_size(text.pos, text.galley.size())));
                        found = true;
                    }
                    found_badge |= text.galley.job.text == "未识别目标";
                }
            }
            assert!(found);
            assert_eq!(found_badge, badge);
        }
    }

    /// Report v3 B-3: a large conversion (long count) must keep painting the
    /// target badge locked at hotkey time, recognised or not.
    #[test]
    fn converting_always_paints_the_locked_target_badge() {
        let ctx = egui::Context::default();
        super::super::fonts::install(&ctx);
        for (unrecognized, expected) in [(true, "未识别目标"), (false, "Xmessage")] {
            for (label, screen) in [
                ("Markdown", Vec2::new(640.0, 360.0)),
                ("Markdown", Vec2::new(960.0, 600.0)),
                ("文本过大，只能粘贴原文", Vec2::new(640.0, 360.0)),
            ] {
                let mut model = render_model();
                model.converting = true;
                model.show_remember = false;
                model.content_label = label.into();
                model.source_chars = 5_130_013;
                model.target_unrecognized = unrecognized;
                model.app.name = "Xmessage".into();
                let output = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, screen)),
                        ..Default::default()
                    },
                    |ctx| {
                        MenuState::default().show(ctx, &model, false);
                    },
                );
                let painted = output.shapes.iter().any(|shape| {
                    matches!(&shape.shape, egui::epaint::Shape::Text(t) if t.galley.job.text == expected)
                });
                assert!(painted, "{expected} missing for {label}");
            }
        }
    }

    #[test]
    fn converting_blocks_choice_keys_but_keeps_escape_and_cancel_button() {
        let ctx = egui::Context::default();
        let mut model = render_model();
        model.converting = true;
        let mut state = MenuState::default();
        for key in [egui::Key::Num1, egui::Key::Enter, egui::Key::Escape] {
            let mut events = Vec::new();
            let _ = ctx.run(
                egui::RawInput {
                    events: vec![egui::Event::Key {
                        key,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    }],
                    ..Default::default()
                },
                |ctx| state.keyboard(ctx, &model, &mut events),
            );
            assert_eq!(events.len(), usize::from(key == egui::Key::Escape));
        }
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(960.0, 600.0))),
                ..Default::default()
            },
            |ctx| {
                state.show(ctx, &model, false);
            },
        );
        for pressed in [true, false] {
            let pos = Pos2::new(300.0, 200.0);
            let mut events = Vec::new();
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(960.0, 600.0))),
                    events: vec![
                        egui::Event::PointerMoved(pos),
                        egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                    ..Default::default()
                },
                |ctx| events = state.show(ctx, &model, false),
            );
            if pressed {
                assert_eq!(events, vec![Event::Cancel { t: TIME }]);
            }
        }
    }

    /// Report v3 B-2: the menu maps under the pointer and the very first
    /// input is one move plus a click, all in one frame on a fresh context
    /// (no hover from an earlier frame). Also without any move at all.
    #[test]
    fn cancel_works_with_single_move_then_click_and_no_prior_hover() {
        let mut model = render_model();
        model.converting = true;
        let screen = Rect::from_min_size(Pos2::ZERO, Vec2::new(960.0, 600.0));
        let pos = Pos2::new(300.0, 200.0);
        let click = |moved: bool| {
            let mut input = Vec::new();
            if moved {
                input.push(egui::Event::PointerMoved(pos));
            }
            for pressed in [true, false] {
                input.push(egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                });
            }
            input
        };
        for moved in [true, false] {
            let ctx = egui::Context::default();
            let mut state = MenuState::default();
            let mut events = Vec::new();
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(screen),
                    events: click(moved),
                    ..Default::default()
                },
                |ctx| events = state.show(ctx, &model, false),
            );
            assert_eq!(events, vec![Event::Cancel { t: TIME }], "moved={moved}");
        }
    }

    #[test]
    fn count_grouping() {
        assert_eq!(grouped(1284), "1,284");
        assert_eq!(grouped(12), "12");
    }
}
