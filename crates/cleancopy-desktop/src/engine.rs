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
use crate::secure_input::{NativeSecureInput, SecureInput};
use cleancopy_core::{
    is_markdown, render_html, render_numbered, render_plain, render_rtf, render_word_html,
};
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
};

pub const RESTORE_DELAY_MS: u64 = 150;
pub const BACKGROUND_TEXT_BYTES: usize = 200 * 1024;
const CONVERSION_POLL_MS: u64 = 16;
pub const MAX_TEXT_BYTES: usize = 5 * 1024 * 1024;
/// Above this, Word/WPS rich text is not offered: its RTF/HTML output is many
/// times the source size (report v6: 5 MB → ~100 MB RTF, 907 MB peak RSS).
/// Measured at 2 MiB: 339 MB peak worker RSS (≤ 400 MB target).
pub const MAX_RICH_BYTES: usize = 2 * 1024 * 1024;
/// Menu note on the greyed Word/WPS item, and the silent-mode toast.
pub const RICH_TOO_LARGE_NOTE: &str = "超过 2MB";
pub const RICH_TOO_LARGE_TOAST: &str = "内容较大，已按纯文本粘贴";
pub const TEXT_PLAIN: &str = "text/plain";
pub const TEXT_MARKDOWN: &str = "text/markdown";
pub const TEXT_HTML: &str = "text/html";
pub const TEXT_RTF: &str = "text/rtf";

/// Exact native bytes, including opaque formats; equality compares all formats.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
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
        self.source_bytes()
            .filter(|bytes| bytes.len() <= MAX_TEXT_BYTES)
            .and_then(|bytes| std::str::from_utf8(bytes).ok())
    }
    fn source_bytes(&self) -> Option<&Vec<u8>> {
        self.formats
            .get(TEXT_MARKDOWN)
            .or_else(|| self.formats.get(TEXT_HTML))
            .or_else(|| self.formats.get(TEXT_PLAIN))
    }
    /// Whether the content is too large for Word/WPS rich text.
    pub fn too_large_for_rich(&self) -> bool {
        self.source_bytes()
            .is_some_and(|bytes| bytes.len() > MAX_RICH_BYTES)
    }
    fn html_source(&self) -> bool {
        !self.formats.contains_key(TEXT_MARKDOWN) && self.formats.contains_key(TEXT_HTML)
    }
    fn source_chars(&self) -> usize {
        let Some(text) = self
            .source_bytes()
            .and_then(|b| std::str::from_utf8(b).ok())
        else {
            return 0;
        };
        // HTML sources count what the reader sees, not tags (N-4).
        if self.html_source() {
            // A counting bug must never take the daemon down (report v4 C-1).
            std::panic::catch_unwind(|| crate::html_source::visible_chars(text))
                .unwrap_or_else(|_| text.chars().count())
        } else {
            text.chars().count()
        }
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
    /// A light, transient notice (e.g. a toast). Defaults to [`Platform::notify`].
    fn toast(&mut self, message: &str) -> Result<(), Self::Error> {
        self.notify(message)
    }
    /// Start reading the clipboard off the UI thread so a menu can be shown
    /// first. Returns false when unsupported; the engine then reads
    /// synchronously with [`Platform::read_clipboard`].
    fn begin_clipboard_read(&mut self) -> bool {
        false
    }
    /// The result of [`Platform::begin_clipboard_read`], once available.
    fn poll_clipboard_read(&mut self) -> Option<Result<Snapshot, Self::Error>> {
        None
    }
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
    /// Why a disabled item is unavailable (e.g. 「超过 2MB」).
    pub note: Option<&'static str>,
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
    pub source_chars: usize,
    pub converting: bool,
    pub original_enabled: bool,
    pub undo_enabled: bool,
    pub show_remember: bool,
    /// Whether the target captured at hotkey time has no App→format rule.
    /// Fixed for the menu's lifetime (the badge never re-detects the target).
    pub target_unrecognized: bool,
    /// Shown before the clipboard has been read; choices are disabled and the
    /// summary shows `content_label` without a character count.
    pub loading: bool,
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
/// A menu shown at hotkey time while the clipboard is read in the background.
struct Pending {
    app: AppIdentity,
    full: bool,
    model: MenuModel,
}
struct Conversion {
    capture: Capture,
    format: Format,
    remember: bool,
    /// `None` means the renderer panicked; the original is pasted instead.
    receiver: mpsc::Receiver<Option<Snapshot>>,
    cancelled: Arc<AtomicBool>,
    progress: MenuModel,
}
impl Drop for Conversion {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}
struct Restoration {
    original: Snapshot,
    /// Fingerprint of what we wrote: a converted Word/WPS snapshot can be
    /// ~100 MB, so keeping a second copy here doubled peak memory (report v6).
    written: u64,
    deadline: u64,
}
fn fingerprint(snapshot: &Snapshot) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    snapshot.hash(&mut hasher);
    hasher.finish()
}

