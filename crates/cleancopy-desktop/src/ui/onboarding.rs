//! First-run onboarding: a 480×360 three-step card in the settings style:
//! how CleanCopy is used (copy → paste hotkey), permissions, and a try-it
//! text box. Finishing or skipping persists
//! `Config::onboarding_done`; the tray item 「使用引导…」 reopens it.
use super::palette::{ACCENT, MUTED, SUB, TEXT};
use super::settings::{hotkey_labels, recorded_hotkey, same_hotkey};
use crate::config::Config;
use cleancopy_core::render_numbered;
use eframe::egui::{self, Color32, FontId, RichText, Stroke, Vec2};

const TRY_INPUT: &str = "onboarding_try_input";

/// Content size; 44 px shorter off macOS, where the native title bar replaces the strip.
pub const SIZE: Vec2 = Vec2::new(480.0, 316.0 + super::TITLE_STRIP);
pub const STEPS: usize = 3;
const LINE: Color32 = Color32::from_rgb(230, 230, 234);
pub const SAMPLE: &str = "# 项目周报\n## 本周进展\n- 完成 **剪贴板监听**\n- 接入 Word 富文本";

/// Accessibility permission state shown on step 2.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Permission {
    /// Linux/Windows: synthetic paste needs no extra grant.
    NotRequired,
    Granted,
    Missing,
}

#[cfg(target_os = "macos")]
mod ax {
    use std::ffi::c_void;
    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXIsProcessTrusted() -> bool;
        fn AXIsProcessTrustedWithOptions(options: *const c_void) -> bool;
        static kAXTrustedCheckOptionPrompt: *const c_void;
    }
    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        static kCFBooleanTrue: *const c_void;
        static kCFTypeDictionaryKeyCallBacks: c_void;
        static kCFTypeDictionaryValueCallBacks: c_void;
        fn CFDictionaryCreate(
            allocator: *const c_void,
            keys: *const *const c_void,
            values: *const *const c_void,
            count: isize,
            key_callbacks: *const c_void,
            value_callbacks: *const c_void,
        ) -> *const c_void;
        fn CFRelease(value: *const c_void);
    }
    pub fn trusted() -> bool {
        // SAFETY: plain C call with no arguments.
        unsafe { AXIsProcessTrusted() }
    }
    /// Shows the system "allow accessibility" prompt when not yet trusted.
    pub fn prompt() -> bool {
        // SAFETY: a one-entry CFDictionary of CF constants, released after use.
        unsafe {
            let keys = [kAXTrustedCheckOptionPrompt];
            let values = [kCFBooleanTrue];
            let options = CFDictionaryCreate(
                std::ptr::null(),
                keys.as_ptr(),
                values.as_ptr(),
                1,
                std::ptr::addr_of!(kCFTypeDictionaryKeyCallBacks),
                std::ptr::addr_of!(kCFTypeDictionaryValueCallBacks),
            );
            let trusted = AXIsProcessTrustedWithOptions(options);
            if !options.is_null() {
                CFRelease(options);
            }
            trusted
        }
    }
}

