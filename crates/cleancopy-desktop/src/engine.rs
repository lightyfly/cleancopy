//! Deterministic, GUI-free desktop paste engine.
//!
//! Drivers supply nondecreasing Unix millisecond timestamps and schedule
//! [`Engine::next_deadline`] for clipboard restoration. Hotkeys act immediately.
//! Stale events are ignored. Menu callbacks
//! must carry the displayed menu ID; menus MUST be nonactivating. App identity
//! fields must identify the actual application, not a window title. Clipboard
//! adapters preserve every native format in `Snapshot`, additionally exposing
//! canonical UTF-8 MIME text formats. No native clipboard code lives here.

use crate::config::{AppIdentity, Config, Format};
use cleancopy_core::{
    is_markdown, render_html, render_numbered, render_plain, render_rtf, render_word_html,
};
use std::collections::BTreeMap;

pub const RESTORE_DELAY_MS: u64 = 150;
pub const MAX_TEXT_BYTES: usize = 5 * 1024 * 1024;
pub const TEXT_PLAIN: &str = "text/plain";
pub const TEXT_MARKDOWN: &str = "text/markdown";
pub const TEXT_HTML: &str = "text/html";
pub const TEXT_RTF: &str = "text/rtf";

/// Exact native bytes, including opaque formats; equality compares all formats.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Snapshot {
    pub formats: BTreeMap<String, Vec<u8>>,
}
impl Snapshot {
    pub fn text(text: impl Into<String>) -> Self {
        let mut result = Self::default();
        result
            .formats
            .insert(TEXT_PLAIN.into(), text.into().into_bytes());
        result
    }
    /// Prefer an explicitly supplied Markdown representation, without rewriting it.
    pub fn source_text(&self) -> Option<&str> {
        self.formats
            .get(TEXT_MARKDOWN)
            .or_else(|| self.formats.get(TEXT_PLAIN))
            .filter(|bytes| bytes.len() <= MAX_TEXT_BYTES)
            .and_then(|bytes| std::str::from_utf8(bytes).ok())
    }
}

