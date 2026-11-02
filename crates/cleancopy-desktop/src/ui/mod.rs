//! Stage B UI. Native clipboard, hotkey, and tray registration are not installed.
pub mod backdrop;
pub mod fonts;
pub mod menus;
pub mod settings;

use crate::{
    config::{AppIdentity, Config, Format},
    engine::{Engine, Event, MenuModel, Platform, Snapshot},
};
use eframe::egui;
use std::{convert::Infallible, path::PathBuf, time::Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum View {
    Menu,
    MenuSecondary,
    Settings,
    Tray,
}

/// In-memory effects deliberately cannot touch the OS clipboard or inject input.
#[derive(Default)]
struct DemoPlatform {
    menu: Option<MenuModel>,
    clipboard: Snapshot,
    now: u64,
    notice: String,
}
impl Platform for DemoPlatform {
    type Error = Infallible;
    fn frontmost_app(&mut self) -> Result<AppIdentity, Self::Error> {
        Ok(AppIdentity {
            name: "Demo Editor".into(),
            exe: Some("cleancopy-demo-editor".into()),
            ..Default::default()
        })
    }
    fn read_clipboard(&mut self) -> Result<Snapshot, Self::Error> {
        Ok(self.clipboard.clone())
    }
    fn write_clipboard(&mut self, snapshot: Snapshot) -> Result<(), Self::Error> {
        self.clipboard = snapshot;
        Ok(())
    }
    fn send_paste(&mut self) -> Result<(), Self::Error> {
        self.notice = "预览：已模拟粘贴，未更改系统剪贴板".into();
        Ok(())
    }
    fn send_undo(&mut self) -> Result<(), Self::Error> {
        self.notice = "预览：已模拟撤销".into();
        Ok(())
    }
    fn show_menu(&mut self, model: MenuModel) -> Result<(), Self::Error> {
        self.menu = Some(model);
        Ok(())
    }
    fn hide_menu(&mut self) -> Result<(), Self::Error> {
        self.menu = None;
        Ok(())
    }
    fn notify(&mut self, message: &str) -> Result<(), Self::Error> {
        self.notice = message.into();
        Ok(())
    }
    fn now(&self) -> u64 {
        self.now
    }
}

pub struct DesktopApp {
    view: View,
    engine: Engine<DemoPlatform>,
    settings: settings::SettingsState,
    menu: menus::MenuState,
    save_path: Option<PathBuf>,
    error: Option<String>,
    enabled: bool,
    started: Instant,
    epoch: u64,
}
impl DesktopApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        view: View,
        config: Config,
        save_path: Option<PathBuf>,
    ) -> Self {
        if let Some(path) = fonts::install(&cc.egui_ctx) {
            eprintln!("CJK font: {}", path.display());
        }
        let mut style = (*cc.egui_ctx.style()).clone();
        style.visuals = egui::Visuals::light();
        style.override_font_id = Some(egui::FontId::proportional(13.0));
        style.visuals.selection.bg_fill = egui::Color32::from_rgb(91, 91, 214);
        cc.egui_ctx.set_style(style);
        let source = "# 项目周报\n## 本周进展\n- 完成 **剪贴板监听**\n- 接入 Word 富文本\n  - 标题映射 标题1/2\n  - 表格与代码块\n## 下周计划\n1. Windows 托盘\n2. 识别失败兜底";
        let epoch = now_unix_ms();
        let enabled = config.paused_until.is_none_or(|until| until <= epoch);
        let mut app = Self {
            view,
            engine: Engine::new(
                config,
                DemoPlatform {
                    clipboard: Snapshot::text(source),
                    now: epoch,
                    ..Default::default()
                },
            ),
            settings: Default::default(),
            menu: Default::default(),
            save_path,
            error: None,
            enabled,
            started: Instant::now(),
            epoch,
        };
        if view != View::Settings {
            app.sync_pause(epoch);
        }
        if matches!(view, View::Menu | View::MenuSecondary) {
            app.open_menu();
        }
        app
    }
    fn open_menu(&mut self) {
        self.dispatch(Event::Resume { t: self.now() });
        self.engine.platform_mut().notice.clear();
        self.dispatch(Event::FullMenuHotkeyPressed { t: self.now() });
    }
    fn now(&self) -> u64 {
        self.epoch + self.started.elapsed().as_millis() as u64
    }
    fn dispatch(&mut self, event: Event) {
        self.engine.platform_mut().now = self.now();
        self.engine.handle(event).unwrap();
        match event {
            Event::Resume { .. } => self.enabled = true,
            Event::Pause { .. } => self.enabled = false,
            _ => {}
        }
        self.save();
    }
    fn sync_pause(&mut self, now: u64) {
        if let Some(until) = self.engine.config().paused_until {
            if now >= until {
                self.dispatch(Event::Resume { t: now });
            } else {
                self.enabled = false;
            }
        }
    }
    fn handle_tray_action(&mut self, ctx: &egui::Context, action: menus::TrayAction, now: u64) {
        match action {
            menus::TrayAction::ShowMenu => {
                self.view = View::Menu;
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(960.0, 600.0)));
                self.open_menu();
            }
            menus::TrayAction::Undo => self.dispatch(Event::Undo { t: now }),
            menus::TrayAction::SetEnabled(enabled) => self.dispatch(if enabled {
                Event::Resume { t: now }
            } else {
                Event::Pause {
                    t: now,
                    until: None,
                }
            }),
            menus::TrayAction::Pause => self.dispatch(Event::Pause {
                t: now,
                until: Some(now.saturating_add(3_600_000)),
            }),
            menus::TrayAction::Settings => {
                self.view = View::Settings;
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(1120.0, 720.0)));
            }
            menus::TrayAction::Quit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
        }
        ctx.request_repaint();
    }
    fn save(&mut self) {
        if let Some(path) = &self.save_path {
            if let Err(error) = self.engine.config().save(path) {
                self.error = Some(error.to_string());
            }
        }
    }
}
impl eframe::App for DesktopApp {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        let now = self.now();
        self.engine.platform_mut().now = now;
        if self.view != View::Settings {
            self.sync_pause(now);
        }
        if let Some(until) = self.engine.config().paused_until {
            ctx.request_repaint_after(std::time::Duration::from_millis(until.saturating_sub(now)));
        }
        if let Some(deadline) = self.engine.next_deadline() {
            if now >= deadline {
                self.engine.tick().unwrap();
            }
            ctx.request_repaint_after(std::time::Duration::from_millis(
                deadline.saturating_sub(now),
            ));
        }
        match self.view {
            View::Settings => {
                // Refresh before editing so history and tray changes are not written
                // back from a stale settings-window snapshot.
                if let Some(path) = &self.save_path {
                    match Config::load(path) {
                        Ok(config) => self.engine.update_config(config).unwrap(),
                        Err(error) => self.error = Some(error.to_string()),
                    }
                    ctx.request_repaint_after(std::time::Duration::from_millis(250));
                }
                let previous = self.engine.config().clone();
                let mut config = previous.clone();
                if self.settings.show(ctx, &mut config) {
                    if let Some(path) = &self.save_path {
                        match config.save_changes(path, &previous) {
                            Ok(merged) => config = merged,
                            Err(error) => self.error = Some(error.to_string()),
                        }
                    }
                    self.engine.update_config(config).unwrap();
                }
            }
            View::Menu | View::MenuSecondary => {
                if let Some(model) = self.engine.platform().menu.clone() {
                    for event in self
                        .menu
                        .show(ctx, &model, self.view == View::MenuSecondary)
                    {
                        let event = match event {
                            Event::MenuChoice { id, choice, .. } => {
                                Event::MenuChoice { t: now, id, choice }
                            }
                            Event::Remember { id, checked, .. } => Event::Remember {
                                t: now,
                                id,
                                checked,
                            },
                            Event::MenuKey { id, key, .. } => Event::MenuKey { t: now, id, key },
                            _ => Event::Cancel { t: now },
                        };
                        self.dispatch(event);
                    }
                } else {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        ui.centered_and_justified(|ui| {
                            ui.vertical_centered(|ui| {
                                ui.label(&self.engine.platform().notice);
                                ui.label("UI 预览 · 不操作系统剪贴板");
                                if ui.button("重新打开粘贴菜单").clicked() {
                                    self.open_menu();
                                }
                            });
                        });
                    });
                }
            }
            View::Tray => {
                let action = menus::tray(ctx, self.engine.config(), &mut self.enabled);
                if let Some(action) = action {
                    self.handle_tray_action(ctx, action, now);
                }
            }
        }
        if let Some(error) = self.error.clone() {
            egui::Window::new("无法保存设置")
                .collapsible(false)
                .show(ctx, |ui| {
                    ui.colored_label(egui::Color32::DARK_RED, error);
                    if ui.button("重试").clicked() {
                        self.error = None;
                        self.save();
                    }
                });
        }
    }
}
fn now_unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

