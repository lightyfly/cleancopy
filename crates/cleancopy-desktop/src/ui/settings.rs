//! Self-contained settings view. The caller owns persistence and window setup.
use crate::config::{AppIdentity, AppRule, Config, Fallback, Format, SecondaryItem};
use cleancopy_core::{
    render_html, render_markdown, render_numbered, render_plain, render_word_html, NumberingStyle,
};
use eframe::egui::{self, Color32, FontId, RichText, Stroke, Vec2};

const ACCENT: Color32 = super::palette::ACCENT;
const LINE: Color32 = Color32::from_rgb(230, 230, 234);
const SUB: Color32 = super::palette::SUB;
const MUTED: Color32 = super::palette::MUTED;
const TEXT: Color32 = super::palette::TEXT;
const SAMPLE: &str = "# 项目周报\n## 本周进展\n- 完成 **剪贴板监听**\n- 接入 Word 富文本\n  - 标题映射 标题1/2\n  - 表格与代码块\n## 下周计划\n1. Windows 托盘\n2. 识别失败兜底";
const FORMATS: [Format; 5] = [
    Format::ChineseNumbered,
    Format::WordRichText,
    Format::Markdown,
    Format::PlainStripped,
    Format::HtmlSource,
];

/// Transient editor state only; no configuration is loaded or saved here.
pub struct SettingsState {
    markdown: String,
    preview: Format,
    selected_app: String,
    adding: bool,
    app_name: String,
    app_identifier: String,
    identifier_kind: usize,
    app_format: Format,
    dragging: Option<usize>,
    recording: Option<HotkeyTarget>,
    hotkey_error: Option<String>,
    scroll_to_hotkey: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HotkeyTarget {
    Paste,
    FullMenu,
}

impl Default for SettingsState {
    fn default() -> Self {
        Self {
            markdown: SAMPLE.into(),
            preview: Format::ChineseNumbered,
            selected_app: "TextEdit / Notepad / gedit".into(),
            adding: false,
            app_name: String::new(),
            app_identifier: String::new(),
            identifier_kind: 0,
            app_format: Format::ChineseNumbered,
            dragging: None,
            recording: None,
            scroll_to_hotkey: false,
            hotkey_error: None,
        }
    }
}

impl SettingsState {
    /// Opened from onboarding's "更改快捷键": scroll to the hotkey item and
    /// start recording the paste hotkey.
    pub fn focus_hotkey(&mut self) {
        self.scroll_to_hotkey = true;
        self.recording = Some(HotkeyTarget::Paste);
        self.hotkey_error = None;
    }
    /// Returns true only for configuration edits, not preview or selection edits.
    pub fn show(&mut self, ctx: &egui::Context, config: &mut Config) -> bool {
        let mut changed = self.record_hotkey(ctx, config);
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(Color32::WHITE))
            .show(ctx, |ui| {
                ui.style_mut().visuals = egui::Visuals::light();
                ui.style_mut().visuals.selection.bg_fill = ACCENT;
                ui.style_mut().visuals.override_text_color = Some(TEXT);
                ui.style_mut().spacing.item_spacing = Vec2::new(8.0, 5.0);
                ui.style_mut().override_font_id = Some(FontId::proportional(13.0));
                let origin = ui.max_rect().min;
                let size = ui.max_rect().size().max(Vec2::new(MIN_WIDTH, 480.0));
                let window = egui::Rect::from_min_size(origin, size);
                ui.set_min_size(window.size());
                let title = egui::Rect::from_min_size(origin, Vec2::new(size.x, 44.0));
                ui.painter().text(
                    title.center(),
                    egui::Align2::CENTER_CENTER,
                    "CleanCopy 设置",
                    FontId::proportional(13.0),
                    TEXT,
                );
                // Traffic-light buttons only on macOS; Linux/Windows use
                // native window decorations (report v4 M-6).
                super::paint_mac_window_buttons(ui, origin);
                ui.painter()
                    .hline(title.x_range(), title.bottom(), Stroke::new(1.0_f32, LINE));
                let left = egui::Rect::from_min_size(
                    origin + Vec2::new(0.0, 44.0),
                    Vec2::new(LEFT_WIDTH, size.y - 44.0),
                );
                ui.painter()
                    .vline(left.right(), left.y_range(), Stroke::new(1.0_f32, LINE));
                let mut left_ui = ui.new_child(
                    egui::UiBuilder::new()
                        .id_salt("settings_left")
                        .max_rect(left.shrink2(Vec2::new(20.0, 16.0))),
                );
                // The left column scrolls on its own; the preview stays fixed.
                egui::ScrollArea::vertical()
                    .id_salt("settings_controls")
                    .auto_shrink([false, false])
                    .max_height(left.height() - 32.0)
                    .show(&mut left_ui, |ui| {
                        ui.set_width(LEFT_WIDTH - 52.0);
                        changed |= self.controls(ui, config);
                    });
                let right = egui::Rect::from_min_size(
                    origin + Vec2::new(LEFT_WIDTH, 44.0),
                    Vec2::new(size.x - LEFT_WIDTH, size.y - 44.0),
                );
                ui.painter()
                    .rect_filled(right, 0.0, Color32::from_rgb(250, 250, 251));
                changed |= self.preview_ui(ui, right, config);
            });
        changed |= self.add_dialog(ctx, config);
        changed
    }

    fn record_hotkey(&mut self, ctx: &egui::Context, config: &mut Config) -> bool {
        let Some(target) = self.recording else {
            return false;
        };
        let mut pressed = None;
        ctx.input_mut(|input| {
            input.events.retain(|event| match event {
                egui::Event::Key {
                    key,
                    pressed: true,
                    repeat: false,
                    modifiers,
                    ..
                } => {
                    pressed.get_or_insert((*key, *modifiers));
                    false
                }
                egui::Event::Key { .. } | egui::Event::Text(_) => false,
                _ => true,
            });
        });
        let Some((key, modifiers)) = pressed else {
            return false;
        };
        if key == egui::Key::Escape && modifiers.is_none() {
            self.recording = None;
            self.hotkey_error = None;
            return false;
        }
        let Some(shortcut) = recorded_hotkey(key, modifiers) else {
            self.hotkey_error = Some(
                "请使用 Ctrl、Alt 或 Super 修饰键（可加 Shift）加字母、数字、F1–F12 或导航键"
                    .into(),
            );
            return false;
        };
        let (current, other) = match target {
            HotkeyTarget::Paste => (&mut config.hotkey, &config.full_menu_hotkey),
            HotkeyTarget::FullMenu => (&mut config.full_menu_hotkey, &config.hotkey),
        };
        if same_hotkey(&shortcut, other) {
            self.hotkey_error = Some("两个快捷键不能相同，请选择不同的组合键".into());
            return false;
        }
        let changed = *current != shortcut;
        *current = shortcut;
        self.recording = None;
        self.hotkey_error = None;
        changed
    }