/// All effects are fallible. A failed write must leave the clipboard unchanged;
/// successful writes replace the complete set of formats. Input injection must
/// target the previously verified foreground app synchronously. `notify` must
/// not activate another application. No effect is allowed to reenter the engine.
pub trait Platform {
    type Error;
    fn frontmost_app(&mut self) -> Result<AppIdentity, Self::Error>;
    fn read_clipboard(&mut self) -> Result<Snapshot, Self::Error>;
    fn write_clipboard(&mut self, snapshot: Snapshot) -> Result<(), Self::Error>;
    fn send_paste(&mut self) -> Result<(), Self::Error>;
    fn send_undo(&mut self) -> Result<(), Self::Error>;
    fn show_menu(&mut self, model: MenuModel) -> Result<(), Self::Error>;
    fn hide_menu(&mut self) -> Result<(), Self::Error>;
    fn notify(&mut self, message: &str) -> Result<(), Self::Error>;
    /// Nondecreasing Unix milliseconds, shared with persisted pause/history values.
    fn now(&self) -> u64;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuKind {
    Full,
    Secondary,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    Original,
    Format(Format),
    Undo,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatItem {
    pub format: Format,
    /// Numeric accelerator, starting at 2 (1 is Original).
    pub shortcut: u8,
    /// False only when conversion is unsafe/unavailable (huge, binary, image).
    pub enabled: bool,
    /// Plain text remains explicitly convertible, but is visually de-emphasized.
    pub muted: bool,
}
/// Full menus contain Original, a formatted submenu, and Undo. Secondary menus
/// contain enabled configured formats plus Original and a remember checkbox.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuModel {
    pub id: u64,
    pub kind: MenuKind,
    pub app: AppIdentity,
    pub formats: Vec<FormatItem>,
    /// Initial highlighted action; ordinary or unconvertible content uses Original.
    pub default_choice: Choice,
    pub content_label: String,
    pub original_enabled: bool,
    pub undo_enabled: bool,
    pub show_remember: bool,
    pub remember: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuKey {
    Digit(u8),
    Enter,
    Escape,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    MenuKey {
        t: u64,
        id: u64,
        key: MenuKey,
    },
    HotkeyPressed {
        t: u64,
    },
    FullMenuHotkeyPressed {
        t: u64,
    },
    Timer {
        t: u64,
    },
    MenuChoice {
        t: u64,
        id: u64,
        choice: Choice,
    },
    Remember {
        t: u64,
        id: u64,
        checked: bool,
    },
    Cancel {
        t: u64,
    },
    /// `None` pauses indefinitely; `Some` is a Unix millisecond expiry.
    Pause {
        t: u64,
        until: Option<u64>,
    },
    Resume {
        t: u64,
    },
    Undo {
        t: u64,
    },
}
impl Event {
    fn time(self) -> u64 {
        match self {
            Self::MenuKey { t, .. }
            | Self::HotkeyPressed { t }
            | Self::FullMenuHotkeyPressed { t }
            | Self::Timer { t }
            | Self::MenuChoice { t, .. }
            | Self::Remember { t, .. }
            | Self::Cancel { t }
            | Self::Pause { t, .. }
            | Self::Resume { t }
            | Self::Undo { t } => t,
        }
    }
}

struct Capture {
    app: AppIdentity,
    original: Snapshot,
    markdown: bool,
    convertible: bool,
}
struct Menu {
    capture: Capture,
    model: MenuModel,
}
struct Restoration {
    original: Snapshot,
    written: Snapshot,
    deadline: u64,
}
struct Undo {
    app: AppIdentity,
    original: Snapshot,
}

/// Owns injected effects and mutable configuration; the caller persists config
/// after remembered choices/history changes. Errors propagate, but restoration
/// state survives failures and blocks overlapping transactions until recovered.
pub struct Engine<P: Platform> {
    platform: P,
    config: Config,
    now: u64,
    serial: u64,
    paused: bool,
    menu: Option<Menu>,
    restoring: Option<Restoration>,
    undo: Option<Undo>,
}
impl<P: Platform> Engine<P> {
    pub fn new(config: Config, platform: P) -> Self {
        Self {
            platform,
            config,
            now: 0,
            serial: 0,
            paused: false,
            menu: None,
            restoring: None,
            undo: None,
        }
    }
    pub fn config(&self) -> &Config {
        &self.config
    }
    /// Apply settings only after cancelling an open menu. A clipboard
    /// restoration already in flight is retained and serviced normally.
    pub fn update_config(&mut self, config: Config) -> Result<(), P::Error> {
        self.cancel()?;
        self.config = config;
        Ok(())
    }
    pub fn platform(&self) -> &P {
        &self.platform
    }
    pub fn platform_mut(&mut self) -> &mut P {
        &mut self.platform
    }
    pub fn next_deadline(&self) -> Option<u64> {
        self.restoring.as_ref().map(|r| r.deadline)
    }
    /// Dispatch a native hotkey using the injected clock.
    pub fn hotkey_pressed(&mut self) -> Result<(), P::Error> {
        self.handle(Event::HotkeyPressed {
            t: self.platform.now(),
        })
    }
    /// Open the full menu immediately using the injected clock.
    pub fn full_menu_hotkey_pressed(&mut self) -> Result<(), P::Error> {
        self.handle(Event::FullMenuHotkeyPressed {
            t: self.platform.now(),
        })
    }
    /// Service scheduled work using the same clock as native hotkeys.
    pub fn tick(&mut self) -> Result<(), P::Error> {
        self.handle(Event::Timer {
            t: self.platform.now(),
        })
    }
    pub fn handle(&mut self, event: Event) -> Result<(), P::Error> {
        let t = event.time();
        if t < self.now {
            return Ok(());
        }
        self.now = t;
        match event {
            Event::MenuKey { id, key, .. } => {
                let Some(menu) = self.menu.as_ref().filter(|menu| menu.model.id == id) else {
                    return Ok(());
                };
                let choice = match key {
                    MenuKey::Escape => return self.cancel(),
                    MenuKey::Enter => Some(menu.model.default_choice),
                    MenuKey::Digit(1) => Some(Choice::Original),
                    MenuKey::Digit(digit) => menu
                        .model
                        .formats
                        .iter()
                        .find(|item| item.shortcut == digit)
                        .map(|item| Choice::Format(item.format)),
                };
                if let Some(choice) = choice {
                    self.handle(Event::MenuChoice { t, id, choice })?;
                }
                Ok(())
            }
            Event::Timer { .. } => {
                if self.restoring.as_ref().is_some_and(|r| t >= r.deadline) {
                    return self.restore(t);
                }
                Ok(())
            }
            Event::Cancel { .. } => self.cancel(),
            Event::Pause { until, .. } => {
                self.paused = until.is_none();
                self.config.paused_until = until;
                self.cancel()
            }
            Event::Resume { .. } => {
                self.paused = false;
                self.config.paused_until = None;
                Ok(())
            }
            Event::Remember { id, checked, .. } => {
                if let Some(menu) = self
                    .menu
                    .as_mut()
                    .filter(|m| m.model.id == id && m.model.show_remember)
                {
                    menu.model.remember = checked;
                    self.platform.show_menu(menu.model.clone())?;
                }
                Ok(())
            }
            Event::MenuChoice { id, choice, .. } => {
                if self.restoring.is_some() || !self.menu.as_ref().is_some_and(|m| m.model.id == id)
                {
                    return Ok(());
                }
                let menu = self.menu.as_ref().unwrap();
                let allowed = match choice {
                    Choice::Original => true,
                    Choice::Undo => menu.model.undo_enabled,
                    Choice::Format(format) => menu
                        .model
                        .formats
                        .iter()
                        .any(|i| i.format == format && i.enabled),
                };
                if !allowed {
                    return Ok(());
                }
                self.platform.hide_menu()?;
                let menu = self.menu.take().unwrap();
                match choice {
                    Choice::Undo => self.undo(t),
                    Choice::Original => self.paste(menu.capture, None, false, t),
                    Choice::Format(format) => {
                        self.paste(menu.capture, Some(format), menu.model.remember, t)
                    }
                }
            }
            Event::Undo { .. } => {
                if self.restoring.is_some() {
                    return Ok(());
                }
                self.cancel()?;
                self.undo(t)
            }
            Event::HotkeyPressed { .. } | Event::FullMenuHotkeyPressed { .. } => {
                if self.restoring.is_some()
                    || self.menu.is_some()
                    || self.paused
                    || self.config.paused_until.is_some_and(|until| t < until)
                {
                    return Ok(());
                }
                let app = self.platform.frontmost_app()?;
                let original = self.platform.read_clipboard()?;
                let convertible = original.source_text().is_some();
                let markdown = original.source_text().is_some_and(is_markdown);
                let capture = Capture {
                    app,
                    original,
                    convertible,
                    markdown,
                };
                if matches!(event, Event::FullMenuHotkeyPressed { .. }) || !self.config.silent_mode
                {
                    self.open_menu(capture, MenuKind::Full)
                } else if let Some(format) = self.config.format_for(&capture.app) {
                    let format = capture.markdown.then_some(format);
                    self.paste(capture, format, false, t)
                } else {
                    self.open_menu(capture, MenuKind::Secondary)
                }
            }
        }
    }
    fn cancel(&mut self) -> Result<(), P::Error> {
        if self.menu.take().is_some() {
            // Invalidate callbacks even when the adapter fails to hide the menu.
            self.platform.hide_menu()?;
        }
        // Never cancel a required restoration.
        Ok(())
    }
    fn open_menu(&mut self, capture: Capture, kind: MenuKind) -> Result<(), P::Error> {
        self.serial = self.serial.saturating_add(1);
        let formats: Vec<_> = self
            .config
            .secondary_items
            .iter()
            .filter(|item| item.enabled)
            .map(|item| item.format)
            .collect();
        let default_choice = if capture.markdown && capture.convertible {
            self.config
                .format_for(&capture.app)
                .filter(|format| formats.contains(format))
                .or_else(|| formats.first().copied())
                .map(Choice::Format)
                .unwrap_or(Choice::Original)
        } else {
            Choice::Original
        };
        let content_label = if capture
            .original
            .formats
            .get(TEXT_MARKDOWN)
            .or_else(|| capture.original.formats.get(TEXT_PLAIN))
            .is_some_and(|bytes| bytes.len() > MAX_TEXT_BYTES)
        {
            "文本过大，只能粘贴原文"
        } else if capture.markdown {
            "Markdown"
        } else if capture.convertible {
            "普通文本"
        } else if capture
            .original
            .formats
            .keys()
            .any(|key| key.starts_with("image/"))
        {
            "图片，不支持转换"
        } else {
            "不支持转换，只能粘贴原文"
        }
        .to_owned();
        let model = MenuModel {
            id: self.serial,
            kind,
            app: capture.app.clone(),
            formats: formats
                .into_iter()
                .enumerate()
                .map(|(index, format)| FormatItem {
                    format,
                    shortcut: u8::try_from(index + 2).unwrap_or(u8::MAX),
                    enabled: capture.convertible,
                    muted: !capture.markdown,
                })
                .collect(),
            default_choice,
            content_label,
            original_enabled: true,
            undo_enabled: self
                .undo
                .as_ref()
                .is_some_and(|u| same_target(&u.app, &capture.app)),
            show_remember: self.config.format_for(&capture.app).is_none(),
            remember: false,
        };
        self.platform.show_menu(model.clone())?;
        self.menu = Some(Menu { capture, model });
        Ok(())
    }
    fn target(&mut self, app: &AppIdentity) -> Result<bool, P::Error> {
        if same_target(app, &self.platform.frontmost_app()?) {
            Ok(true)
        } else {
            self.platform.notify(
                "Paste cancelled: foreground application changed or lacks a stable identity",
            )?;
            Ok(false)
        }
    }
    fn paste(
        &mut self,
        capture: Capture,
        format: Option<Format>,
        remember: bool,
        t: u64,
    ) -> Result<(), P::Error> {
        if !self.target(&capture.app)? {
            return Ok(());
        }
        if self.platform.read_clipboard()? != capture.original {
            return self
                .platform
                .notify("Paste cancelled: clipboard changed since the hotkey");
        }
        let converted = format.filter(|_| capture.convertible);
        if let Some(format) = converted {
            let written = render(&capture.original, format, self.config.numbering_style);
            self.platform.write_clipboard(written.clone())?;
            self.restoring = Some(Restoration {
                original: capture.original.clone(),
                written,
                deadline: t.saturating_add(RESTORE_DELAY_MS),
            });
        }
        // Focus may have changed while accessing the clipboard.
        if !self.target(&capture.app)? {
            return Ok(());
        }
        self.platform.send_paste()?;
        self.undo = converted.map(|_| Undo {
            app: capture.app.clone(),
            original: capture.original,
        });
        self.config.record_paste(&capture.app, converted, t);
        if remember {
            if let Some(format) = converted {
                self.config.remember(&capture.app, format);
            }
        }
        Ok(())
    }
    fn restore(&mut self, t: u64) -> Result<(), P::Error> {
        // Advance before fallible effects, avoiding a busy-loop on a past deadline.
        let restoration = self.restoring.as_mut().unwrap();
        restoration.deadline = t.saturating_add(RESTORE_DELAY_MS);
        if self.platform.read_clipboard()? != restoration.written {
            self.platform
                .notify("Clipboard restoration skipped: a newer clipboard was copied")?;
            self.restoring = None;
            return Ok(());
        }
        self.platform
            .write_clipboard(restoration.original.clone())?;
        self.restoring = None;
        Ok(())
    }
    fn undo(&mut self, t: u64) -> Result<(), P::Error> {
        let Some(last) = self.undo.as_ref() else {
            return Ok(());
        };
        let app = last.app.clone();
        if !self.target(&app)? {
            return Ok(());
        }
        let current = self.platform.read_clipboard()?;
        // Consume before the irreversible effect: never send Undo twice on errors.
        let last = self.undo.take().unwrap();
        self.platform.send_undo()?;
        if !self.target(&app)? {
            return Ok(());
        }
        self.platform.write_clipboard(last.original.clone())?;
        self.restoring = Some(Restoration {
            original: current,
            written: last.original,
            deadline: t.saturating_add(RESTORE_DELAY_MS),
        });
        if !self.target(&app)? {
            return Ok(());
        }
        self.platform.send_paste()?;
        self.config.record_paste(&app, None, t);
        Ok(())
    }
}

// Unlike config rule aliases, foreground guards require the strongest captured
// identifier to remain identical. A matching name or weaker ID cannot override it.
fn same_target(left: &AppIdentity, right: &AppIdentity) -> bool {
    for (a, b) in [
        (&left.bundle_id, &right.bundle_id),
        (&left.exe, &right.exe),
        (&left.wm_class, &right.wm_class),
    ] {
        if let Some(a) = a.as_ref().filter(|a| !a.trim().is_empty()) {
            return b.as_ref().is_some_and(|b| a.eq_ignore_ascii_case(b));
        }
    }
    false
}
fn render(original: &Snapshot, format: Format, style: cleancopy_core::NumberingStyle) -> Snapshot {
    let source = original.source_text().unwrap_or_default();
    match format {
        Format::Markdown => {
            let mut result = Snapshot::text(source);
            result
                .formats
                .insert(TEXT_MARKDOWN.into(), source.as_bytes().to_vec());
            result
        }
        Format::ChineseNumbered => Snapshot::text(render_numbered(source, style)),
        Format::PlainStripped => Snapshot::text(render_plain(source)),
        Format::HtmlSource => Snapshot::text(render_html(source)),
        Format::WordRichText => {
            let mut result = Snapshot::text(render_plain(source));
            result
                .formats
                .insert(TEXT_HTML.into(), render_word_html(source).into_bytes());
            result
                .formats
                .insert(TEXT_RTF.into(), render_rtf(source).into_bytes());
            result
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct MockPlatform {
        clipboard: Snapshot,
        writes: Vec<Snapshot>,
        menus: Vec<MenuModel>,
        pastes: usize,
        hides: usize,
        fail_hide: bool,
        fail_write: bool,
    }

    impl Platform for MockPlatform {
        type Error = &'static str;

        fn frontmost_app(&mut self) -> Result<AppIdentity, Self::Error> {
            Ok(AppIdentity {
                exe: Some("WINWORD.EXE".into()),
                ..AppIdentity::default()
            })
        }
        fn read_clipboard(&mut self) -> Result<Snapshot, Self::Error> {
            Ok(self.clipboard.clone())
        }
        fn write_clipboard(&mut self, snapshot: Snapshot) -> Result<(), Self::Error> {
            if std::mem::take(&mut self.fail_write) {
                return Err("write");
            }
            self.writes.push(snapshot.clone());
            self.clipboard = snapshot;
            Ok(())
        }
        fn send_paste(&mut self) -> Result<(), Self::Error> {
            self.pastes += 1;
            Ok(())
        }
        fn send_undo(&mut self) -> Result<(), Self::Error> {
            Ok(())
        }
        fn show_menu(&mut self, model: MenuModel) -> Result<(), Self::Error> {
            self.menus.push(model);
            Ok(())
        }
        fn hide_menu(&mut self) -> Result<(), Self::Error> {
            self.hides += 1;
            if std::mem::take(&mut self.fail_hide) {
                return Err("hide");
            }
            Ok(())
        }
        fn notify(&mut self, _: &str) -> Result<(), Self::Error> {
            Ok(())
        }
        fn now(&self) -> u64 {
            0
        }
    }

    fn engine() -> Engine<MockPlatform> {
        let mut clipboard = Snapshot::text("# Original\r\n\r\n**bold**");
        clipboard
            .formats
            .insert("native/opaque".into(), vec![0, 255, 128, 13]);
        Engine::new(
            Config::default(),
            MockPlatform {
                clipboard,
                ..MockPlatform::default()
            },
        )
    }

    #[test]
    fn update_config_and_cancel_preserve_restoration_and_retry_deadlines() {
        for retry in [false, true] {
            let mut engine = engine();
            let original = engine.platform().clipboard.clone();
            engine.handle(Event::HotkeyPressed { t: 0 }).unwrap();
            let written = engine.platform().clipboard.clone();
            assert_ne!(written, original);
            let mut deadline = RESTORE_DELAY_MS;
            if retry {
                engine.platform_mut().fail_write = true;
                assert_eq!(engine.handle(Event::Timer { t: deadline }), Err("write"));
                deadline += RESTORE_DELAY_MS;
            }

            let mut config = engine.config().clone();
            config.silent_mode = false;
            config.paused_until = Some(10_000);
            engine.update_config(config).unwrap();
            engine.handle(Event::Cancel { t: deadline - 2 }).unwrap();
            assert!(!engine.config().silent_mode);
            assert_eq!(engine.config().paused_until, Some(10_000));
            assert_eq!(engine.next_deadline(), Some(deadline));
            assert_eq!(engine.platform().clipboard, written);
            assert_eq!(engine.platform().writes.len(), 1);

            engine
                .handle(Event::HotkeyPressed { t: deadline - 1 })
                .unwrap();
            engine.handle(Event::Timer { t: deadline - 1 }).unwrap();
            assert_eq!(engine.platform().clipboard, written);
            engine.handle(Event::Timer { t: deadline }).unwrap();
            assert_eq!(engine.platform().clipboard, original);
            assert_eq!(engine.platform().writes.len(), 2);
            assert_eq!(engine.platform().pastes, 1);
            assert!(engine.platform().menus.is_empty());
            assert_eq!(engine.next_deadline(), None);
        }
    }

    #[test]
    fn update_config_does_not_restore_over_newer_clipboard() {
        let mut engine = engine();
        engine.handle(Event::HotkeyPressed { t: 0 }).unwrap();
        engine.update_config(Config::default()).unwrap();
        let newer = Snapshot::text("new external copy");
        engine.platform_mut().clipboard = newer.clone();
        engine
            .handle(Event::Timer {
                t: RESTORE_DELAY_MS,
            })
            .unwrap();
        assert_eq!(engine.platform().clipboard, newer);
        assert_eq!(engine.platform().writes.len(), 1);
        assert_eq!(engine.next_deadline(), None);
    }

    #[test]
    fn update_config_cancels_full_menu_before_applying_settings() {
        let mut engine = engine();
        let original = engine.platform().clipboard.clone();
        engine
            .handle(Event::FullMenuHotkeyPressed { t: 0 })
            .unwrap();
        assert_eq!(engine.platform().menus.len(), 1);
        let id = engine.platform().menus[0].id;
        assert_eq!(engine.next_deadline(), None);
        let config = Config {
            silent_mode: false,
            ..Config::default()
        };
        engine.update_config(config).unwrap();
        assert_eq!(engine.platform().hides, 1);
        assert_eq!(engine.next_deadline(), None);
        engine
            .handle(Event::MenuKey {
                t: 1,
                id,
                key: MenuKey::Enter,
            })
            .unwrap();
        engine.handle(Event::Timer { t: 2 }).unwrap();
        assert_eq!(engine.platform().pastes, 0);
        assert!(engine.platform().writes.is_empty());
        assert_eq!(engine.platform().clipboard, original);

        engine.handle(Event::HotkeyPressed { t: 3 }).unwrap();
        assert_eq!(engine.platform().menus.len(), 2);
        assert_eq!(engine.platform().menus[1].kind, MenuKind::Full);
        assert_eq!(engine.next_deadline(), None);
    }

    #[test]
    fn update_config_invalidates_menu_even_when_hiding_fails() {
        for fail_hide in [false, true] {
            let mut engine = engine();
            let original = engine.platform().clipboard.clone();
            engine
                .handle(Event::FullMenuHotkeyPressed { t: 0 })
                .unwrap();
            let id = engine.platform().menus[0].id;
            engine.platform_mut().fail_hide = fail_hide;
            let config = Config {
                silent_mode: false,
                ..Config::default()
            };
            assert_eq!(
                engine.update_config(config),
                if fail_hide { Err("hide") } else { Ok(()) }
            );
            // A failed cancellation does not apply the replacement configuration,
            // but the old menu must still be unable to trigger work.
            assert_eq!(engine.config().silent_mode, fail_hide);
            for event in [
                Event::MenuChoice {
                    t: 2,
                    id,
                    choice: Choice::Format(Format::WordRichText),
                },
                Event::MenuKey {
                    t: 3,
                    id,
                    key: MenuKey::Enter,
                },
                Event::Remember {
                    t: 4,
                    id,
                    checked: true,
                },
                Event::MenuKey {
                    t: 5,
                    id,
                    key: MenuKey::Escape,
                },
                Event::Timer { t: 6 },
            ] {
                engine.handle(event).unwrap();
            }
            assert_eq!(engine.platform().hides, 1);
            assert_eq!(engine.platform().menus.len(), 1);
            assert_eq!(engine.platform().pastes, 0);
            assert!(engine.platform().writes.is_empty());
            assert_eq!(engine.platform().clipboard, original);
            assert_eq!(engine.next_deadline(), None);

            // Retrying a failed update does not try to dismiss the invalid menu again.
            engine
                .update_config(Config {
                    silent_mode: false,
                    ..Config::default()
                })
                .unwrap();
            engine.handle(Event::HotkeyPressed { t: 7 }).unwrap();
            assert_eq!(engine.platform().hides, 1);
            assert_eq!(engine.platform().menus.len(), 2);
            assert_ne!(engine.platform().menus[1].id, id);
            engine
                .handle(Event::MenuKey {
                    t: 8,
                    id,
                    key: MenuKey::Escape,
                })
                .unwrap();
            assert!(engine.menu.is_some());
        }
    }
}