struct Undo {
    app: AppIdentity,
    original: SharedSnapshot,
}

/// A snapshot kept for a long time (Undo) with identical payloads stored once:
/// text clipboards repeat one payload under several formats, which would
/// otherwise keep a large clipboard resident several times over.
struct SharedSnapshot {
    formats: Vec<(String, usize)>,
    payloads: Vec<Vec<u8>>,
}
impl SharedSnapshot {
    fn new(snapshot: Snapshot) -> Self {
        let mut payloads: Vec<Vec<u8>> = Vec::new();
        let formats = snapshot
            .formats
            .into_iter()
            .map(|(name, data)| {
                let index = payloads.iter().position(|p| *p == data).unwrap_or_else(|| {
                    payloads.push(data);
                    payloads.len() - 1
                });
                (name, index)
            })
            .collect();
        Self { formats, payloads }
    }
    fn to_snapshot(&self) -> Snapshot {
        Snapshot {
            formats: self
                .formats
                .iter()
                .map(|(name, index)| (name.clone(), self.payloads[*index].clone()))
                .collect(),
        }
    }
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
    conversion: Option<Conversion>,
    pending: Option<Pending>,
    secure_input: Box<dyn SecureInput>,
}
impl<P: Platform> Engine<P> {
    pub fn new(config: Config, platform: P) -> Self {
        Self::with_secure_input(config, platform, NativeSecureInput)
    }
    pub fn with_secure_input(
        config: Config,
        platform: P,
        secure_input: impl SecureInput + 'static,
    ) -> Self {
        Self {
            platform,
            config,
            now: 0,
            serial: 0,
            paused: false,
            menu: None,
            restoring: None,
            undo: None,
            conversion: None,
            pending: None,
            secure_input: Box::new(secure_input),
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
        self.restoring.as_ref().map(|r| r.deadline).or_else(|| {
            (self.conversion.is_some() || self.pending.is_some())
                .then(|| self.now.saturating_add(CONVERSION_POLL_MS))
        })
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
                if self.pending.as_ref().is_some_and(|p| p.model.id == id) {
                    return if key == MenuKey::Escape {
                        self.cancel()
                    } else {
                        Ok(())
                    };
                }
                if self
                    .conversion
                    .as_ref()
                    .is_some_and(|c| c.progress.id == id)
                {
                    return if key == MenuKey::Escape {
                        self.cancel()
                    } else {
                        Ok(())
                    };
                }
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
                self.poll_pending(t)?;
                self.poll_conversion(t)
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
                    || self.conversion.is_some()
                    || self.pending.is_some()
                    || self.menu.is_some()
                    || self.paused
                    || self.config.paused_until.is_some_and(|until| t < until)
                {
                    return Ok(());
                }
                if self.secure_blocked()? {
                    return Ok(());
                }
                let app = self.platform.frontmost_app()?;
                let full = matches!(event, Event::FullMenuHotkeyPressed { .. })
                    || !self.config.silent_mode;
                // Any route that may show a menu shows it first and reads the
                // clipboard in the background (large clipboards take long).
                if (full || self.config.format_for(&app).is_none())
                    && self.platform.begin_clipboard_read()
                {
                    return self.show_loading(app, full);
                }
                let original = self.platform.read_clipboard()?;
                self.dispatch(app, original, full, false, t)
            }
        }
    }
    fn show_loading(&mut self, app: AppIdentity, full: bool) -> Result<(), P::Error> {
        self.serial = self.serial.saturating_add(1);
        let unrecognized = self.config.format_for(&app).is_none();
        let model = MenuModel {
            id: self.serial,
            kind: if full {
                MenuKind::Full
            } else {
                MenuKind::Secondary
            },
            app: app.clone(),
            formats: self
                .config
                .secondary_items
                .iter()
                .filter(|item| item.enabled)
                .enumerate()
                .map(|(index, item)| FormatItem {
                    format: item.format,
                    shortcut: u8::try_from(index + 2).unwrap_or(u8::MAX),
                    enabled: false,
                    muted: false,
                    note: None,
                })
                .collect(),
            default_choice: Choice::Original,
            content_label: "正在读取剪贴板…".into(),
            source_chars: 0,
            converting: false,
            original_enabled: false,
            undo_enabled: false,
            // Keep the final layout so the window does not jump when filled.
            show_remember: !full && unrecognized,
            target_unrecognized: unrecognized,
            loading: true,
            remember: false,
        };
        self.platform.show_menu(model.clone())?;
        self.pending = Some(Pending { app, full, model });
        Ok(())
    }
    fn poll_pending(&mut self, t: u64) -> Result<(), P::Error> {
        if self.pending.is_none() {
            return Ok(());
        }
        let Some(result) = self.platform.poll_clipboard_read() else {
            return Ok(());
        };
        let pending = self.pending.take().unwrap();
        match result {
            Ok(original) => self.dispatch(pending.app, original, pending.full, true, t),
            Err(error) => {
                let _ = self.platform.hide_menu();
                let _ = self.platform.notify("Cannot read the clipboard");
                Err(error)
            }
        }
    }
    fn dispatch(
        &mut self,
        app: AppIdentity,
        original: Snapshot,
        full: bool,
        loading_shown: bool,
        t: u64,
    ) -> Result<(), P::Error> {
        {
            {
                let convertible = original.source_text().is_some();
                let markdown = convertible
                    && (original.html_source()
                        || original
                            .source_text()
                            .is_some_and(|s| is_markdown(detect_prefix(s))));
                let capture = Capture {
                    app,
                    original,
                    convertible,
                    markdown,
                };
                if full {
                    self.open_menu(capture, MenuKind::Full)
                } else if !capture.markdown
                    && capture
                        .original
                        .source_bytes()
                        .is_some_and(|b| std::str::from_utf8(b).is_ok())
                {
                    if loading_shown {
                        self.platform.hide_menu()?;
                    }
                    self.paste(capture, None, false, t)
                } else if let Some(format) = self.config.format_for(&capture.app) {
                    let mut format = capture.markdown.then_some(format);
                    if loading_shown {
                        self.platform.hide_menu()?;
                    }
                    // Too large for rich text: paste numbered plain text instead.
                    if format == Some(Format::WordRichText) && capture.original.too_large_for_rich()
                    {
                        format = Some(Format::ChineseNumbered);
                        self.platform.toast(RICH_TOO_LARGE_TOAST)?;
                    }
                    self.paste(capture, format, false, t)
                } else {
                    self.open_menu(capture, MenuKind::Secondary)
                }
            }
        }
    }
    fn cancel(&mut self) -> Result<(), P::Error> {
        let converting = self.conversion.take().is_some() | self.pending.take().is_some();
        if self.menu.take().is_some() || converting {
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
        let large = capture.original.too_large_for_rich();
        let available = |format: &Format| !(large && *format == Format::WordRichText);
        let default_choice = if capture.markdown && capture.convertible {
            self.config
                .format_for(&capture.app)
                .filter(|format| formats.contains(format) && available(format))
                .or_else(|| formats.iter().copied().find(available))
                .map(Choice::Format)
                .unwrap_or(Choice::Original)
        } else {
            Choice::Original
        };
        let content_label = if capture
            .original
            .source_bytes()
            .is_some_and(|bytes| bytes.len() > MAX_TEXT_BYTES)
        {
            "文本过大，只能粘贴原文"
        } else if capture.original.html_source() && capture.convertible {
            "HTML"
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
                    enabled: capture.convertible && available(&format),
                    muted: !capture.markdown,
                    note: (capture.convertible && !available(&format))
                        .then_some(RICH_TOO_LARGE_NOTE),
                })
                .collect(),
            default_choice,
            content_label,
            source_chars: capture.original.source_chars(),
            converting: false,
            original_enabled: true,
            undo_enabled: self
                .undo
                .as_ref()
                .is_some_and(|u| same_target(&u.app, &capture.app)),
            show_remember: self.config.format_for(&capture.app).is_none(),
            target_unrecognized: self.config.format_for(&capture.app).is_none(),
            loading: false,
            remember: false,
        };
        self.platform.show_menu(model.clone())?;
        self.menu = Some(Menu { capture, model });
        Ok(())
    }
    fn target(&mut self, app: &AppIdentity) -> Result<bool, P::Error> {
        if self.secure_blocked()? {
            return Ok(false);
        }
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
            if capture
                .original
                .source_bytes()
                .is_some_and(|s| s.len() > BACKGROUND_TEXT_BYTES)
            {
                return self.start_conversion(capture, format, remember);
            }
            return match render_guarded(&capture.original, format, self.config.numbering_style) {
                Some(written) => {
                    self.finish_paste(capture, Some(format), remember, Some(written), t)
                }
                // Converter panic: fall back to pasting the original text.
                None => self.finish_paste(capture, None, remember, None, t),
            };
        }
        self.finish_paste(capture, None, remember, None, t)
    }
    fn finish_paste(
        &mut self,
        capture: Capture,
        converted: Option<Format>,
        remember: bool,
        written: Option<Snapshot>,
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
        if let Some(written) = written {
            // Recheck immediately before mutating clipboard as well as before injection.
            if self.secure_blocked()? {
                return Ok(());
            }
            let fingerprint = fingerprint(&written);
            self.platform.write_clipboard(written)?;
            self.restoring = Some(Restoration {
                original: capture.original.clone(),
                written: fingerprint,
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
            original: SharedSnapshot::new(capture.original),
        });
        self.config.record_paste(&capture.app, converted, t);
        if remember {
            if let Some(format) = converted {
                self.config.remember(&capture.app, format);
            }
        }
        Ok(())
    }
    fn secure_blocked(&mut self) -> Result<bool, P::Error> {
        if !self.secure_input.enabled() {
            return Ok(false);
        }
        self.cancel()?;
        self.platform
            .notify("Paste blocked: Secure Input is enabled")?;
        Ok(true)
    }
    fn start_conversion(
        &mut self,
        capture: Capture,
        format: Format,
        remember: bool,
    ) -> Result<(), P::Error> {
        self.open_menu(capture, MenuKind::Full)?;
        let Menu { capture, mut model } = self.menu.take().unwrap();
        model.converting = true;
        model.original_enabled = false;
        model.undo_enabled = false;
        model.show_remember = false;
        for item in &mut model.formats {
            item.enabled = false;
        }
        if let Err(error) = self.platform.show_menu(model.clone()) {
            let _ = self.platform.hide_menu();
            return Err(error);
        }
        let source = capture.original.clone();
        let style = self.config.numbering_style;
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = cancelled.clone();
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            // Renderers poll the token at block boundaries and stop early;
            // partial output from a cancelled run is never published.
            if let Some(result) = cleancopy_core::cancel::run(&worker_cancelled, || {
                render_guarded(&source, format, style)
            }) {
                let _ = sender.send(result);
            }
        });
        self.conversion = Some(Conversion {
            capture,
            format,
            remember,
            receiver,
            cancelled,
            progress: model,
        });
        Ok(())
    }
    fn poll_conversion(&mut self, t: u64) -> Result<(), P::Error> {
        let Some(conversion) = self.conversion.as_ref() else {
            return Ok(());
        };
        let result = match conversion.receiver.try_recv() {
            Ok(result) => Some(result),
            Err(mpsc::TryRecvError::Empty) => return Ok(()),
            Err(mpsc::TryRecvError::Disconnected) => None,
        };
        let conversion = self.conversion.take().unwrap();
        self.platform.hide_menu()?;
        // Last check right before pasting: a cancel that raced the worker wins.
        if conversion.cancelled.load(Ordering::Relaxed) {
            return Ok(());
        }
        let Some(written) = result else {
            return self
                .platform
                .notify("Conversion failed; clipboard unchanged");
        };
        let capture = Capture {
            app: conversion.capture.app.clone(),
            original: conversion.capture.original.clone(),
            markdown: conversion.capture.markdown,
            convertible: conversion.capture.convertible,
        };
        let Some(written) = written else {
            // Converter panicked in the worker: paste the original instead.
            return self.finish_paste(capture, None, conversion.remember, None, t);
        };
        self.finish_paste(
            capture,
            Some(conversion.format),
            conversion.remember,
            Some(written),
            t,
        )
    }
    fn restore(&mut self, t: u64) -> Result<(), P::Error> {
        // Advance before fallible effects, avoiding a busy-loop on a past deadline.
        let restoration = self.restoring.as_mut().unwrap();
        restoration.deadline = t.saturating_add(RESTORE_DELAY_MS);
        if fingerprint(&self.platform.read_clipboard()?) != restoration.written {
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
        let original = last.original.to_snapshot();
        let written = fingerprint(&original);
        self.platform.write_clipboard(original)?;
        self.restoring = Some(Restoration {
            original: current,
            written,
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
/// Run a conversion, converting a panic into `None` so the caller can paste
/// the original text and the hotkey keeps working (report v4 C-1).
fn render_guarded(
    original: &Snapshot,
    format: Format,
    style: cleancopy_core::NumberingStyle,
) -> Option<Snapshot> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        #[cfg(test)]
        if original.formats.contains_key(TEST_PANIC_FORMAT) {
            panic!("test-induced converter panic");
        }
        render(original, format, style)
    }))
    .map_err(|_| eprintln!("CleanCopy: converter panicked; pasting original text"))
    .ok()
}
/// Test hook: a snapshot carrying this format makes `render_guarded` panic.
#[cfg(test)]
pub const TEST_PANIC_FORMAT: &str = "application/x-cleancopy-test-panic";
fn render(original: &Snapshot, format: Format, style: cleancopy_core::NumberingStyle) -> Snapshot {
    let raw = original.source_text().unwrap_or_default();
    let markdown;
    let source = if original.html_source() {
        markdown = crate::html_source::html_to_markdown(raw);
        &markdown
    } else {
        raw
    };
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

/// Markdown detection looks at a bounded prefix so huge clipboards do not
/// delay the menu (full detection of 3 MB took ~0.7 s in debug builds).
const DETECT_PREFIX_BYTES: usize = 256 * 1024;
fn detect_prefix(text: &str) -> &str {
    if text.len() <= DETECT_PREFIX_BYTES {
        return text;
    }
    let mut end = DETECT_PREFIX_BYTES;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    // Cut at a line boundary so a partial line cannot look like syntax.
    let cut = text[..end].rfind('\n').map_or(end, |i| i + 1);
    &text[..cut]
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
        notices: Vec<String>,
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
        fn notify(&mut self, message: &str) -> Result<(), Self::Error> {
            self.notices.push(message.into());
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
    fn converter_panic_pastes_original_and_hotkey_keeps_working() {
        let mut engine = engine();
        let original = engine.platform().clipboard.clone();
        let mut poisoned = original.clone();
        poisoned
            .formats
            .insert(TEST_PANIC_FORMAT.into(), Vec::new());
        engine.platform_mut().clipboard = poisoned.clone();
        engine.handle(Event::HotkeyPressed { t: 0 }).unwrap();
        // Original pasted untouched: no clipboard write, one paste.
        assert_eq!(engine.platform().pastes, 1);
        assert!(engine.platform().writes.is_empty());
        assert_eq!(engine.platform().clipboard, poisoned);
        // The next hotkey still converts normally.
        engine.platform_mut().clipboard = original.clone();
        engine.handle(Event::HotkeyPressed { t: 10 }).unwrap();
        assert_eq!(engine.platform().pastes, 2);
        assert_eq!(engine.platform().writes.len(), 1);
    }

    fn large_markdown() -> Snapshot {
        let block = "## 小节\n\n- **要点**：中文内容 `代码` 与 _强调_。\n\n";
        let text = format!(
            "# 大文档\n\n{}",
            block.repeat(MAX_RICH_BYTES / block.len() + 1)
        );
        assert!(text.len() > MAX_RICH_BYTES && text.len() < MAX_TEXT_BYTES);
        Snapshot::text(text)
    }

    #[test]
    fn silent_word_target_over_2mb_pastes_numbered_plain_text_with_toast() {
        let mut engine = engine();
        let app = engine.platform_mut().frontmost_app().unwrap();
        assert_eq!(engine.config().format_for(&app), Some(Format::WordRichText));
        engine.platform_mut().clipboard = large_markdown();
        engine.handle(Event::HotkeyPressed { t: 0 }).unwrap();
        for _ in 0..4000 {
            if engine.platform().pastes > 0 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
            engine.tick().unwrap();
        }
        assert_eq!(engine.platform().pastes, 1);
        let written = &engine.platform().writes[0];
        assert!(!written.formats.contains_key(TEXT_RTF));
        assert!(!written.formats.contains_key(TEXT_HTML));
        let plain = std::str::from_utf8(&written.formats[TEXT_PLAIN]).unwrap();
        assert!(plain.starts_with("大文档"), "{}", &plain[..40]);
        assert!(plain.contains("一、小节"));
        assert_eq!(
            engine.platform().notices,
            vec![RICH_TOO_LARGE_TOAST.to_string()]
        );
    }

    #[test]
    fn menu_greys_word_rich_text_over_2mb_with_note() {
        let mut engine = engine();
        engine.platform_mut().clipboard = large_markdown();
        engine
            .handle(Event::FullMenuHotkeyPressed { t: 0 })
            .unwrap();
        let menu = engine.platform().menus.last().unwrap().clone();
        let word = menu
            .formats
            .iter()
            .find(|f| f.format == Format::WordRichText)
            .unwrap();
        assert!(!word.enabled);
        assert_eq!(word.note, Some(RICH_TOO_LARGE_NOTE));
        assert_ne!(menu.default_choice, Choice::Format(Format::WordRichText));
        assert!(menu
            .formats
            .iter()
            .filter(|f| f.format != Format::WordRichText)
            .all(|f| f.enabled && f.note.is_none()));
        // Small content keeps Word/WPS available.
        engine.handle(Event::Cancel { t: 1 }).unwrap();
        engine.platform_mut().clipboard = Snapshot::text("# a\n\n- b");
        engine
            .handle(Event::FullMenuHotkeyPressed { t: 2 })
            .unwrap();
        let menu = engine.platform().menus.last().unwrap();
        assert!(menu.formats.iter().all(|f| f.enabled && f.note.is_none()));
    }

    #[test]
    fn undo_item_is_greyed_after_undoing() {
        let mut engine = engine();
        engine.handle(Event::HotkeyPressed { t: 0 }).unwrap();
        engine
            .handle(Event::Timer {
                t: RESTORE_DELAY_MS + 1,
            })
            .unwrap();
        engine
            .handle(Event::FullMenuHotkeyPressed {
                t: RESTORE_DELAY_MS + 2,
            })
            .unwrap();
        assert!(engine.platform().menus.last().unwrap().undo_enabled);
        engine
            .handle(Event::MenuChoice {
                t: RESTORE_DELAY_MS + 3,
                id: engine.platform().menus.last().unwrap().id,
                choice: Choice::Undo,
            })
            .unwrap();
        assert_eq!(engine.platform().pastes, 2);
        engine
            .handle(Event::Timer {
                t: 2 * RESTORE_DELAY_MS + 10,
            })
            .unwrap();
        engine
            .handle(Event::FullMenuHotkeyPressed {
                t: 2 * RESTORE_DELAY_MS + 11,
            })
            .unwrap();
        assert!(!engine.platform().menus.last().unwrap().undo_enabled);
    }

    #[test]
    fn background_converter_panic_pastes_original() {
        let mut engine = engine();
        let mut big = Snapshot::text(format!(
            "# Big\n\n{}",
            "研发&测试 ".repeat(BACKGROUND_TEXT_BYTES / 8)
        ));
        big.formats.insert(TEST_PANIC_FORMAT.into(), Vec::new());
        engine.platform_mut().clipboard = big.clone();
        engine.handle(Event::HotkeyPressed { t: 0 }).unwrap();
        for _ in 0..500 {
            if engine.platform().pastes > 0 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
            engine.tick().unwrap();
        }
        assert_eq!(engine.platform().pastes, 1);
        assert!(engine.platform().writes.is_empty());
        assert_eq!(engine.platform().clipboard, big);
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