    fn controls(&mut self, ui: &mut egui::Ui, config: &mut Config) -> bool {
        let mut changed = false;
        ui.horizontal(|ui| {
            section(ui, "目标 App → 粘贴格式");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .link(RichText::new("＋ 添加 App").color(ACCENT).size(11.0))
                    .clicked()
                {
                    self.adding = true;
                }
            });
        });
        card(ui, |ui| {
            {
                {
                    for (name, indices) in grouped_rules(&config.app_rules) {
                        let mut format = config.app_rules[indices[0]].format;
                        let mixed = indices
                            .iter()
                            .any(|&i| config.app_rules[i].format != format);
                        row_selected(ui, 31.0, self.selected_app == name, |ui| {
                            badge(ui, &name);
                            let label = display_name(&name);
                            if app_name(ui, label, self.selected_app == name)
                                .on_hover_text(format!(
                                    "{} · {} 个匹配标识{}",
                                    name,
                                    indices.len(),
                                    if mixed {
                                        " · 格式不同，选择后统一"
                                    } else {
                                        ""
                                    }
                                ))
                                .clicked()
                            {
                                self.selected_app = name.clone();
                                self.preview = format;
                            }
                            if format_combo(ui, ("app_format", &name), &mut format) {
                                changed |= set_group_format(&mut config.app_rules, &name, format);
                                self.selected_app = name.clone();
                                self.preview = format;
                            }
                        });
                    }
                }
            }
            row_selected(ui, 31.0, self.selected_app == "其他 App", |ui| {
                badge(ui, "?");
                if app_name(ui, "其他 App", self.selected_app == "其他 App").clicked() {
                    self.selected_app = "其他 App".into();
                    if let Fallback::Fixed(format) = config.fallback {
                        self.preview = format;
                    }
                }
                let old = config.fallback;
                let mut options = vec![(Fallback::Popup, "弹出格式菜单")];
                options.extend(FORMATS.map(|f| (Fallback::Fixed(f), format_name(f))));
                dropdown(ui, "fallback", &mut config.fallback, &options, 155.0);
                if old != config.fallback {
                    changed = true;
                    self.selected_app = "其他 App".into();
                    if let Fallback::Fixed(format) = config.fallback {
                        self.preview = format;
                    }
                }
            });
        });
        ui.add_space(8.0);
        section(ui, "中文序号风格");
        changed |= segmented(
            ui,
            "numbering_style",
            &[
                (NumberingStyle::Chinese, "一、（一）1.（1）"),
                (NumberingStyle::ChineseCompact, "一、1.（1）"),
                (NumberingStyle::Decimal, "1. 1.1 1.1.1"),
            ],
            &mut config.numbering_style,
            ui.available_width(),
        );
        ui.add_space(8.0);
        section(ui, "二级格式菜单（勾选显示 · 拖动排序）");
        let mut movement = None;
        let mut hover_target = None;
        let mut shortcut = 2;
        card(ui, |ui| {
            for (index, item) in config.secondary_items.iter_mut().enumerate() {
                row(ui, 31.0, |ui| {
                    let grip = ui
                        .add(
                            egui::Label::new(RichText::new("⋮⋮").color(MUTED))
                                .sense(egui::Sense::drag()),
                        )
                        .on_hover_text("拖动以调整顺序");
                    if grip.drag_started() {
                        self.dragging = Some(index);
                    }
                    changed |= check(ui, &mut item.enabled, format_name(item.format)).changed();
                    ui.add(
                        egui::Label::new(RichText::new(format_name(item.format)).size(13.0))
                            .truncate(),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if item.enabled {
                            key_cap(ui, &shortcut.to_string());
                            shortcut += 1;
                        }
                    });
                    if self.dragging.is_some() && ui.rect_contains_pointer(ui.max_rect()) {
                        hover_target = Some(index);
                        ui.painter().hline(
                            ui.max_rect().x_range(),
                            ui.max_rect().top(),
                            Stroke::new(2.0_f32, ACCENT),
                        );
                    }
                });
            }
        });
        if ui.input(|i| i.pointer.any_released()) {
            if let (Some(from), Some(to)) = (self.dragging.take(), hover_target) {
                movement = Some((from, to));
            }
        }
        if let Some((from, to)) = movement {
            changed |= reorder(&mut config.secondary_items, from, to);
        }
        ui.add_space(8.0);
        let heading = section(ui, "快捷键（点击后按下新的组合键）");
        if std::mem::take(&mut self.scroll_to_hotkey) {
            heading.scroll_to_me(Some(egui::Align::TOP));
        }
        ui.horizontal(|ui| {
            let label = "静默模式 · 按目标 App 规则直接粘贴";
            ui.label(label);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                changed |= toggle(ui, &mut config.silent_mode, label)
                    .on_hover_text("关闭后先显示格式菜单；其他 App 的兜底规则仍然生效")
                    .changed();
            });
        });
        for (target, label, hotkey) in [
            (HotkeyTarget::Paste, "智能粘贴", &config.hotkey),
            (
                HotkeyTarget::FullMenu,
                "完整粘贴菜单",
                &config.full_menu_hotkey,
            ),
        ] {
            ui.horizontal(|ui| {
                ui.label(label);
                let recording = self.recording == Some(target);
                let text = if recording {
                    "请按组合键…（Esc 取消）".to_owned()
                } else {
                    hotkey_labels(hotkey).join(" + ")
                };
                let button = egui::Button::new(RichText::new(text).size(12.0))
                    .fill(Color32::from_rgb(247, 247, 248))
                    .stroke(Stroke::new(1.0_f32, LINE))
                    .corner_radius(5.0);
                if ui
                    .with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add(button)
                    })
                    .inner
                    .clicked()
                {
                    self.recording = Some(target);
                    self.hotkey_error = None;
                }
            });
        }
        if let Some(error) = &self.hotkey_error {
            ui.colored_label(Color32::DARK_RED, error);
        }
        changed
    }

    fn preview_ui(&mut self, parent: &mut egui::Ui, rect: egui::Rect, config: &mut Config) -> bool {
        let mut changed = false;
        let header = egui::Rect::from_min_size(rect.min, Vec2::new(rect.width(), 44.0));
        let mut ui = parent.new_child(
            egui::UiBuilder::new()
                .id_salt("preview_header")
                .max_rect(header.shrink2(Vec2::new(20.0, 8.0)))
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        ui.label(RichText::new("实时预览"));
        ui.label(RichText::new("·").color(MUTED));
        ui.label(RichText::new(format!("当前：{}", display_name(&self.selected_app))).color(SUB));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // Other formats (去符号 / HTML) are reached by selecting an App row.
            let mut tab = match self.preview {
                Format::WordRichText => Format::WordRichText,
                Format::Markdown => Format::Markdown,
                _ => Format::ChineseNumbered,
            };
            let before = tab;
            segmented(
                ui,
                "preview_format",
                &[
                    (Format::ChineseNumbered, "纯文本"),
                    (Format::WordRichText, "Word"),
                    (Format::Markdown, "Markdown"),
                ],
                &mut tab,
                220.0,
            );
            if tab != before {
                self.preview = tab;
            }
        });
        parent.painter().hline(
            header.x_range(),
            header.bottom(),
            Stroke::new(1.0_f32, LINE),
        );
        let pane_height = rect.height() - 80.0;
        let pane_width = (rect.width() / 2.0).floor();
        let source = egui::Rect::from_min_size(
            rect.min + Vec2::new(0.0, 44.0),
            Vec2::new(pane_width, pane_height),
        );
        let output = egui::Rect::from_min_max(
            egui::pos2(source.right(), source.top()),
            egui::pos2(rect.right(), source.bottom()),
        );
        parent.painter().rect_filled(output, 0.0, Color32::WHITE);
        parent
            .painter()
            .vline(output.left(), output.y_range(), Stroke::new(1.0_f32, LINE));
        let mut source_ui = parent.new_child(
            egui::UiBuilder::new()
                .id_salt("source_pane")
                .max_rect(source.shrink2(Vec2::new(20.0, 16.0))),
        );
        section(&mut source_ui, "示例 MARKDOWN（可编辑）");
        egui::ScrollArea::vertical()
            .id_salt("markdown_editor_scroll")
            .show(&mut source_ui, |ui| {
                ui.add(
                    egui::TextEdit::multiline(&mut self.markdown)
                        .font(FontId::monospace(12.0))
                        .frame(false)
                        .desired_width(pane_width - 40.0)
                        .desired_rows(26),
                );
            });
        let mut output_ui = parent.new_child(
            egui::UiBuilder::new()
                .id_salt("output_pane")
                .max_rect(output.shrink2(Vec2::new(20.0, 16.0))),
        );
        section(&mut output_ui, "粘贴效果");
        egui::ScrollArea::both()
            .id_salt("rendered_preview")
            .show(&mut output_ui, |ui| {
                ui.set_min_width(output.width() - 42.0);
                match self.preview {
                    Format::WordRichText => rich_preview(ui, &render_word_html(&self.markdown)),
                    Format::ChineseNumbered => {
                        indented_text(ui, &render_numbered(&self.markdown, config.numbering_style));
                    }
                    Format::PlainStripped => {
                        ui.add(
                            egui::Label::new(
                                RichText::new(render_plain(&self.markdown))
                                    .size(13.0)
                                    .line_height(Some(24.0)),
                            )
                            .wrap(),
                        );
                    }
                    Format::Markdown => {
                        ui.add(
                            egui::Label::new(
                                RichText::new(render_markdown(&self.markdown))
                                    .monospace()
                                    .size(12.0),
                            )
                            .wrap(),
                        );
                    }
                    Format::HtmlSource => {
                        ui.add(
                            egui::Label::new(
                                RichText::new(render_html(&self.markdown))
                                    .monospace()
                                    .size(12.0),
                            )
                            .wrap(),
                        );
                    }
                }
            });
        let footer = egui::Rect::from_min_size(
            egui::pos2(rect.left(), rect.bottom() - 36.0),
            Vec2::new(rect.width(), 36.0),
        );
        parent
            .painter()
            .hline(footer.x_range(), footer.top(), Stroke::new(1.0_f32, LINE));
        let mut ui = parent.new_child(
            egui::UiBuilder::new()
                .id_salt("settings_footer")
                .max_rect(footer.shrink2(Vec2::new(20.0, 4.0)))
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        ui.label(RichText::new("改动即时生效").size(12.0).color(MUTED));
        ui.label(RichText::new("·").color(MUTED));
        if ui.link(RichText::new("恢复默认").size(12.0)).clicked() {
            let defaults = Config::default();
            // Runtime pause and recent history are not settings and must survive a reset.
            changed = config.silent_mode != defaults.silent_mode
                || config.numbering_style != defaults.numbering_style
                || config.app_rules != defaults.app_rules
                || config.fallback != defaults.fallback
                || config.secondary_items != defaults.secondary_items
                || config.hotkey != defaults.hotkey
                || config.full_menu_hotkey != defaults.full_menu_hotkey;
            config.silent_mode = defaults.silent_mode;
            config.numbering_style = defaults.numbering_style;
            config.app_rules = defaults.app_rules;
            config.fallback = defaults.fallback;
            config.secondary_items = defaults.secondary_items;
            config.hotkey = defaults.hotkey;
            config.full_menu_hotkey = defaults.full_menu_hotkey;
            self.recording = None;
            self.hotkey_error = None;
            self.preview = Format::ChineseNumbered;
            self.selected_app = "TextEdit / Notepad / gedit".into();
        }
        changed
    }

    fn add_dialog(&mut self, ctx: &egui::Context, config: &mut Config) -> bool {
        if !self.adding {
            return false;
        }
        let mut open = true;
        let mut added = false;
        egui::Window::new("添加 App")
            .id(egui::Id::new("add_app_dialog"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .default_width(400.0)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label("显示名称");
                ui.text_edit_singleline(&mut self.app_name);
                ui.label("匹配标识（不是窗口标题）");
                dropdown(
                    ui,
                    "identifier_kind",
                    &mut self.identifier_kind,
                    &[(0, "Bundle ID"), (1, "可执行文件名"), (2, "WM_CLASS")],
                    155.0,
                );
                ui.add(
                    egui::TextEdit::singleline(&mut self.app_identifier).hint_text(
                        [
                            "例如 com.example.editor",
                            "例如 editor.exe",
                            "例如 org.example.Editor",
                        ][self.identifier_kind],
                    ),
                );
                format_combo(ui, "new_app_format", &mut self.app_format);
                let error = new_app_error(
                    &self.app_name,
                    &self.app_identifier,
                    self.identifier_kind,
                    config.app_rules.len(),
                );
                if let Some(error) = error {
                    ui.label(RichText::new(error).size(11.0).color(MUTED));
                }
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(error.is_none(), egui::Button::new("添加"))
                        .clicked()
                    {
                        let name = self.app_name.trim().to_owned();
                        let id = self.app_identifier.trim().to_owned();
                        let mut app = AppIdentity {
                            name: name.clone(),
                            ..AppIdentity::default()
                        };
                        match self.identifier_kind {
                            0 => app.bundle_id = Some(id),
                            1 => app.exe = Some(id),
                            _ => app.wm_class = Some(id),
                        }
                        // Keep aliases sharing a name consistent, including previously mixed groups.
                        set_group_format(&mut config.app_rules, &name, self.app_format);
                        config.app_rules.insert(
                            0,
                            AppRule {
                                app,
                                format: self.app_format,
                            },
                        );
                        self.selected_app = name;
                        self.preview = self.app_format;
                        self.app_name.clear();
                        self.app_identifier.clear();
                        added = true;
                    }
                    if ui.button("取消").clicked() {
                        self.adding = false;
                    }
                });
            });
        self.adding &= open && !added;
        added
    }
}