/// Live permission status (macOS: `AXIsProcessTrusted`).
pub fn permission_status() -> Permission {
    #[cfg(target_os = "macos")]
    {
        if ax::trusted() {
            Permission::Granted
        } else {
            Permission::Missing
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        Permission::NotRequired
    }
}

fn request_permission() {
    #[cfg(target_os = "macos")]
    {
        ax::prompt();
        let _ = std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
            .spawn();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Open,
    Finished,
    /// Esc: close the window like its close button (onboarding not marked
    /// done, the hotkey pause is released; report v5 N-5).
    Closed,
    /// "更改快捷键": open settings at the hotkey item.
    OpenHotkeySettings,
}

pub struct OnboardingState {
    pub step: usize,
    pub input: String,
    pub tried: bool,
    permission: Permission,
    polled_at: f64,
    open_hotkey_settings: bool,
}

impl Default for OnboardingState {
    fn default() -> Self {
        Self {
            step: 0,
            input: String::new(),
            tried: false,
            permission: permission_status(),
            polled_at: 0.0,
            open_hotkey_settings: false,
        }
    }
}

impl OnboardingState {
    pub fn next(&mut self) {
        self.step = (self.step + 1).min(STEPS - 1);
    }
    pub fn back(&mut self) {
        self.step = self.step.saturating_sub(1);
    }
    pub fn is_last(&self) -> bool {
        self.step + 1 == STEPS
    }
    /// Marks onboarding as done; returns whether the config changed.
    pub fn finish(&mut self, config: &mut Config) -> bool {
        let changed = !config.onboarding_done;
        config.onboarding_done = true;
        changed
    }
    /// Step 3: the paste hotkey pressed inside the try-it box inserts the
    /// formatted sample, mirroring what the daemon pastes into other apps.
    pub fn try_paste(&mut self, config: &Config) {
        if !self.input.is_empty() && !self.input.ends_with('\n') {
            self.input.push('\n');
        }
        self.input
            .push_str(&render_numbered(SAMPLE, config.numbering_style));
        self.tried = true;
    }

    /// Draws the card. Returns (config changed, outcome).
    pub fn show(&mut self, ctx: &egui::Context, config: &mut Config) -> (bool, Outcome) {
        let mut changed = false;
        let mut outcome = Outcome::Open;
        let now = ctx.input(|i| i.time);
        if self.step == 1 && now - self.polled_at > 1.0 {
            self.polled_at = now;
            self.permission = permission_status();
            ctx.request_repaint_after(std::time::Duration::from_secs(1));
        }
        let input_id = egui::Id::new(TRY_INPUT);
        if self.step == 2 && ctx.memory(|m| m.has_focus(input_id)) {
            let mut hit = false;
            ctx.input_mut(|input| {
                // egui-winit turns any Ctrl/⌘(+…)+V / C / X press into
                // `Paste` / `Copy` / `Cut` and drops the key event, so map those
                // back to their key with the modifiers held this frame.
                let mods = input.modifiers;
                let is_hotkey = |key| {
                    recorded_hotkey(key, mods).is_some_and(|h| same_hotkey(&h, &config.hotkey))
                };
                input.events.retain(|event| match event {
                    egui::Event::Paste(_) if is_hotkey(egui::Key::V) => {
                        hit = true;
                        false
                    }
                    egui::Event::Copy if is_hotkey(egui::Key::C) => {
                        hit = true;
                        false
                    }
                    egui::Event::Cut if is_hotkey(egui::Key::X) => {
                        hit = true;
                        false
                    }
                    egui::Event::Key {
                        key,
                        pressed: true,
                        modifiers,
                        ..
                    } if recorded_hotkey(*key, *modifiers)
                        .is_some_and(|h| same_hotkey(&h, &config.hotkey)) =>
                    {
                        hit = true;
                        false
                    }
                    _ => true,
                });
            });
            if hit {
                self.try_paste(config);
            }
        } else if !ctx.wants_keyboard_input() && ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
            if self.is_last() {
                changed |= self.finish(config);
                outcome = Outcome::Finished;
            } else {
                self.next();
            }
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(Color32::WHITE))
            .show(ctx, |ui| {
                ui.style_mut().visuals = egui::Visuals::light();
                ui.style_mut().visuals.selection.bg_fill = ACCENT;
                ui.style_mut().visuals.override_text_color = Some(TEXT);
                ui.style_mut().override_font_id = Some(FontId::proportional(13.0));
                let origin = ui.max_rect().min;
                let size = ui.max_rect().size();
                super::paint_title_strip(ui, origin, size.x, "欢迎使用 CleanCopy");
                let footer = egui::Rect::from_min_max(
                    egui::pos2(origin.x, origin.y + size.y - 52.0),
                    origin + size,
                );
                ui.painter()
                    .hline(footer.x_range(), footer.top(), Stroke::new(1.0_f32, LINE));
                let body = egui::Rect::from_min_max(
                    egui::pos2(origin.x, origin.y + super::TITLE_STRIP),
                    egui::pos2(origin.x + size.x, footer.top()),
                )
                .shrink2(Vec2::new(28.0, 18.0));
                let mut body_ui = ui.new_child(egui::UiBuilder::new().max_rect(body));
                body_ui.spacing_mut().item_spacing = Vec2::new(8.0, 10.0);
                self.step_indicator(&mut body_ui);
                match self.step {
                    0 => step_usage(&mut body_ui, config),
                    1 => self.step_permission(&mut body_ui),
                    _ => self.step_try(&mut body_ui, config, input_id),
                }
                let mut foot = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(footer.shrink2(Vec2::new(20.0, 10.0)))
                        .layout(egui::Layout::left_to_right(egui::Align::Center)),
                );
                if !self.is_last() && foot.link(RichText::new("跳过").color(SUB)).clicked() {
                    changed |= self.finish(config);
                    outcome = Outcome::Finished;
                }
                foot.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let (label, primary) = if self.is_last() {
                        ("完成", true)
                    } else {
                        ("下一步", true)
                    };
                    let button = egui::Button::new(RichText::new(label).color(if primary {
                        Color32::WHITE
                    } else {
                        TEXT
                    }))
                    .fill(ACCENT)
                    .corner_radius(6.0)
                    .min_size(Vec2::new(76.0, 28.0));
                    if ui.add(button).clicked() {
                        if self.is_last() {
                            changed |= self.finish(config);
                            outcome = Outcome::Finished;
                        } else {
                            self.next();
                        }
                    }
                    if self.step > 0
                        && ui
                            .add(
                                egui::Button::new("上一步")
                                    .fill(Color32::WHITE)
                                    .stroke(Stroke::new(1.0_f32, LINE))
                                    .corner_radius(6.0)
                                    .min_size(Vec2::new(68.0, 28.0)),
                            )
                            .clicked()
                    {
                        self.back();
                    }
                });
            });
        if outcome == Outcome::Open && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            outcome = Outcome::Closed;
        }
        if std::mem::take(&mut self.open_hotkey_settings) && outcome == Outcome::Open {
            outcome = Outcome::OpenHotkeySettings;
        }
        (changed, outcome)
    }
    /// The try-it step needs the hotkey delivered to this window, so the
    /// daemon's global grab must be paused while it is shown.
    /// A hotkey press delivered outside egui (Linux X11 grab) while this
    /// window is focused: format into the try box, focusing it if needed.
    pub fn native_hotkey(&mut self, ctx: &egui::Context, config: &Config) {
        if self.step != 2 {
            return;
        }
        ctx.memory_mut(|m| m.request_focus(egui::Id::new(TRY_INPUT)));
        self.try_paste(config);
        ctx.request_repaint();
    }

    pub fn wants_hotkey_pause(&self) -> bool {
        self.step == 2
    }

    fn step_indicator(&self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            for (index, name) in ["怎么用", "权限", "试一试"].into_iter().enumerate() {
                let done = index < self.step;
                let current = index == self.step;
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(18.0), egui::Sense::hover());
                let fill = if current || done {
                    ACCENT
                } else {
                    Color32::from_rgb(240, 240, 243)
                };
                ui.painter().circle_filled(rect.center(), 9.0, fill);
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    if done {
                        "✓".to_owned()
                    } else {
                        (index + 1).to_string()
                    },
                    FontId::proportional(11.0),
                    if current || done { Color32::WHITE } else { SUB },
                );
                ui.label(
                    RichText::new(name)
                        .size(12.0)
                        .color(if current { TEXT } else { MUTED }),
                );
                if index + 1 < STEPS {
                    let (line, _) =
                        ui.allocate_exact_size(Vec2::new(28.0, 18.0), egui::Sense::hover());
                    ui.painter()
                        .hline(line.x_range(), line.center().y, Stroke::new(1.0_f32, LINE));
                }
            }
        });
        ui.add_space(4.0);
    }

    fn step_permission(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("授予权限").size(15.0));
        match self.permission {
            Permission::NotRequired => {
                ui.label(RichText::new("当前平台模拟粘贴不需要额外授权。").color(SUB));
                status_row(ui, true, "无需权限");
            }
            Permission::Granted | Permission::Missing => {
                ui.label(
                    RichText::new(
                        "CleanCopy 需要「辅助功能」权限来读取前台 App 并模拟 ⌘V 粘贴。授权后此处自动打勾。",
                    )
                    .color(SUB),
                );
                let granted = self.permission == Permission::Granted;
                status_row(
                    ui,
                    granted,
                    if granted {
                        "辅助功能：已授权"
                    } else {
                        "辅助功能：未授权"
                    },
                );
                if !granted && ui.button("请求授权并打开系统设置").clicked() {
                    request_permission();
                    self.polled_at = 0.0;
                }
            }
        }
    }

    fn step_try(&mut self, ui: &mut egui::Ui, config: &Config, input_id: egui::Id) {
        let keys = hotkey_text(&config.hotkey);
        ui.label(RichText::new("试一试").size(15.0));
        ui.label(
            RichText::new(format!(
                "点击下面的输入框，按 {keys} 粘贴示例 Markdown 的转换效果。"
            ))
            .size(12.0)
            .color(SUB),
        );
        ui.horizontal_top(|ui| {
            egui::Frame::new()
                .fill(Color32::from_rgb(250, 250, 251))
                .stroke(Stroke::new(1.0_f32, LINE))
                .corner_radius(8.0)
                .inner_margin(8.0)
                .show(ui, |ui| {
                    ui.set_width(160.0);
                    ui.set_height(TRY_BOX_HEIGHT);
                    ui.label(RichText::new(SAMPLE).monospace().size(11.0).color(SUB));
                });
            ui.vertical(|ui| {
                egui::Frame::new()
                    .stroke(Stroke::new(1.0_f32, if self.tried { ACCENT } else { LINE }))
                    .corner_radius(8.0)
                    .inner_margin(6.0)
                    .show(ui, |ui| {
                        egui::ScrollArea::vertical()
                            .id_salt("try_scroll")
                            .max_height(TRY_BOX_HEIGHT + 4.0)
                            .show(ui, |ui| {
                                ui.add(
                                    egui::TextEdit::multiline(&mut self.input)
                                        .id(input_id)
                                        .frame(false)
                                        .desired_width(212.0)
                                        .desired_rows(4)
                                        .hint_text(format!("在这里按 {keys}")),
                                );
                            });
                    });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    if self.tried {
                        ui.label(RichText::new("✓").size(13.0).strong().color(SUCCESS));
                        ui.label(
                            RichText::new("已按「纯文本 · 中文序号」粘贴")
                                .size(12.0)
                                .color(SUCCESS),
                        );
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add(
                                egui::Label::new(
                                    RichText::new("更改快捷键")
                                        .size(11.0)
                                        .color(SUB)
                                        .underline(),
                                )
                                .sense(egui::Sense::click()),
                            )
                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                            .clicked()
                        {
                            self.open_hotkey_settings = true;
                        }
                    });
                });
            });
        });
    }
}

