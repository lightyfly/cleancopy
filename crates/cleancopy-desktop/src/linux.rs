//! Opt-in X11 platform and event driver. No daemon is installed by this module.
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
    hotkeys: Vec<u8>,
    locks: Vec<u16>,
    menu_keys: Vec<u8>,
    menu: Option<MenuModel>,
    verified_window: Option<Window>,
    last_write: Option<(NativeSnapshot, Snapshot)>,
    epoch: u64,
    started: Instant,
    pub notice: Option<String>,
}
impl LinuxPlatform {
    /// Connect to DISPLAY and reserve Ctrl+Shift+V, failing on checked BadAccess.
    pub fn new() -> Result<Self> {
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
        let hotkeys: Vec<_> = keys
            .iter()
            .filter(|(_, s)| s.contains(&0x76) || s.contains(&0x56))
            .map(|(k, _)| *k)
            .collect();
        if hotkeys.is_empty() {
            return Err(err("keyboard map has no V key"));
        }
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
            epoch: unix_ms(),
            started: Instant::now(),
            notice: None,
        };
        for &key in &result.hotkeys {
            for &locks in &result.locks {
                result
                    .conn
                    .grab_key(
                        false,
                        root,
                        ModMask::from(u16::from(ModMask::CONTROL | ModMask::SHIFT) | locks),
                        key,
                        GrabMode::ASYNC,
                        GrabMode::ASYNC,
                    )
                    .map_err(err)?
                    .check()
                    .map_err(|e| {
                        err(format!(
                            "cannot register Ctrl+Shift+V (global hotkey conflict/BadAccess): {e}"
                        ))
                    })?;
            }
        }
        result.conn.flush().map_err(err)?;
        Ok(result)
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
                if self.hotkeys.contains(&key.detail)
                    && u16::from(key.state) & u16::from(ModMask::CONTROL | ModMask::SHIFT)
                        == u16::from(ModMask::CONTROL | ModMask::SHIFT)
                {
                    events.push(Event::HotkeyPressed { t: self.now() });
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
        let shifts: Vec<_> = self
            .keys
            .iter()
            .filter(|(k, s)| held(*k) && (s.contains(&0xffe1) || s.contains(&0xffe2)))
            .map(|(k, _)| *k)
            .collect();
        let controls: Vec<_> = self
            .keys
            .iter()
            .filter(|(k, s)| held(*k) && (s.contains(&0xffe3) || s.contains(&0xffe4)))
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
        // Do not restore Shift: restoring a physically released modifier would leave
        // the server with a stuck Shift. Real future key events establish its state.
        for shift in shifts {
            fake(xproto::KEY_RELEASE_EVENT, shift)?;
        }
        for ctrl in &controls {
            fake(xproto::KEY_RELEASE_EVENT, *ctrl)?;
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
        Ok(Self {
            engine: Engine::new(config, LinuxPlatform::new()?),
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

/// Run the opt-in native menu host on the main thread. No settings/tray/default-daemon change.
pub fn run(config: Config, save_path: Option<PathBuf>) -> Result<()> {
    use eframe::egui;
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
            Ok(Box::new(LinuxApp {
                driver,
                menu: Default::default(),
                save_path,
                shown: None,
                window,
                saved: String::new(),
                frames: 0,
            }))
        }),
    )
    .map_err(err)
}
struct LinuxApp {
    driver: LinuxDriver,
    menu: crate::ui::menus::MenuState,
    save_path: Option<PathBuf>,
    shown: Option<u64>,
    window: Window,
    saved: String,
    frames: u8,
}
impl eframe::App for LinuxApp {
    fn update(&mut self, ctx: &eframe::egui::Context, _: &mut eframe::Frame) {
        use eframe::egui;
        if let Err(error) = self.driver.poll() {
            eprintln!("CleanCopy: {error}");
        }
        let model = self.driver.engine.platform().menu.clone();
        let id = model.as_ref().map(|m| m.id);
        if id != self.shown || id.is_none() {
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
        if let Some(model) = model {
            for event in self.menu.show(ctx, &model, 0, false) {
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
        if let Some(path) = &self.save_path {
            if let Ok(serialized) = toml::to_string(self.driver.engine.config()) {
                if serialized != self.saved {
                    match self.driver.engine.config().save(path) {
                        Ok(()) => self.saved = serialized,
                        Err(error) => eprintln!("CleanCopy config: {error}"),
                    }
                }
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