fn section(ui: &mut egui::Ui, text: &str) -> egui::Response {
    // Weight 500 is not available from the installed CJK faces; never bold.
    ui.label(RichText::new(text).size(11.0).color(MUTED))
}

/// Window width the settings layout is designed for (default 960×680).
pub const DEFAULT_SIZE: Vec2 = Vec2::new(960.0, 680.0);
const MIN_WIDTH: f32 = 860.0;
const LEFT_WIDTH: f32 = 400.0;
const SEG_TRACK: Color32 = Color32::from_rgb(240, 240, 243);

/// macOS-style segmented control: light gray track, white sliding thumb
/// with a soft shadow, no accent fill.
pub(super) fn segmented<T: Copy + PartialEq>(
    ui: &mut egui::Ui,
    id: &str,
    options: &[(T, &str)],
    value: &mut T,
    width: f32,
) -> bool {
    let (track, _) = ui.allocate_exact_size(Vec2::new(width, 28.0), egui::Sense::hover());
    ui.painter().rect_filled(track, 7.0, SEG_TRACK);
    let inner = track.shrink(2.0);
    let cell = inner.width() / options.len() as f32;
    let selected = options.iter().position(|(v, _)| v == value).unwrap_or(0);
    let x = ui.ctx().animate_value_with_time(
        egui::Id::new(("segmented_thumb", id)),
        selected as f32 * cell,
        0.12,
    );
    let thumb = egui::Rect::from_min_size(
        inner.min + Vec2::new(x, 0.0),
        Vec2::new(cell, inner.height()),
    );
    ui.painter().add(
        egui::Shadow {
            offset: [0, 1],
            blur: 2,
            spread: 0,
            color: Color32::from_black_alpha(26),
        }
        .as_shape(thumb, 5.0),
    );
    ui.painter().rect_filled(thumb, 5.0, Color32::WHITE);
    let mut changed = false;
    for (index, (option, label)) in options.iter().enumerate() {
        let rect = egui::Rect::from_min_size(
            inner.min + Vec2::new(index as f32 * cell, 0.0),
            Vec2::new(cell, inner.height()),
        );
        let on = index == selected;
        let response = ui.interact(
            rect,
            egui::Id::new(("segmented", id, index)),
            egui::Sense::click(),
        );
        response.widget_info(|| {
            egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, on, *label)
        });
        ui.painter().with_clip_rect(rect).text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            *label,
            FontId::proportional(12.0),
            if on { TEXT } else { SUB },
        );
        if response.clicked() && !on {
            *value = *option;
            changed = true;
        }
    }
    changed
}

