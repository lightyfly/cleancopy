//! Self-contained settings view. The caller owns persistence and window setup.
use crate::config::{AppIdentity, AppRule, Config, Fallback, Format, SecondaryItem};
use cleancopy_core::{
    render_html, render_markdown, render_numbered, render_plain, render_word_html, NumberingStyle,
};
use eframe::egui::{self, Color32, FontId, RichText, Stroke, Vec2};

const ACCENT: Color32 = Color32::from_rgb(91, 91, 214);
const LINE: Color32 = Color32::from_rgb(230, 230, 234);
const MUTED: Color32 = Color32::from_rgb(145, 145, 153);
const TEXT: Color32 = Color32::from_rgb(29, 29, 31);
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
        }
    }
}

impl SettingsState {
    /// Returns true only for configuration edits, not preview or selection edits.
    pub fn show(&mut self, ctx: &egui::Context, config: &mut Config) -> bool {
        let mut changed = false;
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(Color32::WHITE))
            .show(ctx, |ui| {
                ui.style_mut().visuals = egui::Visuals::light();
                ui.style_mut().visuals.selection.bg_fill = ACCENT;
                ui.style_mut().visuals.override_text_color = Some(TEXT);
                ui.style_mut().spacing.item_spacing = Vec2::new(8.0, 8.0);
                ui.style_mut().override_font_id = Some(FontId::proportional(13.0));
                let origin = ui.max_rect().min;
                let window = egui::Rect::from_min_size(origin, Vec2::new(1120.0, 720.0));
                ui.set_min_size(window.size());
                let title = egui::Rect::from_min_size(origin, Vec2::new(1120.0, 44.0));
                ui.painter().text(
                    title.center(),
                    egui::Align2::CENTER_CENTER,
                    "CleanCopy 设置",
                    FontId::proportional(13.0),
                    TEXT,
                );
                for (i, color) in [
                    Color32::from_rgb(255, 95, 87),
                    Color32::from_rgb(254, 188, 46),
                    Color32::from_rgb(40, 200, 64),
                ]
                .into_iter()
                .enumerate()
                {
                    ui.painter().circle_filled(
                        origin + Vec2::new(22.0 + i as f32 * 20.0, 22.0),
                        6.0,
                        color,
                    );
                }
                ui.painter()
                    .hline(title.x_range(), title.bottom(), Stroke::new(1.0_f32, LINE));
                let left = egui::Rect::from_min_size(
                    origin + Vec2::new(0.0, 44.0),
                    Vec2::new(440.0, 676.0),
                );
                ui.painter()
                    .vline(left.right(), left.y_range(), Stroke::new(1.0_f32, LINE));
                let mut left_ui = ui.new_child(
                    egui::UiBuilder::new()
                        .id_salt("settings_left")
                        .max_rect(left.shrink2(Vec2::new(24.0, 20.0))),
                );
                egui::ScrollArea::vertical()
                    .id_salt("settings_controls")
                    .show(&mut left_ui, |ui| {
                        ui.set_width(392.0);
                        changed |= self.controls(ui, config);
                    });
                let right = egui::Rect::from_min_size(
                    origin + Vec2::new(440.0, 44.0),
                    Vec2::new(680.0, 676.0),
                );
                ui.painter()
                    .rect_filled(right, 0.0, Color32::from_rgb(250, 250, 251));
                changed |= self.preview_ui(ui, right, config);
            });
        changed |= self.add_dialog(ctx, config);
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
            egui::ScrollArea::vertical()
                .id_salt("app_groups")
                .max_height(228.0)
                .show(ui, |ui| {
                    for (name, indices) in grouped_rules(&config.app_rules) {
                        let mut format = config.app_rules[indices[0]].format;
                        let mixed = indices
                            .iter()
                            .any(|&i| config.app_rules[i].format != format);
                        row(ui, 38.0, |ui| {
                            badge(ui, &name);
                            let label = display_name(&name);
                            if ui
                                .add_sized(
                                    [147.0, 24.0],
                                    egui::Button::new(RichText::new(label).size(12.0))
                                        .selected(self.selected_app == name)
                                        .frame(false),
                                )
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
                });
            row(ui, 38.0, |ui| {
                badge(ui, "?");
                if ui
                    .add_sized([147.0, 24.0], egui::Button::new("其他 App").frame(false))
                    .clicked()
                {
                    self.selected_app = "其他 App".into();
                    if let Fallback::Fixed(format) = config.fallback {
                        self.preview = format;
                    }
                }
                let old = config.fallback;
                egui::ComboBox::from_id_salt("fallback")
                    .width(155.0)
                    .selected_text(match config.fallback {
                        Fallback::Popup => "弹出格式菜单",
                        Fallback::Fixed(f) => format_name(f),
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut config.fallback, Fallback::Popup, "弹出格式菜单");
                        for format in FORMATS {
                            ui.selectable_value(
                                &mut config.fallback,
                                Fallback::Fixed(format),
                                format_name(format),
                            );
                        }
                    });
                if old != config.fallback {
                    changed = true;
                    self.selected_app = "其他 App".into();
                    if let Fallback::Fixed(format) = config.fallback {
                        self.preview = format;
                    }
                }
            });
        });
        ui.add_space(12.0);
        section(ui, "中文序号风格");
        egui::Frame::new()
            .fill(Color32::from_rgb(240, 240, 243))
            .corner_radius(7.0)
            .inner_margin(2.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    for (style, label) in [
                        (NumberingStyle::Chinese, "一、（一）1.（1）"),
                        (NumberingStyle::ChineseCompact, "一、1. (1)"),
                        (NumberingStyle::Decimal, "1. 1.1 1.1.1"),
                    ] {
                        if ui
                            .add_sized(
                                [126.0, 26.0],
                                egui::Button::new(RichText::new(label).size(12.0))
                                    .selected(config.numbering_style == style),
                            )
                            .clicked()
                            && config.numbering_style != style
                        {
                            config.numbering_style = style;
                            changed = true;
                        }
                    }
                });
            });
        ui.add_space(12.0);
        section(ui, "二级格式菜单（勾选显示 · 拖动排序）");
        let mut movement = None;
        let mut hover_target = None;
        let count = config.secondary_items.len();
        let mut shortcut = 2;
        card(ui, |ui| {
            for (index, item) in config.secondary_items.iter_mut().enumerate() {
                row(ui, 36.0, |ui| {
                    let grip = ui
                        .add(
                            egui::Label::new(RichText::new("⋮⋮").color(MUTED))
                                .sense(egui::Sense::drag()),
                        )
                        .on_hover_text("拖动以调整顺序；也可使用右侧上移、下移按钮");
                    if grip.drag_started() {
                        self.dragging = Some(index);
                    }
                    changed |= ui.checkbox(&mut item.enabled, "").changed();
                    ui.add_sized(
                        [150.0, 22.0],
                        egui::Label::new(RichText::new(format_name(item.format)).size(12.0))
                            .truncate(),
                    );
                    if item.enabled {
                        ui.label(RichText::new(shortcut.to_string()).color(MUTED).monospace());
                        shortcut += 1;
                    } else {
                        ui.label("  ");
                    }
                    if ui
                        .add_enabled(index > 0, egui::Button::new("↑").small())
                        .on_hover_text("上移格式")
                        .clicked()
                    {
                        movement = Some((index, index - 1));
                    }
                    if ui
                        .add_enabled(index + 1 < count, egui::Button::new("↓").small())
                        .on_hover_text("下移格式")
                        .clicked()
                    {
                        movement = Some((index, index + 1));
                    }
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
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            section(ui, "唤出快捷键");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                for key in hotkey_labels(&config.hotkey).into_iter().rev() {
                    egui::Frame::new()
                        .fill(Color32::from_rgb(250, 250, 251))
                        .stroke(Stroke::new(1.0_f32, LINE))
                        .corner_radius(4.0)
                        .inner_margin(4.0)
                        .show(ui, |ui| {
                            ui.label(RichText::new(key).size(11.0).color(MUTED));
                        });
                }
            });
        });
        changed |= ui
            .checkbox(
                &mut config.silent_mode,
                "静默模式 · 按目标 App 规则直接粘贴",
            )
            .on_hover_text("关闭后先显示格式菜单；其他 App 的兜底规则仍然生效")
            .changed();
        changed
    }

    fn preview_ui(&mut self, parent: &mut egui::Ui, rect: egui::Rect, config: &mut Config) -> bool {
        let mut changed = false;
        let header = egui::Rect::from_min_size(rect.min, Vec2::new(680.0, 44.0));
        let mut ui = parent.new_child(
            egui::UiBuilder::new()
                .id_salt("preview_header")
                .max_rect(header.shrink2(Vec2::new(20.0, 8.0)))
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        ui.label(RichText::new("实时预览").strong());
        ui.label(
            RichText::new(format!("· 当前：{}", display_name(&self.selected_app)))
                .size(11.0)
                .color(MUTED),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            for (format, label) in [
                (Format::Markdown, "Markdown"),
                (Format::WordRichText, "Word"),
                (Format::ChineseNumbered, "纯文本"),
            ] {
                if ui.selectable_label(self.preview == format, label).clicked() {
                    self.preview = format;
                }
            }
        });
        parent.painter().hline(
            header.x_range(),
            header.bottom(),
            Stroke::new(1.0_f32, LINE),
        );
        let pane_height = rect.height() - 80.0;
        let source = egui::Rect::from_min_size(
            rect.min + Vec2::new(0.0, 44.0),
            Vec2::new(340.0, pane_height),
        );
        let output = source.translate(Vec2::new(340.0, 0.0));
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
                        .desired_width(300.0)
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
                ui.set_min_width(298.0);
                match self.preview {
                    Format::WordRichText => rich_preview(ui, &render_word_html(&self.markdown)),
                    Format::ChineseNumbered => {
                        ui.add(
                            egui::Label::new(
                                RichText::new(render_numbered(
                                    &self.markdown,
                                    config.numbering_style,
                                ))
                                .size(13.0)
                                .line_height(Some(24.0)),
                            )
                            .wrap(),
                        );
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
            Vec2::new(680.0, 36.0),
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
                || config.hotkey != defaults.hotkey;
            config.silent_mode = defaults.silent_mode;
            config.numbering_style = defaults.numbering_style;
            config.app_rules = defaults.app_rules;
            config.fallback = defaults.fallback;
            config.secondary_items = defaults.secondary_items;
            config.hotkey = defaults.hotkey;
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
                egui::ComboBox::from_id_salt("identifier_kind")
                    .selected_text(["Bundle ID", "可执行文件名", "WM_CLASS"][self.identifier_kind])
                    .show_ui(ui, |ui| {
                        for (i, label) in ["Bundle ID", "可执行文件名", "WM_CLASS"]
                            .into_iter()
                            .enumerate()
                        {
                            ui.selectable_value(&mut self.identifier_kind, i, label);
                        }
                    });
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

fn section(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).size(11.0).strong().color(MUTED));
}
fn card(ui: &mut egui::Ui, contents: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(Color32::WHITE)
        .stroke(Stroke::new(1.0_f32, LINE))
        .corner_radius(10.0)
        .show(ui, |ui| {
            ui.set_width(390.0);
            ui.spacing_mut().item_spacing.y = 0.0;
            contents(ui);
        });
}
fn row(ui: &mut egui::Ui, height: f32, contents: impl FnOnce(&mut egui::Ui)) {
    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), height),
        egui::Sense::hover(),
    );
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
        Format::PlainStripped => "纯文本 · 去除标记",
        Format::HtmlSource => "HTML 源码",
    }
}
fn format_combo(ui: &mut egui::Ui, id: impl std::hash::Hash, value: &mut Format) -> bool {
    let old = *value;
    egui::ComboBox::from_id_salt(id)
        .width(155.0)
        .selected_text(RichText::new(format_name(*value)).size(12.0))
        .show_ui(ui, |ui| {
            for format in FORMATS {
                ui.selectable_value(value, format, format_name(format));
            }
        });
    old != *value
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
fn hotkey_labels(hotkey: &str) -> Vec<String> {
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
                font_id: if code {
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
                extra_letter_spacing: if bold || heading > 0 { 0.35 } else { 0.0 },
                // Slight tracking distinguishes emphasis without requiring a bundled bold font.
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
        let mut config = Config::default();
        let mut state = SettingsState::default();
        state.markdown.push_str("\n\n| A | B |\n|---|---|\n| **bold** | *italic* |\n\n```rust\nlet x = 1;\n```\n\n> quote\n\n[link](https://example.com)");
        for format in FORMATS {
            state.preview = format;
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        Vec2::new(1120.0, 720.0),
                    )),
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
}
