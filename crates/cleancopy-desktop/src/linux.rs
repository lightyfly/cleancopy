//! X11 platform, event driver, and background desktop menu host.
use crate::{
    config::{AppIdentity, Config},
    engine::{Engine, Event, MenuKey, MenuModel, Platform, Snapshot, TEXT_PLAIN},
};
use cleancopy_clipboard::{NativeClipboard, NativeSnapshot, NativeTarget};
use std::{
    collections::BTreeSet,
    fmt,
    path::PathBuf,
    time::{Instant, SystemTime, UNIX_EPOCH},
};
use x11rb::{
    connection::Connection,
    protocol::{
        xproto::{self, AtomEnum, ConnectionExt, GrabMode, ModMask, Window},
        xtest::ConnectionExt as _,
        Event as XEvent,
    },
    rust_connection::RustConnection,
    CURRENT_TIME,
};

#[derive(Debug)]
pub struct LinuxError(pub String);
impl fmt::Display for LinuxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for LinuxError {}
fn err(e: impl fmt::Display) -> LinuxError {
    LinuxError(e.to_string())
}
type Result<T> = std::result::Result<T, LinuxError>;

/// Owns passive grabs, native clipboard ownership, and a monotonic Unix clock.
/// Call `poll_events` regularly, or use `LinuxDriver` to also service engine timers.
pub struct LinuxPlatform {
    conn: RustConnection,
    root: Window,
    clipboard: NativeClipboard,
    keys: Vec<(u8, Vec<u32>)>,
    hotkeys: Vec<(u8, u16, bool)>,
    locks: Vec<u16>,
    menu_keys: Vec<u8>,
    menu: Option<MenuModel>,
    verified_window: Option<Window>,
    last_write: Option<(NativeSnapshot, Snapshot)>,
    pending_read: Option<std::sync::mpsc::Receiver<std::result::Result<NativeSnapshot, String>>>,
    epoch: u64,
    started: Instant,
    pub notice: Option<String>,
}
impl LinuxPlatform {
    /// Connect to DISPLAY and reserve both default shortcuts.
    pub fn new() -> Result<Self> {
        Self::with_config(&Config::default())
    }
    pub fn with_config(config: &Config) -> Result<Self> {
        let (conn, screen) = x11rb::connect(None).map_err(err)?;
        let root = conn.setup().roots[screen].root;
        conn.xtest_get_version(2, 2)
            .map_err(err)?
            .reply()
            .map_err(err)?;
        let min = conn.setup().min_keycode;
        let mapping = conn
            .get_keyboard_mapping(min, conn.setup().max_keycode - min + 1)
            .map_err(err)?
            .reply()
            .map_err(err)?;
        let keys: Vec<_> = mapping
            .keysyms
            .chunks(usize::from(mapping.keysyms_per_keycode))
            .enumerate()
            .map(|(i, symbols)| (min + i as u8, symbols.to_vec()))
            .collect();
        let modifiers = conn
            .get_modifier_mapping()
            .map_err(err)?
            .reply()
            .map_err(err)?;
        let per = modifiers.keycodes.len() / 8;
        let mut lock_masks = Vec::new();
        if per != 0 {
            for (index, codes) in modifiers.keycodes.chunks(per).enumerate() {
                if codes.iter().any(|code| {
                    keys.iter().any(|(k, syms)| {
                        k == code
                            && syms
                                .iter()
                                .any(|s| matches!(*s, 0xffe5 | 0xffe6 | 0xff7f | 0xff14))
                    })
                }) {
                    lock_masks.push(1 << index);
                }
            }
        }
        let locks = lock_variants(&lock_masks);
        let hotkeys = configured_hotkeys(config, &keys)?;
        let clipboard = NativeClipboard::new().map_err(err)?;
        let result = Self {
            conn,
            root,
            clipboard,
            keys,
            hotkeys,
            locks,
            menu_keys: Vec::new(),
            menu: None,
            verified_window: None,
            last_write: None,
            pending_read: None,
            epoch: unix_ms(),
            started: Instant::now(),
            notice: None,
        };
        result.grab_hotkeys(&result.hotkeys)?;
        result.conn.flush().map_err(err)?;
        Ok(result)
    }
    fn grab_hotkeys(&self, hotkeys: &[(u8, u16, bool)]) -> Result<()> {
        for &(key, modifiers, full) in hotkeys {
            for &locks in &self.locks {
                self.conn
                    .grab_key(
                        false,
                        self.root,
                        ModMask::from(modifiers | locks),
                        key,
                        GrabMode::ASYNC,
                        GrabMode::ASYNC,
                    )
                    .map_err(err)?
                    .check()
                    .map_err(|e| {
                        err(format!(
                            "cannot register {} hotkey (global hotkey conflict/BadAccess): {e}",
                            if full { "full-menu" } else { "paste" }
                        ))
                    })?;
            }
        }
        Ok(())
    }
    /// Register new shortcuts before releasing old ones, retaining working grabs on failure.
    pub fn update_hotkeys(&mut self, config: &Config) -> Result<()> {
        let new = configured_hotkeys(config, &self.keys)?;
        let added: Vec<_> = new
            .iter()
            .copied()
            .filter(|(key, mask, _)| !self.hotkeys.iter().any(|(k, m, _)| k == key && m == mask))
            .collect();
        if let Err(error) = self.grab_hotkeys(&added) {
            self.ungrab_hotkeys(&added)?;
            return Err(error);
        }
        let removed: Vec<_> = self
            .hotkeys
            .iter()
            .copied()
            .filter(|(key, mask, _)| !new.iter().any(|(k, m, _)| k == key && m == mask))
            .collect();
        self.ungrab_hotkeys(&removed)?;
        self.hotkeys = new;
        self.conn.flush().map_err(err)
    }
    fn ungrab_hotkeys(&self, hotkeys: &[(u8, u16, bool)]) -> Result<()> {
        for &(key, modifiers, _) in hotkeys {
            for &locks in &self.locks {
                self.conn
                    .ungrab_key(key, self.root, ModMask::from(modifiers | locks))
                    .map_err(err)?
                    .check()
                    .map_err(err)?;
            }
        }
        Ok(())
    }
    pub fn menu(&self) -> Option<&MenuModel> {
        self.menu.as_ref()
    }
    fn atom(&self, name: &[u8]) -> Result<u32> {
        Ok(self
            .conn
            .intern_atom(false, name)
            .map_err(err)?
            .reply()
            .map_err(err)?
            .atom)
    }
    fn is_transient_popup(&self, window: Window) -> Result<bool> {
        let type_atom = self.atom(b"_NET_WM_WINDOW_TYPE")?;
        let types: Vec<u32> = self
            .conn
            .get_property(false, window, type_atom, AtomEnum::ATOM, 0, 16)
            .map_err(err)?
            .reply()
            .map_err(err)?
            .value32()
            .map(|v| v.collect())
            .unwrap_or_default();
        for name in [
            &b"_NET_WM_WINDOW_TYPE_NOTIFICATION"[..],
            b"_NET_WM_WINDOW_TYPE_POPUP_MENU",
            b"_NET_WM_WINDOW_TYPE_DROPDOWN_MENU",
            b"_NET_WM_WINDOW_TYPE_TOOLTIP",
            b"_NET_WM_WINDOW_TYPE_COMBO",
            b"_NET_WM_WINDOW_TYPE_DND",
        ] {
            if types.contains(&self.atom(name)?) {
                return Ok(true);
            }
        }
        Ok(false)
    }
    fn active_window(&self) -> Result<Window> {
        let atom = self.atom(b"_NET_ACTIVE_WINDOW")?;
        let property = self
            .conn
            .get_property(false, self.root, atom, AtomEnum::WINDOW, 0, 1)
            .map_err(err)?
            .reply()
            .map_err(err)?;
        let window = property.value32().and_then(|mut v| v.next()).unwrap_or(0);
        if window != 0 {
            return Ok(window);
        }
        let focus = self
            .conn
            .get_input_focus()
            .map_err(err)?
            .reply()
            .map_err(err)?
            .focus;
        if focus > 1 && focus != self.root {
            Ok(focus)
        } else {
            Err(err("no foreground X11 application"))
        }
    }
    fn key(&self, symbol: u32) -> Result<u8> {
        self.keys
            .iter()
            .find(|(_, s)| s.contains(&symbol))
            .map(|(k, _)| *k)
            .ok_or_else(|| err(format!("keysym {symbol:#x} is unavailable")))
    }
    /// Drain native hotkeys/menu accelerators without blocking. Dispatch these before timers.
    pub fn poll_events(&mut self) -> Result<Vec<Event>> {
        let mut events = Vec::new();
        while let Some(event) = self.conn.poll_for_event().map_err(err)? {
            if let XEvent::KeyPress(key) = event {
                let locks = self.locks.iter().fold(0, |mask, lock| mask | lock);
                let modifiers = u16::from(key.state) & 0xff & !locks;
                if let Some(&(_, _, full)) = self
                    .hotkeys
                    .iter()
                    .find(|&&(code, mask, _)| code == key.detail && modifiers == mask)
                {
                    events.push(if full {
                        Event::FullMenuHotkeyPressed { t: self.now() }
                    } else {
                        Event::HotkeyPressed { t: self.now() }
                    });
                } else if let Some(menu) = &self.menu {
                    let symbols = self
                        .keys
                        .iter()
                        .find(|(k, _)| *k == key.detail)
                        .map(|(_, s)| s.as_slice())
                        .unwrap_or_default();
                    let action = symbols.iter().find_map(|s| match *s {
                        0x31..=0x39 => Some(MenuKey::Digit((*s - 0x30) as u8)),
                        0xff0d => Some(MenuKey::Enter),
                        0xff1b => Some(MenuKey::Escape),
                        _ => None,
                    });
                    if let Some(key) = action {
                        events.push(Event::MenuKey {
                            t: self.now(),
                            id: menu.id,
                            key,
                        });
                    }
                }
            }
        }
        Ok(events)
    }
    /// Pointer placement in physical X11 pixels, clamped to the root screen.
    pub fn menu_position(&self, width: i32, height: i32) -> (i32, i32) {
        let geometry = self
            .conn
            .get_geometry(self.root)
            .ok()
            .and_then(|c| c.reply().ok());
        let (w, h) = geometry
            .map(|g| (i32::from(g.width), i32::from(g.height)))
            .unwrap_or((1024, 768));
        let pointer = self
            .conn
            .query_pointer(self.root)
            .ok()
            .and_then(|c| c.reply().ok())
            .filter(|p| p.same_screen)
            .map(|p| (i32::from(p.root_x) + 8, i32::from(p.root_y) + 8))
            .unwrap_or(((w - width) / 2, (h - height) / 2));
        clamp_position(pointer, (width, height), (w, h))
    }
    fn inject(&mut self, symbol: u32) -> Result<()> {
        let target = self
            .verified_window
            .ok_or_else(|| err("input injection requires a verified foreground application"))?;
        if self.active_window()? != target {
            return Err(err("foreground window changed before input injection"));
        }
        // End the passive hotkey's active keyboard grab before generating target input.
        self.conn
            .ungrab_keyboard(CURRENT_TIME)
            .map_err(err)?
            .check()
            .map_err(err)?;
        let map = self
            .conn
            .query_keymap()
            .map_err(err)?
            .reply()
            .map_err(err)?
            .keys;
        let held = |key: u8| map[usize::from(key / 8)] & (1 << (key % 8)) != 0;
        let modifiers: Vec<_> = self
            .keys
            .iter()
            .filter(|(k, s)| {
                held(*k)
                    && s.iter()
                        .any(|symbol| matches!(*symbol, 0xffe1..=0xffe4 | 0xffe7..=0xffee))
            })
            .map(|(k, _)| *k)
            .collect();
        let control = self.key(0xffe3)?;
        let key = self.key(symbol)?;
        let fake = |kind, code| -> Result<()> {
            self.conn
                .xtest_fake_input(kind, code, CURRENT_TIME, self.root, 0, 0, 0)
                .map_err(err)?
                .check()
                .map_err(err)
        };
        // Never restore synthetic modifier releases: a physical release may already
        // be queued, and restoring it could leave a stuck modifier on the server.
        for modifier in modifiers {
            fake(xproto::KEY_RELEASE_EVENT, modifier)?;
        }
        if held(key) {
            fake(xproto::KEY_RELEASE_EVENT, key)?;
        }
        fake(xproto::KEY_PRESS_EVENT, control)?;
        fake(xproto::KEY_PRESS_EVENT, key)?;
        fake(xproto::KEY_RELEASE_EVENT, key)?;
        fake(xproto::KEY_RELEASE_EVENT, control)?;
        self.conn.flush().map_err(err)
    }
    fn ungrab_menu(&mut self) -> Result<()> {
        for key in self.menu_keys.drain(..) {
            for &mask in &self.locks {
                self.conn
                    .ungrab_key(key, self.root, ModMask::from(mask))
                    .map_err(err)?
                    .check()
                    .map_err(err)?;
            }
        }
        Ok(())
    }
}
impl Platform for LinuxPlatform {
    type Error = LinuxError;
    fn frontmost_app(&mut self) -> Result<AppIdentity> {
        let window = self.active_window()?;
        let property = self
            .conn
            .get_property(false, window, AtomEnum::WM_CLASS, AtomEnum::STRING, 0, 4096)
            .map_err(err)?
            .reply()
            .map_err(err)?;
        let wm_class = parse_wm_class(&property.value);
        let pid_atom = self.atom(b"_NET_WM_PID")?;
        let pid = self
            .conn
            .get_property(false, window, pid_atom, AtomEnum::CARDINAL, 0, 1)
            .map_err(err)?
            .reply()
            .map_err(err)?
            .value32()
            .and_then(|mut v| v.next());
        // CleanCopy's own windows and transient system popups are never
        // paste targets; the target is locked at hotkey time instead.
        if pid == Some(std::process::id()) {
            return Err(err(
                "foreground window belongs to CleanCopy; not a paste target",
            ));
        }
        if self.is_transient_popup(window)? {
            return Err(err(
                "foreground window is a system popup; not a paste target",
            ));
        }
        let exe = pid
            .and_then(|pid| std::fs::read_link(format!("/proc/{pid}/exe")).ok())
            .and_then(|path| {
                path.file_name()
                    .map(|name| name.to_string_lossy().into_owned())
            });
        self.verified_window = Some(window);
        Ok(AppIdentity {
            name: wm_class
                .clone()
                .or_else(|| exe.clone())
                .unwrap_or_else(|| "Unknown application".into()),
            wm_class,
            exe,
            bundle_id: None,
        })
    }
    fn read_clipboard(&mut self) -> Result<Snapshot> {
        let native = self.clipboard.read_snapshot().map_err(err)?;
        if let Some((last, snapshot)) = &self.last_write {
            if *last == native {
                return Ok(snapshot.clone());
            }
        }
        Ok(encode_snapshot(&native))
    }
    fn begin_clipboard_read(&mut self) -> bool {
        // Each read uses its own X11 connection, so it can run on a worker.
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let result = NativeClipboard::new()
                .and_then(|clipboard| clipboard.read_snapshot())
                .map_err(|e| e.to_string());
            let _ = sender.send(result);
        });
        self.pending_read = Some(receiver);
        true
    }
    fn poll_clipboard_read(&mut self) -> Option<Result<Snapshot>> {
        let result = match self.pending_read.as_ref()?.try_recv() {
            Ok(result) => result,
            Err(std::sync::mpsc::TryRecvError::Empty) => return None,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                Err("clipboard reader stopped".to_owned())
            }
        };
        self.pending_read = None;
        Some(result.map_err(err).map(|native| match &self.last_write {
            Some((last, snapshot)) if *last == native => snapshot.clone(),
            _ => encode_snapshot(&native),
        }))
    }
    fn write_clipboard(&mut self, snapshot: Snapshot) -> Result<()> {
        let native = decode_snapshot(&snapshot)?;
        self.clipboard.write_snapshot(&native).map_err(err)?;
        self.last_write = Some((native, snapshot));
        Ok(())
    }
    fn send_paste(&mut self) -> Result<()> {
        self.inject(0x76)
    }
    fn send_undo(&mut self) -> Result<()> {
        self.inject(0x7a)
    }
    fn show_menu(&mut self, model: MenuModel) -> Result<()> {
        if !model.loading && self.menu.as_ref().is_some_and(|m| m.loading) {
            eprintln!("CleanCopy menu metadata ready");
        }
        if self.menu.is_none() {
            for symbol in (0x31..=0x39).chain([0xff0d, 0xff1b]) {
                let key = self.key(symbol)?;
                self.menu_keys.push(key);
                for &mask in &self.locks {
                    let grab = self
                        .conn
                        .grab_key(
                            false,
                            self.root,
                            ModMask::from(mask),
                            key,
                            GrabMode::ASYNC,
                            GrabMode::ASYNC,
                        )
                        .map_err(err)?
                        .check();
                    if let Err(e) = grab {
                        self.ungrab_menu()?;
                        return Err(err(format!("menu accelerator grab failed: {e}")));
                    }
                }
            }
        }
        self.menu = Some(model);
        self.conn.flush().map_err(err)
    }
    fn hide_menu(&mut self) -> Result<()> {
        self.menu = None;
        self.ungrab_menu()?;
        self.conn.flush().map_err(err)
    }
    fn notify(&mut self, message: &str) -> Result<()> {
        eprintln!("CleanCopy: {message}");
        self.notice = Some(message.into());
        Ok(())
    }
    fn now(&self) -> u64 {
        self.epoch
            .saturating_add(self.started.elapsed().as_millis() as u64)
    }
}