/// Checkbox matching the mockup `.cb`: 15px, radius 4, accent fill + white tick.
pub(super) fn check(ui: &mut egui::Ui, value: &mut bool, label: &str) -> egui::Response {
    let (rect, mut response) = ui.allocate_exact_size(Vec2::splat(15.0), egui::Sense::click());
    if response.clicked() {
        *value = !*value;
        response.mark_changed();
    }
    let on = *value;
    response
        .widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Checkbox, true, on, label));
    if on {
        ui.painter().rect_filled(rect, 4.0, ACCENT);
        let p = |x: f32, y: f32| rect.min + Vec2::new(x, y);
        ui.painter().line(
            vec![p(4.0, 7.8), p(6.6, 10.4), p(11.2, 5.0)],
            Stroke::new(1.6_f32, Color32::WHITE),
        );
    } else {
        ui.painter().rect(
            rect,
            4.0,
            Color32::WHITE,
            Stroke::new(1.0_f32, Color32::from_rgb(199, 199, 204)),
            egui::StrokeKind::Inside,
        );
    }
    response
}

/// Key cap like the mockup `.kbd span`.
fn key_cap(ui: &mut egui::Ui, text: &str) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(20.0, 20.0), egui::Sense::hover());
    ui.painter().rect(
        rect,
        4.0,
        Color32::from_rgb(247, 247, 248),
        Stroke::new(1.0_f32, LINE),
        egui::StrokeKind::Inside,
    );
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        text,
        FontId::proportional(11.0),
        SUB,
    );
}