/// Height of the step-3 sample / result boxes; leaves room for the success
/// hint plus a ≥12px gap above the footer bar.
const TRY_BOX_HEIGHT: f32 = 96.0;
/// Success green, 5.0:1 on white (WCAG AA).
pub(crate) const SUCCESS: Color32 = Color32::from_rgb(21, 128, 61);

fn step_usage(ui: &mut egui::Ui, config: &Config) {
    ui.label(RichText::new("复制，然后用快捷键粘贴").size(15.0));
    ui.label(
        RichText::new(
            "在任何地方复制文本，回到目标 App 按快捷键，CleanCopy 会按目标 App 自动转换格式后粘贴。",
        )
        .color(SUB),
    );
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        key_cap_wide(
            ui,
            if cfg!(target_os = "macos") {
                "⌘C 复制"
            } else {
                "复制"
            },
        );
        ui.label(RichText::new("→").size(16.0).color(MUTED));
        for key in hotkey_labels(&config.hotkey) {
            key_cap_wide(ui, &key);
        }
    });
    ui.add_space(14.0);
    for (hotkey, text) in [
        (&config.hotkey, "智能粘贴"),
        (&config.full_menu_hotkey, "打开完整菜单"),
    ] {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            for key in hotkey_labels(hotkey) {
                small_key_cap(ui, &key);
            }
            ui.add_space(4.0);
            ui.label(RichText::new(text).size(12.0).color(SUB));
        });
        ui.add_space(2.0);
    }
}