/// Caller-owned engine. `poll` never sleeps; drive at ~5–10ms or integrate with a GUI.
pub struct LinuxDriver {
    engine: Engine<LinuxPlatform>,
}
impl LinuxDriver {
    pub fn new(config: Config) -> Result<Self> {
        let platform = LinuxPlatform::with_config(&config)?;
        Ok(Self {
            engine: Engine::new(config, platform),
        })
    }
    pub fn engine(&self) -> &Engine<LinuxPlatform> {
        &self.engine
    }
    pub fn engine_mut(&mut self) -> &mut Engine<LinuxPlatform> {
        &mut self.engine
    }
    pub fn dispatch(&mut self, event: Event) -> Result<()> {
        self.engine.handle(event)
    }
    pub fn poll(&mut self) -> Result<()> {
        for event in self.engine.platform_mut().poll_events()? {
            self.engine.handle(event)?;
        }
        if self
            .engine
            .next_deadline()
            .is_some_and(|deadline| deadline <= self.engine.platform().now())
        {
            self.engine.tick()?;
        }
        Ok(())
    }
}
fn configured_hotkeys(config: &Config, keys: &[(u8, Vec<u32>)]) -> Result<Vec<(u8, u16, bool)>> {
    let mut result = Vec::new();
    for (text, full) in [(&config.hotkey, false), (&config.full_menu_hotkey, true)] {
        let (symbol, modifiers) = parse_hotkey(text)?;
        for (code, _) in keys.iter().filter(|(_, symbols)| symbols.contains(&symbol)) {
            if result
                .iter()
                .any(|&(key, mask, _)| key == *code && mask == modifiers)
            {
                return Err(err("paste and full-menu hotkeys must be different"));
            }
            result.push((*code, modifiers, full));
        }
        if !result.iter().any(|&(_, _, is_full)| is_full == full) {
            return Err(err(format!("keyboard map cannot register {text}")));
        }
    }
    Ok(result)
}
fn parse_hotkey(text: &str) -> Result<(u32, u16)> {
    let mut parts: Vec<_> = text.split('+').map(str::trim).collect();
    let key = parts.pop().unwrap_or_default();
    let mut modifiers = 0;
    for part in parts {
        modifiers |= u16::from(match part.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => ModMask::CONTROL,
            "shift" => ModMask::SHIFT,
            "alt" | "option" => ModMask::M1,
            "super" | "cmd" | "command" => ModMask::M4,
            _ => return Err(err(format!("unsupported hotkey modifier: {part}"))),
        });
    }
    if modifiers & u16::from(ModMask::CONTROL | ModMask::M1 | ModMask::M4) == 0 {
        return Err(err("hotkey requires Ctrl, Alt or Super"));
    }
    let lower = key.to_ascii_lowercase();
    let symbol = match lower.as_str() {
        "escape" => 0xff1b,
        "enter" => 0xff0d,
        "space" => 0x20,
        "tab" => 0xff09,
        "backspace" => 0xff08,
        "delete" => 0xffff,
        "insert" => 0xff63,
        "home" => 0xff50,
        "end" => 0xff57,
        "pageup" => 0xff55,
        "pagedown" => 0xff56,
        "arrowleft" | "left" => 0xff51,
        "arrowup" | "up" => 0xff52,
        "arrowright" | "right" => 0xff53,
        "arrowdown" | "down" => 0xff54,
        _ if lower.len() == 1 && lower.as_bytes()[0].is_ascii_alphanumeric() => {
            u32::from(lower.as_bytes()[0])
        }
        _ => lower
            .strip_prefix('f')
            .and_then(|n| n.parse::<u32>().ok())
            .filter(|n| (1..=12).contains(n))
            .map(|n| 0xffbd + n)
            .ok_or_else(|| err(format!("unsupported hotkey key: {key}")))?,
    };
    Ok((symbol, modifiers))
}
fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
fn lock_variants(masks: &[u16]) -> Vec<u16> {
    let mut variants = BTreeSet::from([0]);
    for mask in masks {
        let extra: Vec<_> = variants.iter().map(|v| v | mask).collect();
        variants.extend(extra);
    }
    variants.into_iter().collect()
}
fn parse_wm_class(bytes: &[u8]) -> Option<String> {
    let mut parts = bytes.split(|b| *b == 0);
    let instance = parts.next().unwrap_or_default();
    let class = parts.next().filter(|s| !s.is_empty()).unwrap_or(instance);
    (!class.is_empty()).then(|| String::from_utf8_lossy(class).into_owned())
}
fn clamp_position((x, y): (i32, i32), (w, h): (i32, i32), (sw, sh): (i32, i32)) -> (i32, i32) {
    (x.clamp(0, (sw - w).max(0)), y.clamp(0, (sh - h).max(0)))
}
const NATIVE: &str = "cleancopy:x11:";
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn unhex(value: &str) -> Result<Vec<u8>> {
    if !value.len().is_multiple_of(2) {
        return Err(err("invalid snapshot atom"));
    }
    (0..value.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&value[i..i + 2], 16).map_err(err))
        .collect()
}
fn encode_snapshot(native: &NativeSnapshot) -> Snapshot {
    let mut result = Snapshot::default();
    for target in &native.targets {
        result.formats.insert(
            format!(
                "{NATIVE}{}:{}:{}",
                target.format,
                hex(&target.target),
                hex(&target.type_name)
            ),
            target.data.clone(),
        );
    }
    for alias in ["text/markdown", "text/plain", "text/html", "text/rtf"] {
        let target = native
            .targets
            .iter()
            .find(|t| t.target == alias.as_bytes())
            .or_else(|| {
                (alias == TEXT_PLAIN)
                    .then(|| {
                        native.targets.iter().find(|t| {
                            matches!(
                                t.target.as_slice(),
                                b"UTF8_STRING" | b"text/plain;charset=utf-8"
                            )
                        })
                    })
                    .flatten()
            });
        if let Some(target) = target.filter(|t| t.format == 8) {
            result.formats.insert(alias.into(), target.data.clone());
        }
    }
    result
}
fn decode_snapshot(snapshot: &Snapshot) -> Result<NativeSnapshot> {
    let mut targets = Vec::new();
    for (name, data) in &snapshot.formats {
        if let Some(rest) = name.strip_prefix(NATIVE) {
            let fields: Vec<_> = rest.split(':').collect();
            if fields.len() != 3 {
                return Err(err("invalid native snapshot metadata"));
            }
            targets.push(NativeTarget {
                format: fields[0].parse().map_err(err)?,
                target: unhex(fields[1])?,
                type_name: unhex(fields[2])?,
                data: data.clone(),
            });
        }
    }
    if targets.is_empty() {
        for (name, data) in &snapshot.formats {
            targets.push(NativeTarget {
                target: name.as_bytes().to_vec(),
                type_name: name.as_bytes().to_vec(),
                format: 8,
                data: data.clone(),
            });
        }
        if let Some(data) = snapshot.formats.get(TEXT_PLAIN) {
            targets.push(NativeTarget {
                target: b"UTF8_STRING".to_vec(),
                type_name: b"UTF8_STRING".to_vec(),
                format: 8,
                data: data.clone(),
            });
        }
    }
    targets.sort_by(|a, b| a.target.cmp(&b.target));
    Ok(NativeSnapshot { targets })
}