/// Renders text whose leading spaces encode nesting as real left indentation.
fn indented_text(ui: &mut egui::Ui, text: &str) {
    ui.spacing_mut().item_spacing.y = 0.0;
    for line in text.lines() {
        let trimmed = line.trim_start_matches(' ');
        if trimmed.is_empty() {
            ui.add_space(12.0);
            continue;
        }
        let depth = line.len() - trimmed.len();
        ui.horizontal_wrapped(|ui| {
            // 2 leading spaces = 1em at 13px.
            ui.add_space(depth as f32 * 6.5);
            ui.add(
                egui::Label::new(RichText::new(trimmed).size(13.0).line_height(Some(24.0))).wrap(),
            );
        });
    }
}
fn card(ui: &mut egui::Ui, contents: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(Color32::WHITE)
        .stroke(Stroke::new(1.0_f32, LINE))
        .corner_radius(10.0)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 0.0;
            contents(ui);
        });
}
fn app_name(ui: &mut egui::Ui, label: &str, selected: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(147.0, 24.0), egui::Sense::click());
    // Selection is shown by the full-row tint in `row_selected`; text is unchanged.
    ui.painter().with_clip_rect(rect).text(
        rect.left_center() + Vec2::new(4.0, 0.0),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(13.0),
        TEXT,
    );
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Button, ui.is_enabled(), selected, label)
    });
    response
}

fn row(ui: &mut egui::Ui, height: f32, contents: impl FnOnce(&mut egui::Ui)) {
    row_selected(ui, height, false, contents);
}
/// Accent at 8% alpha behind the whole selected row.
const ROW_SELECTED: Color32 = Color32::from_rgba_premultiplied(
    (0x5B_u32 * 20 / 255) as u8,
    (0x5B_u32 * 20 / 255) as u8,
    (0xD6_u32 * 20 / 255) as u8,
    20,
);
fn row_selected(
    ui: &mut egui::Ui,
    height: f32,
    selected: bool,
    contents: impl FnOnce(&mut egui::Ui),
) {
    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), height),
        egui::Sense::hover(),
    );
    if selected {
        ui.painter().rect_filled(rect, 0.0, ROW_SELECTED);
    }
    ui.painter()
        .hline(rect.x_range(), rect.bottom(), Stroke::new(1.0_f32, LINE));
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect.shrink2(Vec2::new(10.0, 3.0)))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    child.spacing_mut().item_spacing.x = 6.0;
    contents(&mut child);
}
fn format_name(format: Format) -> &'static str {
    match format {
        Format::ChineseNumbered => "纯文本 · 中文序号",
        Format::WordRichText => "Word / WPS 富文本",
        Format::Markdown => "Markdown 原样",
        Format::PlainStripped => "纯文本 · 去符号",
        Format::HtmlSource => "HTML 源码",
    }
}
fn format_combo(ui: &mut egui::Ui, id: impl std::hash::Hash, value: &mut Format) -> bool {
    let options = FORMATS.map(|f| (f, format_name(f)));
    dropdown(ui, id, value, &options, 155.0)
}

/// Borderless select: right-aligned text plus a chevron, light gray only on
/// hover/open. Replaces egui's ComboBox gray block and filled triangle.
fn dropdown<T: Copy + PartialEq>(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    value: &mut T,
    options: &[(T, &str)],
    width: f32,
) -> bool {
    let popup_id = ui.make_persistent_id(("dropdown", id));
    let current = options
        .iter()
        .find(|(v, _)| v == value)
        .map(|(_, l)| *l)
        .unwrap_or("");
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 24.0), egui::Sense::click());
    let open = ui.memory(|m| m.is_popup_open(popup_id));
    if response.hovered() || open {
        ui.painter().rect_filled(rect, 5.0, SEG_TRACK);
    }
    let chevron = egui::pos2(rect.right() - 12.0, rect.center().y);
    ui.painter().line(
        vec![
            chevron + Vec2::new(-3.5, -1.5),
            chevron + Vec2::new(0.0, 2.0),
            chevron + Vec2::new(3.5, -1.5),
        ],
        Stroke::new(1.3_f32, SUB),
    );
    ui.painter().with_clip_rect(rect).text(
        egui::pos2(rect.right() - 22.0, rect.center().y),
        egui::Align2::RIGHT_CENTER,
        current,
        FontId::proportional(12.0),
        TEXT,
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::ComboBox, true, current));
    if response.clicked() {
        ui.memory_mut(|m| m.toggle_popup(popup_id));
    }
    let mut changed = false;
    egui::popup_below_widget(
        ui,
        popup_id,
        &response,
        egui::PopupCloseBehavior::CloseOnClick,
        |ui| {
            ui.set_min_width(width);
            for (option, label) in options {
                if ui
                    .selectable_label(*value == *option, RichText::new(*label).size(12.0))
                    .clicked()
                    && *value != *option
                {
                    *value = *option;
                    changed = true;
                }
            }
        },
    );
    changed
}

