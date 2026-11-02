//! Presentation-only floating menus. The caller owns engine dispatch and native windows.
//!
//! Events use the placeholder timestamp 1000; replace it with the engine clock before
//! dispatch. A native host must keep the paste menu nonactivating (egui cannot do that).
use crate::{
    config::{Config, Format},
    engine::{Choice, Event, MenuKey, MenuKind, MenuModel},
};
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, Vec2};

const ACCENT: Color32 = Color32::from_rgb(91, 91, 214);
const TEXT: Color32 = Color32::from_rgb(29, 29, 31);
const SUB: Color32 = Color32::from_rgb(110, 110, 115);
const MUTED: Color32 = Color32::from_rgb(161, 161, 166);
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
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        model: &MenuModel,
        chars: usize,
        secondary: bool,
    ) -> Vec<Event> {
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
                // Reserve badge space rather than allowing a long content label to overlap it.
                let badge_width = if model.show_remember { 84.0 } else { 94.0 };
                let summary_clip = Rect::from_min_max(
                    summary.min,
                    Pos2::new(summary.right() - badge_width - 5.0, summary.bottom()),
                );
                ui.painter().with_clip_rect(summary_clip).text(
                    summary.left_center(),
                    Align2::LEFT_CENTER,
                    format!("剪贴板  {} · {} 字", model.content_label, grouped(chars)),
                    FontId::proportional(12.0),
                    SUB,
                );
                let badge = Rect::from_min_size(
                    Pos2::new(summary.right() - badge_width, summary.top() + 5.0),
                    Vec2::new(badge_width, 20.0),
                );
                let (label, color, bg) = if model.show_remember {
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
                line(
                    ui,
                    main.left() + 14.0,
                    main.right() - 14.0,
                    main.top() + 45.0,
                );
                let format_enabled = model.formats.iter().any(|f| f.enabled);
                for (index, (icon, label, hint, enabled)) in [
                    ("⌁", "粘贴原文", "1", model.original_enabled),
                    ("✦", "粘贴格式化文本", "›", format_enabled),
                    (
                        "↺",
                        "撤销上次转换",
                        if cfg!(target_os = "macos") {
                            "⌘Z"
                        } else {
                            "Ctrl+Z"
                        },
                        model.undo_enabled,
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
                            item.enabled,
                            item.muted,
                        );
                        if item.enabled && pointer_moved(ctx) && response.hovered() {
                            self.main = 1;
                            self.format = index;
                            self.in_submenu = true;
                        }
                        if item.enabled && response.clicked() {
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
                Rect::from_min_size(bounds.min + Vec2::splat(16.0), Vec2::new(300.0, 346.0)),
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
fn panel(ui: &egui::Ui, rect: Rect) {
    ui.painter().rect_filled(
        rect.translate(Vec2::new(0.0, 4.0)),
        12.0,
        Color32::from_black_alpha(12),
    );
    ui.painter().rect_filled(rect, 12.0, Color32::WHITE);
    ui.painter().rect_stroke(
        rect,
        12.0,
        Stroke::new(1.0_f32, LINE),
        egui::StrokeKind::Inside,
    );
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
    painter.text(
        rect.min + Vec2::new(18.0, 15.0),
        Align2::CENTER_CENTER,
        icon,
        FontId::proportional(13.0),
        color,
    );
    let hint_width = if hint.is_empty() {
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
    if !hint.is_empty() {
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
            original_enabled: true,
            undo_enabled: false,
            show_remember: true,
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
        assert!(text.contains(&config.full_menu_hotkey.as_str()));
        assert!(!text.contains(&config.hotkey.as_str()));
    }

    #[test]
    fn count_grouping() {
        assert_eq!(grouped(1284), "1,284");
        assert_eq!(grouped(12), "12");
    }
}