/// D-Bus callbacks only enqueue actions; X11 and engine work stay on the UI thread.
struct NativeTray {
    sender: std::sync::mpsc::Sender<crate::ui::menus::TrayAction>,
    config: Config,
    enabled: bool,
}
impl NativeTray {
    fn send(&self, action: crate::ui::menus::TrayAction) {
        let _ = self.sender.send(action);
    }
}
impl ksni::Tray for NativeTray {
    const MENU_ON_ACTIVATE: bool = true;
    fn id(&self) -> String {
        "cleancopy".into()
    }
    fn title(&self) -> String {
        "CleanCopy".into()
    }
    fn icon_name(&self) -> String {
        "edit-paste".into()
    }
    fn watcher_offline(&self, reason: ksni::OfflineReason) -> bool {
        eprintln!("CleanCopy tray: no StatusNotifier host ({reason:?}); daemon continues");
        true
    }
    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        use crate::ui::menus::TrayAction;
        use ksni::menu::{CheckmarkItem, StandardItem};
        let mut items = vec![
            CheckmarkItem {
                label: if self.enabled {
                    "CleanCopy 运行中"
                } else {
                    "CleanCopy 已暂停"
                }
                .into(),
                checked: self.enabled,
                activate: Box::new(|tray: &mut Self| {
                    tray.send(TrayAction::SetEnabled(!tray.enabled))
                }),
                ..Default::default()
            }
            .into(),
            ksni::MenuItem::Separator,
        ];
        items.push(
            StandardItem {
                label: "最近粘贴".into(),
                enabled: false,
                ..Default::default()
            }
            .into(),
        );
        if self.config.recent_pastes.is_empty() {
            items.push(
                StandardItem {
                    label: "暂无粘贴记录".into(),
                    enabled: false,
                    ..Default::default()
                }
                .into(),
            );
        }
        for recent in self.config.recent_pastes.iter().take(2) {
            let label = recent
                .format
                .map(|format| crate::ui::menus::format_label(format).1)
                .unwrap_or("原文");
            items.push(
                StandardItem {
                    label: format!("{} → {label}", recent.app.name),
                    enabled: false,
                    ..Default::default()
                }
                .into(),
            );
        }
        items.push(ksni::MenuItem::Separator);
        for (label, action) in [
            (
                format!("唤出粘贴菜单 ({})", self.config.full_menu_hotkey),
                TrayAction::ShowMenu,
            ),
            ("撤销上次转换".into(), TrayAction::Undo),
            ("暂停 1 小时".into(), TrayAction::Pause),
            ("设置…".into(), TrayAction::Settings),
            ("使用引导…".into(), TrayAction::Onboarding),
            ("退出 CleanCopy".into(), TrayAction::Quit),
        ] {
            items.push(
                StandardItem {
                    label,
                    activate: Box::new(move |tray: &mut Self| tray.send(action)),
                    ..Default::default()
                }
                .into(),
            );
        }
        items
    }
}