/// iOS/macOS-style switch (accent when on, gray when off).
fn toggle(ui: &mut egui::Ui, value: &mut bool, label: &str) -> egui::Response {
    let (rect, mut response) = ui.allocate_exact_size(Vec2::new(32.0, 18.0), egui::Sense::click());
    if response.clicked() {
        *value = !*value;
        response.mark_changed();
    }
    let on = *value;
    response
        .widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Checkbox, true, on, label));
    let t = ui.ctx().animate_bool_with_time(response.id, on, 0.12);
    let track = Color32::from_rgb(214, 214, 219).lerp_to_gamma(ACCENT, t);
    ui.painter().rect_filled(rect, 9.0, track);
    let x = egui::lerp(rect.left() + 9.0..=rect.right() - 9.0, t);
    let knob = egui::pos2(x, rect.center().y);
    ui.painter().circle_filled(
        knob + Vec2::new(0.0, 0.5),
        7.5,
        Color32::from_black_alpha(30),
    );
    ui.painter().circle_filled(knob, 7.0, Color32::WHITE);
    response
}
fn display_name(name: &str) -> &str {
    match name {
        "TextEdit / Notepad / gedit" => "文本编辑 / 记事本",
        "WPS Writer" => "WPS 文字",
        _ => name,
    }
}
fn badge(ui: &mut egui::Ui, name: &str) {
    let (text, color) = match name {
        "Microsoft Word" => ("W", Color32::from_rgb(43, 92, 216)),
        "WPS Writer" => ("W", Color32::from_rgb(216, 69, 43)),
        "Notion" => ("N", Color32::BLACK),
        "Obsidian" => ("O", ACCENT),
        "?" => ("?", MUTED),
        _ => ("TXT", Color32::from_rgb(107, 114, 128)),
    };
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(20.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 5.0, color);
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        text,
        FontId::proportional(10.0),
        Color32::WHITE,
    );
}
fn grouped_rules(rules: &[AppRule]) -> Vec<(String, Vec<usize>)> {
    let mut groups: Vec<(String, Vec<usize>)> = Vec::new();
    for (index, rule) in rules.iter().enumerate() {
        if let Some((_, indices)) = groups.iter_mut().find(|(name, _)| *name == rule.app.name) {
            indices.push(index);
        } else {
            groups.push((rule.app.name.clone(), vec![index]));
        }
    }
    groups
}
fn set_group_format(rules: &mut [AppRule], name: &str, format: Format) -> bool {
    let mut changed = false;
    for rule in rules.iter_mut().filter(|r| r.app.name == name) {
        changed |= rule.format != format;
        rule.format = format;
    }
    changed
}
fn reorder(items: &mut Vec<SecondaryItem>, from: usize, to: usize) -> bool {
    if from == to || from >= items.len() || to >= items.len() {
        return false;
    }
    let item = items.remove(from);
    items.insert(to, item);
    true
}
fn new_app_error<'a>(name: &str, identifier: &str, kind: usize, count: usize) -> Option<&'a str> {
    if count >= 1024 {
        return Some("最多允许 1024 条 App 匹配规则");
    }
    if name.trim().is_empty() || identifier.trim().is_empty() {
        return Some("请输入名称和实际匹配标识");
    }
    if [name, identifier]
        .iter()
        .any(|s| s.len() > 4096 || s.contains('\0'))
    {
        return Some("每项最多 4096 字节，不能包含 NUL");
    }
    if kind == 1
        && identifier
            .trim()
            .rsplit(['/', '\\'])
            .next()
            .is_none_or(|s| s.trim().is_empty())
    {
        return Some("请输入包含文件名的可执行文件路径");
    }
    None
}
pub(super) fn recorded_hotkey(key: egui::Key, modifiers: egui::Modifiers) -> Option<String> {
    let name = format!("{key:?}");
    let key = if let Some(digit) = name.strip_prefix("Num") {
        digit.to_owned()
    } else if (name.len() == 1 && name.as_bytes()[0].is_ascii_uppercase())
        || matches!(
            key,
            egui::Key::Escape
                | egui::Key::Enter
                | egui::Key::Space
                | egui::Key::Tab
                | egui::Key::Backspace
                | egui::Key::Delete
                | egui::Key::Insert
                | egui::Key::Home
                | egui::Key::End
                | egui::Key::PageUp
                | egui::Key::PageDown
                | egui::Key::ArrowUp
                | egui::Key::ArrowDown
                | egui::Key::ArrowLeft
                | egui::Key::ArrowRight
        )
        || name
            .strip_prefix('F')
            .and_then(|n| n.parse::<u8>().ok())
            .is_some_and(|n| (1..=12).contains(&n))
    {
        name
    } else {
        return None;
    };
    let mut parts = Vec::new();
    if modifiers.ctrl {
        parts.push("Ctrl");
    }
    if modifiers.alt {
        parts.push("Alt");
    }
    if modifiers.shift {
        parts.push("Shift");
    }
    // egui's `command` also mirrors Ctrl on non-macOS; only mac_cmd is Super.
    if modifiers.mac_cmd {
        parts.push("Super");
    }
    if !(modifiers.ctrl || modifiers.alt || modifiers.mac_cmd) {
        return None;
    }
    parts.push(&key);
    Some(parts.join("+"))
}

pub(super) fn same_hotkey(left: &str, right: &str) -> bool {
    let canonical = |value: &str| {
        let mut parts: Vec<_> = value
            .split('+')
            .map(|part| match part.trim().to_ascii_lowercase().as_str() {
                "control" => "ctrl".to_owned(),
                "cmd" | "command" => "super".to_owned(),
                "option" => "alt".to_owned(),
                other => other.to_owned(),
            })
            .collect();
        parts.sort();
        parts
    };
    canonical(left) == canonical(right)
}

pub(super) fn hotkey_labels(hotkey: &str) -> Vec<String> {
    hotkey
        .split('+')
        .map(|key| match key.trim() {
            "Super" | "Cmd" | "Command" if cfg!(target_os = "macos") => "⌘".into(),
            "Super" | "Cmd" | "Command" => "Super".into(),
            "Shift" if cfg!(target_os = "macos") => "⇧".into(),
            "Alt" if cfg!(target_os = "macos") => "⌥".into(),
            "Ctrl" | "Control" if cfg!(target_os = "macos") => "⌃".into(),
            key => key.into(),
        })
        .collect()
}