/// Hotkey as text with `+` separators (macOS symbols run together, ⌘⇧V).
pub(crate) fn hotkey_text(hotkey: &str) -> String {
    hotkey_labels(hotkey).join(if cfg!(target_os = "macos") { "" } else { "+" })
}

fn small_key_cap(ui: &mut egui::Ui, text: &str) {
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_owned(), FontId::proportional(11.0), TEXT);
    let width = (galley.size().x + 10.0).max(20.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 20.0), egui::Sense::hover());
    ui.painter().rect(
        rect,
        4.0,
        Color32::from_rgb(247, 247, 248),
        Stroke::new(1.0_f32, LINE),
        egui::StrokeKind::Inside,
    );
    ui.painter()
        .galley(rect.center() - galley.size() / 2.0, galley, TEXT);
}

fn key_cap_wide(ui: &mut egui::Ui, text: &str) {
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_owned(), FontId::proportional(13.0), TEXT);
    let width = (galley.size().x + 16.0).max(30.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 30.0), egui::Sense::hover());
    ui.painter().rect(
        rect,
        6.0,
        Color32::from_rgb(247, 247, 248),
        Stroke::new(1.0_f32, LINE),
        egui::StrokeKind::Inside,
    );
    ui.painter()
        .galley(rect.center() - galley.size() / 2.0, galley, TEXT);
}