/// Run the background helper and nonactivating native menu on the main thread.
pub fn run(config: Config, save_path: Option<PathBuf>) -> Result<()> {
    use eframe::egui;
    use ksni::blocking::TrayMethods;
    let enabled = config.paused_until.is_none_or(|until| until <= unix_ms());
    let (sender, actions) = std::sync::mpsc::channel();
    let tray = match (NativeTray {
        sender,
        config: config.clone(),
        enabled,
    })
    .assume_sni_available(true)
    .spawn()
    {
        Ok(handle) => Some(handle),
        Err(error) => {
            eprintln!("CleanCopy tray unavailable: {error}; daemon continues");
            None
        }
    };
    let saved = config.clone();
    let driver = LinuxDriver::new(config)?;
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("CleanCopy floating menu")
            .with_inner_size([640.0, 360.0])
            .with_decorations(false)
            .with_resizable(false)
            .with_active(false)
            .with_visible(false)
            .with_always_on_top(),
        ..Default::default()
    };
    eframe::run_native(
        "CleanCopy floating menu",
        options,
        Box::new(move |cc| {
            use raw_window_handle::{HasWindowHandle, RawWindowHandle};
            crate::ui::fonts::install(&cc.egui_ctx);
            cc.egui_ctx.set_visuals(egui::Visuals::light());
            let window = match cc.window_handle()?.as_raw() {
                RawWindowHandle::Xlib(h) => h.window as u32,
                RawWindowHandle::Xcb(h) => h.window.get(),
                _ => return Err("X11 window handle required".into()),
            };
            driver
                .engine
                .platform()
                .conn
                .change_window_attributes(
                    window,
                    &xproto::ChangeWindowAttributesAux::new().override_redirect(1),
                )?
                .check()?;
            // WM_HINTS input=False plus override_redirect prevents activation even on clicks.
            x11rb::wrapper::ConnectionExt::change_property32(
                &driver.engine.platform().conn,
                xproto::PropMode::REPLACE,
                window,
                AtomEnum::WM_HINTS,
                AtomEnum::WM_HINTS,
                &[1, 0, 0, 0, 0, 0, 0, 0, 0],
            )?
            .check()?;
            let mut app = LinuxApp {
                driver,
                tray,
                actions,
                enabled,
                settings_child: None,
                quitting: false,
                menu: Default::default(),
                save_path,
                shown: None,
                window,
                saved,
                frames: 0,
                last_sync: Instant::now(),
            };
            if !app.driver.engine.config().onboarding_done {
                // First launch: show onboarding; it persists the flag itself.
                if let Err(error) = app.open_window("--onboarding") {
                    eprintln!("onboarding: {error}");
                }
            }
            Ok(Box::new(app))
        }),
    )
    .map_err(err)
}
impl LinuxApp {
    /// Spawns `--settings` / `--onboarding` as a child window process, keeping
    /// at most one such window open.
    fn open_window(&mut self, flag: &str) -> Result<()> {
        if let Some(child) = &mut self.settings_child {
            if child.try_wait().map_err(err)?.is_none() {
                return Ok(());
            }
        }
        let mut command = std::process::Command::new(std::env::current_exe().map_err(err)?);
        command.arg(flag);
        if let Some(path) = &self.save_path {
            command.arg("--config").arg(path);
        }
        self.settings_child = Some(command.spawn().map_err(err)?);
        Ok(())
    }
}
struct LinuxApp {
    driver: LinuxDriver,
    tray: Option<ksni::blocking::Handle<NativeTray>>,
    actions: std::sync::mpsc::Receiver<crate::ui::menus::TrayAction>,
    enabled: bool,
    settings_child: Option<std::process::Child>,
    quitting: bool,
    menu: crate::ui::menus::MenuState,
    save_path: Option<PathBuf>,
    shown: Option<u64>,
    window: Window,
    saved: Config,
    frames: u8,
    last_sync: Instant,
}
impl LinuxApp {
    fn warm_up_menu(&self, ctx: &eframe::egui::Context, frame: u8) {
        use crate::engine::{Choice, FormatItem, MenuKind, MenuModel};
        let config = self.driver.engine.config();
        let formats: Vec<FormatItem> = config
            .secondary_items
            .iter()
            .enumerate()
            .map(|(index, item)| FormatItem {
                format: item.format,
                shortcut: u8::try_from(index + 2).unwrap_or(u8::MAX),
                enabled: true,
                muted: false,
            })
            .collect();
        let model = MenuModel {
            id: u64::MAX,
            kind: if frame.is_multiple_of(2) {
                MenuKind::Secondary
            } else {
                MenuKind::Full
            },
            app: Default::default(),
            formats,
            default_choice: Choice::Original,
            content_label: if frame > 3 {
                "正在读取剪贴板…".into()
            } else {
                "Markdown".into()
            },
            source_chars: 1_234_567,
            converting: false,
            original_enabled: true,
            undo_enabled: true,
            show_remember: true,
            target_unrecognized: true,
            loading: frame > 3,
            remember: false,
        };
        // Throwaway state: never touches the real menu's highlight or events.
        let _ = crate::ui::menus::MenuState::default().show(ctx, &model, false);
    }
    fn tray_action(&mut self, action: crate::ui::menus::TrayAction) -> Result<()> {
        use crate::ui::menus::TrayAction;
        let t = self.driver.engine.platform().now();
        match action {
            TrayAction::ShowMenu => {
                self.driver.dispatch(Event::Resume { t })?;
                self.enabled = true;
                self.driver.dispatch(Event::Cancel { t })?;
                self.driver.dispatch(Event::FullMenuHotkeyPressed { t })?;
            }
            TrayAction::Undo => self.driver.dispatch(Event::Undo { t })?,
            TrayAction::SetEnabled(enabled) => {
                self.driver.dispatch(if enabled {
                    Event::Resume { t }
                } else {
                    Event::Pause { t, until: None }
                })?;
                self.enabled = enabled;
            }
            TrayAction::Pause => {
                self.driver.dispatch(Event::Pause {
                    t,
                    until: Some(t.saturating_add(3_600_000)),
                })?;
                self.enabled = false;
            }
            TrayAction::Settings => self.open_window("--settings")?,
            TrayAction::Onboarding => self.open_window("--onboarding")?,
            TrayAction::Quit => {
                self.driver.dispatch(Event::Cancel { t })?;
                // Service pending clipboard restoration before exiting.
                self.quitting = true;
            }
        }
        Ok(())
    }
}
impl Drop for LinuxApp {
    fn drop(&mut self) {
        if let Some(tray) = &self.tray {
            let _ = tray.shutdown();
        }
    }
}
impl eframe::App for LinuxApp {
    fn update(&mut self, ctx: &eframe::egui::Context, _: &mut eframe::Frame) {
        use eframe::egui;
        if let Err(error) = self.driver.poll() {
            eprintln!("CleanCopy: {error}");
        }
        while let Ok(action) = self.actions.try_recv() {
            if !self.quitting {
                if let Err(error) = self.tray_action(action) {
                    eprintln!("CleanCopy tray: {error}");
                }
            }
        }
        if self
            .driver
            .engine
            .config()
            .paused_until
            .is_some_and(|until| until <= self.driver.engine.platform().now())
        {
            let t = self.driver.engine.platform().now();
            if self.driver.dispatch(Event::Resume { t }).is_ok() {
                self.enabled = true;
            }
        }
        let model = self.driver.engine.platform().menu.clone();
        let id = model.as_ref().map(|m| m.id);
        // Map only when a menu appears. Loading → filled keeps the mapped
        // window (re-mapping mid-WM-reparent made it flicker invisible).
        if id.is_some() != self.shown.is_some() || id.is_none() {
            let p = self.driver.engine.platform();
            if id.is_some() {
                let (x, y) = p.menu_position(640, 360);
                let _ = p.conn.configure_window(
                    self.window,
                    &xproto::ConfigureWindowAux::new()
                        .x(x)
                        .y(y)
                        .stack_mode(xproto::StackMode::ABOVE),
                );
                let _ = p.conn.map_window(self.window);
            } else {
                let _ = p.conn.unmap_window(self.window);
            }
            let _ = p.conn.flush();
            self.shown = id;
        }
        // eframe unconditionally maps after its first paint. Unmap on the next
        // frame before declaring readiness so silent operation has no visible menu.
        if self.frames == 1 {
            eprintln!("CleanCopy Linux hotkey ready");
            println!("CleanCopy Linux hotkey ready");
            use std::io::Write;
            let _ = std::io::stdout().flush();
        }
        self.frames = self.frames.saturating_add(1);
        if model.is_none() && (2..=5).contains(&self.frames) {
            // Warm-up while the pre-created window is still hidden: lay out
            // every menu variant once so glyph rasterisation and layout caches
            // are ready before the first hotkey press (the first-press outlier).
            self.warm_up_menu(ctx, self.frames);
        }
        if let Some(model) = model {
            for event in self.menu.show(ctx, &model, false) {
                let t = self.driver.engine.platform().now();
                let event = match event {
                    Event::MenuChoice { id, choice, .. } => Event::MenuChoice { t, id, choice },
                    Event::Remember { id, checked, .. } => Event::Remember { t, id, checked },
                    Event::MenuKey { id, key, .. } => Event::MenuKey { t, id, key },
                    _ => Event::Cancel { t },
                };
                if let Err(error) = self.driver.dispatch(event) {
                    eprintln!("CleanCopy: {error}");
                }
            }
        }
        let config_dirty =
            toml::to_string(self.driver.engine.config()).ok() != toml::to_string(&self.saved).ok();
        if config_dirty || self.last_sync.elapsed() >= std::time::Duration::from_millis(250) {
            self.last_sync = Instant::now();
            if let Some(path) = &self.save_path {
                // Merge daemon-owned changes with the latest settings under the same
                // file lock used by the settings process; never overwrite its edits.
                match self.driver.engine.config().save_changes(path, &self.saved) {
                    Ok(config) => {
                        let changed = toml::to_string(&config).ok()
                            != toml::to_string(self.driver.engine.config()).ok();
                        let applied = !changed
                            || (|| -> Result<()> {
                                self.driver.engine.platform_mut().update_hotkeys(&config)?;
                                self.driver.engine.update_config(config.clone())
                            })()
                            .map_err(|error| eprintln!("CleanCopy settings: {error}"))
                            .is_ok();
                        if applied {
                            self.saved = config;
                        }
                    }
                    Err(error) => eprintln!("CleanCopy config: {error}"),
                }
            }
            if let Some(tray) = &self.tray {
                let config = self.driver.engine.config();
                let enabled = self.enabled
                    && config
                        .paused_until
                        .is_none_or(|until| until <= self.driver.engine.platform().now());
                // ksni only publishes changed properties; avoid queueing updates every frame.
                let _ = tray.update(|state| {
                    if state.enabled != enabled
                        || toml::to_string(&state.config).ok() != toml::to_string(config).ok()
                    {
                        state.enabled = enabled;
                        state.config = config.clone();
                    }
                });
            }
        }
        if self.quitting && self.driver.engine.next_deadline().is_none() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if let Some(child) = &mut self.settings_child {
            if child.try_wait().ok().flatten().is_some() {
                self.settings_child = None;
            }
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(8));
        // Native mapping is managed above, not through focus-affecting viewport commands.
        let _ = egui::ViewportId::ROOT;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_independent_shortcuts_and_recorder_keys() {
        assert_eq!(parse_hotkey("Ctrl+Shift+V").unwrap(), (0x76, 5));
        assert_eq!(parse_hotkey("Ctrl+Alt+V").unwrap(), (0x76, 12));
        assert_eq!(parse_hotkey("Cmd+Option+F12").unwrap(), (0xffc9, 72));
        assert_eq!(parse_hotkey("Alt+ArrowLeft").unwrap(), (0xff51, 8));
        for invalid in ["V", "Shift+V", "Ctrl+F13", "Ctrl+", "Hyper+V"] {
            assert!(parse_hotkey(invalid).is_err(), "{invalid}");
        }
        let keys = vec![(55, vec![0x76, 0x56])];
        assert_eq!(
            configured_hotkeys(&Config::default(), &keys).unwrap().len(),
            2
        );
        let duplicate = Config {
            full_menu_hotkey: "Shift+Ctrl+v".into(),
            ..Config::default()
        };
        assert!(configured_hotkeys(&duplicate, &keys).is_err());
    }
    #[test]
    fn modifier_powerset_deduplicates() {
        assert_eq!(lock_variants(&[2, 16, 2]), vec![0, 2, 16, 18]);
        assert_eq!(lock_variants(&[]), vec![0]);
    }
    #[test]
    fn class_uses_class_not_instance_or_title() {
        assert_eq!(
            parse_wm_class(b"editor-instance\0Gedit\0"),
            Some("Gedit".into())
        );
        assert_eq!(parse_wm_class(b"instance\0\0"), Some("instance".into()));
        assert_eq!(parse_wm_class(b"\0\0"), None);
    }
    #[test]
    fn pointer_placement_clamps() {
        assert_eq!(clamp_position((999, -2), (640, 360), (1024, 768)), (384, 0));
        assert_eq!(clamp_position((20, 20), (640, 360), (100, 100)), (0, 0));
    }
    #[test]
    fn native_snapshot_roundtrip_keeps_metadata_and_opaque_atoms() {
        let native = NativeSnapshot {
            targets: vec![
                NativeTarget {
                    target: b"UTF8_STRING".to_vec(),
                    type_name: b"UTF8_STRING".to_vec(),
                    format: 8,
                    data: b"# hello".to_vec(),
                },
                NativeTarget {
                    target: vec![255, 0x61],
                    type_name: b"INTEGER".to_vec(),
                    format: 32,
                    data: vec![0, 255, 128, 0],
                },
            ],
        };
        let snapshot = encode_snapshot(&native);
        assert_eq!(snapshot.source_text(), Some("# hello"));
        assert_eq!(decode_snapshot(&snapshot).unwrap(), native);
    }
    #[test]
    fn generated_formats_have_native_utf8_alias() {
        let native = decode_snapshot(&Snapshot::text("test")).unwrap();
        assert_eq!(native.targets.len(), 2);
        assert!(native.targets.iter().any(|t| t.target == b"UTF8_STRING"));
    }
}