// The input is our core renderer's safe, controlled HTML, not arbitrary browser HTML.
// Convert semantic tags into egui text runs: never display HTML source in Word mode.
fn rich_preview(ui: &mut egui::Ui, html: &str) {
    let body = html.split_once("<body>").map_or(html, |(_, body)| body);
    let mut rest = body;
    let mut job = egui::text::LayoutJob::default();
    let (mut bold, mut italic, mut code, mut strike) = (false, false, false, false);
    let mut heading = 0;
    let mut lists: Vec<Option<usize>> = Vec::new();
    let mut table = false;
    let mut link = false;
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix('<') {
            let Some((tag, remaining)) = after.split_once('>') else {
                break;
            };
            rest = remaining;
            let name = tag.split_whitespace().next().unwrap_or("");
            match name {
                "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                    flush_rich(ui, &mut job, false);
                    heading = name[1..].parse::<u8>().unwrap_or(1);
                }
                "/h1" | "/h2" | "/h3" | "/h4" | "/h5" | "/h6" => {
                    flush_rich(ui, &mut job, false);
                    heading = 0;
                    ui.add_space(6.0);
                }
                "strong" | "b" | "th" => bold = true,
                "/strong" | "/b" => bold = false,
                "em" | "i" => italic = true,
                "/em" | "/i" => italic = false,
                "del" | "s" => strike = true,
                "/del" | "/s" => strike = false,
                "code" | "pre" => code = true,
                "/code" | "/pre" => {
                    if name == "/pre" {
                        flush_rich(ui, &mut job, false);
                    }
                    code = false;
                }
                "a" => link = true,
                "/a" => link = false,
                "ul" => {
                    flush_rich(ui, &mut job, false);
                    lists.push(None);
                }
                "ol" => {
                    flush_rich(ui, &mut job, false);
                    let start = tag
                        .split("start=\"")
                        .nth(1)
                        .and_then(|s| s.split('"').next())
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(1);
                    lists.push(Some(start));
                }
                "/ul" | "/ol" => {
                    flush_rich(ui, &mut job, false);
                    lists.pop();
                }
                "li" => {
                    flush_rich(ui, &mut job, false);
                    let indent = "    ".repeat(lists.len().saturating_sub(1));
                    let marker = if let Some(Some(n)) = lists.last_mut() {
                        let marker = format!("{n}. ");
                        *n += 1;
                        marker
                    } else {
                        "• ".into()
                    };
                    job.append(
                        &format!("{indent}{marker}"),
                        0.0,
                        egui::TextFormat {
                            font_id: FontId::proportional(13.0),
                            color: TEXT,
                            ..Default::default()
                        },
                    );
                }
                "/li" | "br" | "br/" => flush_rich(ui, &mut job, false),
                "p" => {
                    if lists.is_empty() {
                        flush_rich(ui, &mut job, false);
                    }
                }
                "/p" => {
                    flush_rich(ui, &mut job, false);
                    if lists.is_empty() {
                        ui.add_space(5.0);
                    }
                }
                "table" => {
                    flush_rich(ui, &mut job, false);
                    table = true;
                }
                "/table" => {
                    flush_rich(ui, &mut job, true);
                    table = false;
                }
                "tr" => {
                    flush_rich(ui, &mut job, true);
                }
                "/tr" => flush_rich(ui, &mut job, true),
                "/td" | "/th" => {
                    job.append(
                        "   │   ",
                        0.0,
                        egui::TextFormat {
                            color: MUTED,
                            ..Default::default()
                        },
                    );
                    bold = false;
                }
                "hr" => {
                    flush_rich(ui, &mut job, false);
                    ui.separator();
                }
                "blockquote" => {
                    flush_rich(ui, &mut job, false);
                    job.append(
                        "▎ ",
                        0.0,
                        egui::TextFormat {
                            color: ACCENT,
                            ..Default::default()
                        },
                    );
                }
                _ => {}
            }
        } else {
            let end = rest.find('<').unwrap_or(rest.len());
            let text = decode_entities(&rest[..end]);
            rest = &rest[end..];
            if text.trim().is_empty() && !code {
                continue;
            }
            let size = match heading {
                1 => 24.0,
                2 => 18.0,
                3 => 16.0,
                4 => 14.0,
                _ => 13.0,
            };
            let format = egui::TextFormat {
                font_id: if bold || heading > 0 {
                    FontId::new(if code { 12.0 } else { size }, super::fonts::bold_family())
                } else if code {
                    FontId::monospace(12.0)
                } else {
                    FontId::proportional(size)
                },
                color: if link { ACCENT } else { TEXT },
                background: if code {
                    Color32::from_rgb(245, 245, 247)
                } else {
                    Color32::TRANSPARENT
                },
                italics: italic,
                underline: if link {
                    Stroke::new(1.0_f32, ACCENT)
                } else {
                    Stroke::NONE
                },
                strikethrough: if strike {
                    Stroke::new(1.0_f32, TEXT)
                } else {
                    Stroke::NONE
                },
                ..Default::default()
            };
            job.append(
                if code { &text } else { text.trim_matches('\n') },
                0.0,
                format,
            );
        }
    }
    flush_rich(ui, &mut job, table);
}
fn flush_rich(ui: &mut egui::Ui, job: &mut egui::text::LayoutJob, border: bool) {
    if job.text.is_empty() {
        return;
    }
    let job = std::mem::take(job);
    if border {
        egui::Frame::new()
            .stroke(Stroke::new(1.0_f32, LINE))
            .inner_margin(5.0)
            .show(ui, |ui| {
                ui.add(egui::Label::new(job).wrap());
            });
    } else {
        ui.add(egui::Label::new(job).wrap());
    }
}
fn decode_entities(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(index) = rest.find('&') {
        out.push_str(&rest[..index]);
        rest = &rest[index..];
        if let Some(end) = rest.find(';').filter(|&end| end <= 16) {
            let entity = &rest[1..end];
            let ch = match entity {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" | "#39" => Some('\''),
                "nbsp" => Some(' '),
                _ => entity
                    .strip_prefix("#x")
                    .and_then(|s| u32::from_str_radix(s, 16).ok())
                    .or_else(|| entity.strip_prefix('#').and_then(|s| s.parse().ok()))
                    .and_then(char::from_u32),
            };
            if let Some(ch) = ch {
                out.push(ch);
                rest = &rest[end + 1..];
                continue;
            }
        }
        out.push('&');
        rest = &rest[1..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn word_preview_uses_real_bold_family_for_emphasis_and_headings() {
        let ctx = egui::Context::default();
        super::super::fonts::install(&ctx);
        let output = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                rich_preview(
                    ui,
                    "<body><h1>标题</h1><p>正文<strong>剪贴板监听</strong><em>斜体</em></p></body>",
                );
            });
        });
        let mut bold_runs = 0;
        let mut regular_runs = 0;
        for shape in output.shapes {
            if let egui::epaint::Shape::Text(text) = shape.shape {
                for section in &text.galley.job.sections {
                    let run = &text.galley.job.text[section.byte_range.clone()];
                    if run == "标题" || run == "剪贴板监听" {
                        assert_eq!(
                            section.format.font_id.family,
                            super::super::fonts::bold_family()
                        );
                        assert_eq!(section.format.extra_letter_spacing, 0.0);
                        bold_runs += 1;
                    } else if run == "正文" || run == "斜体" {
                        assert_eq!(
                            section.format.font_id.family,
                            egui::FontFamily::Proportional
                        );
                        regular_runs += 1;
                    }
                }
            }
        }
        assert_eq!(bold_runs, 2);
        assert_eq!(regular_runs, 2);
    }

    fn record(
        state: &mut SettingsState,
        config: &mut Config,
        key: egui::Key,
        modifiers: egui::Modifiers,
    ) -> bool {
        let ctx = egui::Context::default();
        let mut changed = false;
        let _ = ctx.run(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers,
                }],
                ..Default::default()
            },
            |ctx| changed = state.record_hotkey(ctx, config),
        );
        changed
    }

    #[test]
    fn separate_recorders_only_edit_the_selected_hotkey() {
        let mut config = Config::default();
        let old_menu = config.full_menu_hotkey.clone();
        let mut state = SettingsState {
            recording: Some(HotkeyTarget::Paste),
            ..Default::default()
        };
        assert!(record(
            &mut state,
            &mut config,
            egui::Key::P,
            egui::Modifiers::CTRL | egui::Modifiers::SHIFT
        ));
        assert_eq!(config.hotkey, "Ctrl+Shift+P");
        assert_eq!(config.full_menu_hotkey, old_menu);
        assert_eq!(state.recording, None);
        state.recording = Some(HotkeyTarget::FullMenu);
        assert!(record(
            &mut state,
            &mut config,
            egui::Key::M,
            egui::Modifiers::CTRL | egui::Modifiers::ALT
        ));
        assert_eq!(config.hotkey, "Ctrl+Shift+P");
        assert_eq!(config.full_menu_hotkey, "Ctrl+Alt+M");
    }

    #[test]
    fn recorder_rejects_duplicates_and_unmodified_keys_and_escape_cancels() {
        let mut config = Config {
            hotkey: "Shift+Control+P".into(),
            ..Default::default()
        };
        let original = config.full_menu_hotkey.clone();
        let mut state = SettingsState {
            recording: Some(HotkeyTarget::FullMenu),
            ..Default::default()
        };
        assert!(!record(
            &mut state,
            &mut config,
            egui::Key::P,
            egui::Modifiers::CTRL | egui::Modifiers::SHIFT
        ));
        assert!(state.hotkey_error.is_some());
        assert!(!record(
            &mut state,
            &mut config,
            egui::Key::V,
            egui::Modifiers::NONE
        ));
        assert!(state.recording.is_some());
        assert!(!record(
            &mut state,
            &mut config,
            egui::Key::Escape,
            egui::Modifiers::NONE
        ));
        assert_eq!(config.full_menu_hotkey, original);
        assert_eq!(state.recording, None);
        assert_eq!(state.hotkey_error, None);
    }

    #[test]
    fn recorded_syntax_matches_native_parser_and_mac_modifiers() {
        assert_eq!(
            recorded_hotkey(
                egui::Key::V,
                egui::Modifiers::MAC_CMD | egui::Modifiers::ALT
            )
            .as_deref(),
            Some("Alt+Super+V")
        );
        assert_eq!(
            recorded_hotkey(egui::Key::Num7, egui::Modifiers::CTRL).as_deref(),
            Some("Ctrl+7")
        );
        assert_eq!(
            recorded_hotkey(egui::Key::F12, egui::Modifiers::CTRL).as_deref(),
            Some("Ctrl+F12")
        );
        assert_eq!(
            recorded_hotkey(egui::Key::ArrowDown, egui::Modifiers::ALT).as_deref(),
            Some("Alt+ArrowDown")
        );
        assert_eq!(recorded_hotkey(egui::Key::F13, egui::Modifiers::CTRL), None);
        assert_eq!(
            recorded_hotkey(egui::Key::Plus, egui::Modifiers::CTRL),
            None
        );
        assert_eq!(recorded_hotkey(egui::Key::V, egui::Modifiers::SHIFT), None);
        assert!(same_hotkey("Super+Alt+V", "Option+Cmd+V"));
    }

    #[test]
    fn groups_preserve_aliases_and_edit_every_rule() {
        let mut config = Config::default();
        let groups = grouped_rules(&config.app_rules);
        assert_eq!(groups.len(), 5);
        assert!(groups[0].1.len() > 1);
        let ids: Vec<_> = config.app_rules.iter().map(|r| r.app.clone()).collect();
        assert!(set_group_format(
            &mut config.app_rules,
            &groups[0].0,
            Format::PlainStripped
        ));
        for i in &groups[0].1 {
            assert_eq!(config.app_rules[*i].format, Format::PlainStripped);
        }
        assert_eq!(
            ids,
            config
                .app_rules
                .iter()
                .map(|r| r.app.clone())
                .collect::<Vec<_>>()
        );
        assert!(!set_group_format(
            &mut config.app_rules,
            &groups[0].0,
            Format::PlainStripped
        ));
    }
    #[test]
    fn reorder_retains_disabled_state_and_relative_order() {
        let mut items = Config::default().secondary_items;
        assert!(reorder(&mut items, 4, 0));
        assert_eq!(items[0].format, Format::HtmlSource);
        assert!(!items[0].enabled);
        assert_eq!(items[1].format, Format::ChineseNumbered);
        assert!(!reorder(&mut items, 0, 0));
        assert!(!reorder(&mut items, 8, 0));
    }
    #[test]
    fn view_renders_every_format_without_changing_configuration() {
        let ctx = egui::Context::default();
        super::super::fonts::install(&ctx);
        let mut config = Config::default();
        let mut state = SettingsState::default();
        state.markdown.push_str("\n\n| A | B |\n|---|---|\n| **bold** | *italic* |\n\n```rust\nlet x = 1;\n```\n\n> quote\n\n[link](https://example.com)");
        for format in FORMATS {
            state.preview = format;
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, DEFAULT_SIZE)),
                    ..Default::default()
                },
                |ctx| assert!(!state.show(ctx, &mut config)),
            );
            assert!(!output.shapes.is_empty());
        }
    }

    #[test]
    fn identifier_validation_and_entities() {
        assert!(new_app_error("Editor", "", 0, 0).is_some());
        assert!(new_app_error("Editor", "C:\\", 1, 0).is_some());
        assert!(new_app_error("Editor", "editor.exe", 1, 0).is_none());
        assert_eq!(
            decode_entities("&lt;x&gt; &amp; &#20013; &#x6587;"),
            "<x> & 中 文"
        );
    }
    #[test]
    fn focus_hotkey_starts_recording_the_paste_hotkey() {
        let mut state = SettingsState::default();
        state.focus_hotkey();
        assert_eq!(state.recording, Some(HotkeyTarget::Paste));
        assert!(state.scroll_to_hotkey);
    }
}