fn status_row(ui: &mut egui::Ui, ok: bool, text: &str) {
    egui::Frame::new()
        .fill(if ok {
            Color32::from_rgb(240, 240, 251)
        } else {
            Color32::from_rgb(255, 244, 229)
        })
        .corner_radius(8.0)
        .inner_margin(egui::Margin::symmetric(12, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(
                RichText::new(format!("{} {text}", if ok { "✓" } else { "○" })).color(if ok {
                    ACCENT
                } else {
                    Color32::from_rgb(150, 80, 8)
                }),
            );
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn navigation_is_clamped_to_three_steps() {
        let mut state = OnboardingState::default();
        state.back();
        assert_eq!(state.step, 0);
        state.next();
        state.next();
        assert!(state.is_last());
        state.next();
        assert_eq!(state.step, 2);
        state.back();
        assert_eq!(state.step, 1);
    }

    #[test]
    fn finish_persists_flag_once() {
        let mut config = Config::default();
        assert!(!config.onboarding_done);
        let mut state = OnboardingState::default();
        assert!(state.finish(&mut config));
        assert!(config.onboarding_done);
        assert!(!state.finish(&mut config));
    }

    #[test]
    fn legacy_config_without_flag_shows_onboarding_and_flag_round_trips() {
        let legacy: Config = toml::from_str("hotkey = 'Ctrl+Shift+V'").unwrap();
        assert!(!legacy.onboarding_done);
        let done = Config {
            onboarding_done: true,
            ..Default::default()
        };
        let back: Config = toml::from_str(&toml::to_string(&done).unwrap()).unwrap();
        assert!(back.onboarding_done);
    }

    #[test]
    #[cfg(not(target_os = "macos"))]
    fn non_macos_needs_no_permission() {
        assert_eq!(permission_status(), Permission::NotRequired);
    }

    #[test]
    fn native_hotkey_formats_only_on_the_try_step_and_focuses_the_box() {
        let ctx = egui::Context::default();
        let config = Config::default();
        let mut state = OnboardingState::default();
        for step in 0..2 {
            state.step = step;
            state.native_hotkey(&ctx, &config);
            assert!(!state.tried && state.input.is_empty(), "step {step}");
        }
        state.step = 2;
        // Fast press: no egui key/paste event and no modifiers held this frame.
        state.native_hotkey(&ctx, &config);
        state.native_hotkey(&ctx, &config);
        assert!(state.tried);
        assert_eq!(
            state.input.matches("一、本周进展").count(),
            2,
            "{}",
            state.input
        );
        assert!(!state.input.contains("**"));
        assert!(ctx.memory(|m| m.has_focus(egui::Id::new(TRY_INPUT))));
    }

    #[test]
    fn try_paste_inserts_formatted_sample() {
        let mut state = OnboardingState::default();
        state.try_paste(&Config::default());
        assert!(state.tried);
        assert!(state.input.contains("一、本周进展"), "{}", state.input);
        assert!(!state.input.contains("**"));
    }

    fn run(
        ctx: &egui::Context,
        state: &mut OnboardingState,
        config: &mut Config,
        events: Vec<egui::Event>,
    ) -> Outcome {
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, SIZE)),
                events,
                ..Default::default()
            },
            |ctx| {
                let (_, outcome) = state.show(ctx, config);
                if outcome == Outcome::Finished {
                    state.step = 99;
                }
            },
        );
        if state.step == 99 {
            Outcome::Finished
        } else {
            Outcome::Open
        }
    }

    #[test]
    fn hotkey_paste_formats_but_plain_paste_stays_raw() {
        let config = Config::default();
        // The default hotkey is Super+Shift+V on macOS, Ctrl+Shift+V elsewhere.
        let primary = if cfg!(target_os = "macos") {
            egui::Modifiers::MAC_CMD | egui::Modifiers::COMMAND
        } else {
            egui::Modifiers::CTRL | egui::Modifiers::COMMAND
        };
        for (modifiers, formatted) in [(primary | egui::Modifiers::SHIFT, true), (primary, false)] {
            let ctx = egui::Context::default();
            let mut state = OnboardingState {
                step: 2,
                ..Default::default()
            };
            let mut cfg = config.clone();
            for events in [vec![], vec![egui::Event::Paste("# raw".into())]] {
                let _ = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, SIZE)),
                        modifiers,
                        events,
                        ..Default::default()
                    },
                    |ctx| {
                        ctx.memory_mut(|m| m.request_focus(egui::Id::new("onboarding_try_input")));
                        state.show(ctx, &mut cfg);
                    },
                );
            }
            assert_eq!(state.tried, formatted, "{modifiers:?}");
            assert_eq!(state.input.contains("# raw"), !formatted, "{}", state.input);
        }
    }

    /// Runs step 3 with the try-it input focused and returns whether the
    /// final frame's events counted as the configured hotkey.
    fn try_step_hit(hotkey: &str, modifiers: egui::Modifiers, event: egui::Event) -> bool {
        let ctx = egui::Context::default();
        let mut cfg = Config {
            hotkey: hotkey.into(),
            ..Config::default()
        };
        let mut state = OnboardingState {
            step: 2,
            ..Default::default()
        };
        for events in [vec![], vec![event]] {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, SIZE)),
                    modifiers,
                    events,
                    ..Default::default()
                },
                |ctx| {
                    ctx.memory_mut(|m| m.request_focus(egui::Id::new("onboarding_try_input")));
                    state.show(ctx, &mut cfg);
                },
            );
        }
        state.tried
    }

    fn key(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: Some(key),
            pressed: true,
            repeat: false,
            modifiers,
        }
    }

    #[test]
    fn try_step_follows_a_non_v_configured_hotkey() {
        use egui::{Key, Modifiers as M};
        let ctrl_alt = M::CTRL | M::COMMAND | M::ALT;
        let ctrl_shift = M::CTRL | M::COMMAND | M::SHIFT;
        // Ordinary key event: Ctrl+Alt+K.
        assert!(try_step_hit("Ctrl+Alt+K", ctrl_alt, key(Key::K, ctrl_alt)));
        assert!(!try_step_hit(
            "Ctrl+Alt+K",
            ctrl_shift,
            key(Key::K, ctrl_shift)
        ));
        assert!(!try_step_hit(
            "Ctrl+Alt+K",
            ctrl_shift,
            key(Key::V, ctrl_shift)
        ));
        // The default V chord no longer triggers once the hotkey is K.
        assert!(!try_step_hit(
            "Ctrl+Alt+K",
            ctrl_shift,
            egui::Event::Paste("x".into())
        ));
        // Function key and a Copy-translated chord.
        assert!(try_step_hit(
            "Ctrl+Shift+F8",
            ctrl_shift,
            key(Key::F8, ctrl_shift)
        ));
        assert!(try_step_hit("Ctrl+Shift+C", ctrl_shift, egui::Event::Copy));
        assert!(!try_step_hit("Ctrl+Shift+C", ctrl_shift, egui::Event::Cut));
        assert!(try_step_hit("Ctrl+Shift+X", ctrl_shift, egui::Event::Cut));
    }

    #[test]
    fn hotkey_text_uses_plus_separators() {
        if cfg!(target_os = "macos") {
            assert_eq!(hotkey_text("Super+Shift+V"), "⌘⇧V");
        } else {
            assert_eq!(hotkey_text("Ctrl+Shift+V"), "Ctrl+Shift+V");
            assert_eq!(hotkey_text("Ctrl+Alt+K"), "Ctrl+Alt+K");
        }
    }

    #[test]
    fn success_green_meets_aa_on_white() {
        let ratio = super::super::palette::contrast(SUCCESS, Color32::WHITE);
        assert!(ratio >= 4.5, "{ratio}");
    }

    #[test]
    fn enter_walks_all_steps_and_finishes() {
        let ctx = egui::Context::default();
        let mut state = OnboardingState::default();
        let mut config = Config::default();
        let enter = || {
            vec![egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }]
        };
        assert_eq!(run(&ctx, &mut state, &mut config, vec![]), Outcome::Open);
        run(&ctx, &mut state, &mut config, enter());
        assert_eq!(state.step, 1);
        run(&ctx, &mut state, &mut config, enter());
        assert_eq!(state.step, 2);
        assert_eq!(
            run(&ctx, &mut state, &mut config, enter()),
            Outcome::Finished
        );
        assert!(config.onboarding_done);
    }

    #[test]
    fn escape_closes_on_every_step_without_finishing() {
        for step in 0..STEPS {
            let ctx = egui::Context::default();
            let mut state = OnboardingState {
                step,
                ..Default::default()
            };
            let mut config = Config::default();
            let mut outcome = Outcome::Open;
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, SIZE)),
                    events: vec![egui::Event::Key {
                        key: egui::Key::Escape,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    }],
                    ..Default::default()
                },
                |ctx| outcome = state.show(ctx, &mut config).1,
            );
            assert_eq!(outcome, Outcome::Closed, "step {step}");
            assert!(!config.onboarding_done);
        }
    }
}