pub fn demo_config() -> Config {
    let mut config = Config::default();
    let now = now_unix_ms();
    for (name, format, age) in [
        ("Microsoft Word", Format::WordRichText, 5 * 60_000),
        ("TextEdit", Format::ChineseNumbered, 2 * 60_000),
    ] {
        config.record_paste(
            &AppIdentity {
                name: name.into(),
                ..Default::default()
            },
            Some(format),
            now.saturating_sub(age),
        );
    }
    config
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(config: Config) -> DesktopApp {
        DesktopApp {
            view: View::Tray,
            enabled: config.paused_until.is_none_or(|until| until <= 1_000),
            engine: Engine::new(
                config,
                DemoPlatform {
                    clipboard: Snapshot::text("# Test clipboard"),
                    ..Default::default()
                },
            ),
            settings: Default::default(),
            menu: Default::default(),
            save_path: None,
            error: None,
            started: Instant::now(),
            epoch: 1_000,
        }
    }

    #[test]
    fn status_toggle_pauses_indefinitely_and_resumes() {
        let ctx = egui::Context::default();
        let mut app = app(Config::default());
        app.handle_tray_action(&ctx, menus::TrayAction::SetEnabled(false), 1_000);
        assert!(!app.enabled);
        assert_eq!(app.engine.config().paused_until, None);
        app.sync_pause(4_000_000);
        assert!(!app.enabled);
        app.engine
            .handle(Event::HotkeyPressed { t: 4_000_000 })
            .unwrap();
        assert_eq!(app.engine.next_deadline(), None);

        app.handle_tray_action(&ctx, menus::TrayAction::SetEnabled(true), 4_000_001);
        assert!(app.enabled);
        assert_eq!(app.engine.config().paused_until, None);
        app.engine
            .handle(Event::HotkeyPressed { t: 4_000_002 })
            .unwrap();
        assert!(app.engine.platform().menu.is_some());
    }

    #[test]
    fn timed_pause_uses_event_clock_and_expires_at_boundary() {
        let ctx = egui::Context::default();
        let mut app = app(Config::default());
        app.handle_tray_action(&ctx, menus::TrayAction::Pause, 1_000);
        assert!(!app.enabled);
        assert_eq!(app.engine.config().paused_until, Some(3_601_000));
        app.sync_pause(3_600_999);
        assert!(!app.enabled);
        app.sync_pause(3_601_000);
        assert!(app.enabled);
        assert_eq!(app.engine.config().paused_until, None);
    }

    #[test]
    fn configured_pause_and_menu_resume_stay_in_sync() {
        let mut app = app(Config {
            paused_until: Some(2_000),
            ..Default::default()
        });
        assert!(!app.enabled);
        app.sync_pause(1_000);
        assert!(!app.enabled);
        app.open_menu();
        assert!(app.enabled);
        assert_eq!(app.engine.config().paused_until, None);
        assert!(app.engine.platform().menu.is_some());

        let mut expired = self::app(Config {
            paused_until: Some(999),
            ..Default::default()
        });
        expired.sync_pause(1_000);
        assert!(expired.enabled);
        assert_eq!(expired.engine.config().paused_until, None);
    }

    #[test]
    fn tray_navigation_resizes_viewport() {
        for (action, view, size) in [
            (
                menus::TrayAction::ShowMenu,
                View::Menu,
                egui::vec2(960.0, 600.0),
            ),
            (
                menus::TrayAction::Settings,
                View::Settings,
                egui::vec2(1120.0, 720.0),
            ),
        ] {
            let ctx = egui::Context::default();
            let mut app = app(Config::default());
            let output = ctx.run(Default::default(), |ctx| {
                app.handle_tray_action(ctx, action, 1_000)
            });
            assert_eq!(app.view, view);
            assert!(output.viewport_output[&egui::ViewportId::ROOT]
                .commands
                .contains(&egui::ViewportCommand::InnerSize(size)));
        }
    }

    #[test]
    fn demo_history_is_recent_and_newest_first() {
        let before = now_unix_ms();
        let config = demo_config();
        let after = now_unix_ms();
        assert_eq!(config.recent_pastes.len(), 2);
        for (recent, age) in config.recent_pastes.iter().zip([2 * 60_000, 5 * 60_000]) {
            assert!((before.saturating_sub(age)..=after.saturating_sub(age))
                .contains(&recent.timestamp));
        }
        assert!(config.recent_pastes[0].timestamp > config.recent_pastes[1].timestamp);
    }
}
