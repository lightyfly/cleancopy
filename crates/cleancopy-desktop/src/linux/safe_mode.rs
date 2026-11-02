//! Safe mode after the supervisor gives up (report v5 N-3): no formatting
//! code runs. A system notification says formatting is paused, the tray
//! offers 「重新启动」, and the paste hotkey keeps working as a plain Ctrl+V,
//! so the user's original clipboard text is pasted unchanged.

use super::{configured_hotkeys, err, keyboard_layout, lock_variants, Result};
use crate::config::Config;
use std::{sync::mpsc, time::Duration};
use x11rb::{
    connection::Connection,
    protocol::{
        xproto::{self, ConnectionExt, GrabMode, ModMask, Window},
        xtest::ConnectionExt as _,
        Event as XEvent,
    },
    rust_connection::RustConnection,
    CURRENT_TIME,
};

pub const NOTICE: &str = "CleanCopy 遇到问题，已暂停格式化";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SafeExit {
    Restart,
    Quit,
}

struct SafeTray {
    sender: mpsc::Sender<SafeExit>,
}
impl ksni::Tray for SafeTray {
    const MENU_ON_ACTIVATE: bool = true;
    fn id(&self) -> String {
        "cleancopy".into()
    }
    fn title(&self) -> String {
        "CleanCopy（格式化已暂停）".into()
    }
    fn icon_name(&self) -> String {
        "dialog-warning".into()
    }
    fn watcher_offline(&self, reason: ksni::OfflineReason) -> bool {
        eprintln!("CleanCopy safe mode tray: no StatusNotifier host ({reason:?})");
        true
    }
    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        use ksni::menu::StandardItem;
        vec![
            StandardItem {
                label: NOTICE.into(),
                enabled: false,
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "快捷键暂时粘贴原文".into(),
                enabled: false,
                ..Default::default()
            }
            .into(),
            ksni::MenuItem::Separator,
            StandardItem {
                label: "重新启动".into(),
                activate: Box::new(|tray: &mut Self| {
                    let _ = tray.sender.send(SafeExit::Restart);
                }),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "退出 CleanCopy".into(),
                activate: Box::new(|tray: &mut Self| {
                    let _ = tray.sender.send(SafeExit::Quit);
                }),
                ..Default::default()
            }
            .into(),
        ]
    }
}

/// Desktop notification over org.freedesktop.Notifications.
fn notify(summary: &str, body: &str) -> std::result::Result<u32, String> {
    use std::collections::HashMap;
    let conn = zbus::blocking::Connection::session().map_err(|e| e.to_string())?;
    let hints: HashMap<&str, zbus::zvariant::Value> =
        HashMap::from([("urgency", zbus::zvariant::Value::U8(2))]);
    let reply = conn
        .call_method(
            Some("org.freedesktop.Notifications"),
            "/org/freedesktop/Notifications",
            Some("org.freedesktop.Notifications"),
            "Notify",
            &(
                "CleanCopy",
                0u32,
                "dialog-warning",
                summary,
                body,
                Vec::<&str>::new(),
                hints,
                -1i32,
            ),
        )
        .map_err(|e| e.to_string())?;
    reply.body().deserialize::<u32>().map_err(|e| e.to_string())
}

struct Passthrough {
    conn: RustConnection,
    root: Window,
    keys: super::KeyTable,
    control: u8,
    v: u8,
    grabbed: Vec<(u8, u16)>,
}
impl Passthrough {
    fn new(config: &Config) -> Result<Self> {
        let (conn, screen) = x11rb::connect(None).map_err(err)?;
        let root = conn.setup().roots[screen].root;
        conn.xtest_get_version(2, 2)
            .map_err(err)?
            .reply()
            .map_err(err)?;
        let (keys, lock_masks) = keyboard_layout(&conn)?;
        let code = |symbol: u32| {
            keys.iter()
                .find(|(_, s)| s.first() == Some(&symbol))
                .or_else(|| keys.iter().find(|(_, s)| s.contains(&symbol)))
                .map(|(k, _)| *k)
                .ok_or_else(|| err(format!("no keycode for keysym {symbol:#x}")))
        };
        let (control, v) = (code(0xffe3)?, code(u32::from(b'v'))?);
        let mut grabbed = Vec::new();
        for (key, modifiers, _) in configured_hotkeys(config, &keys)? {
            for locks in lock_variants(&lock_masks) {
                let mask = modifiers | locks;
                let ok = conn
                    .grab_key(
                        false,
                        root,
                        ModMask::from(mask),
                        key,
                        GrabMode::ASYNC,
                        GrabMode::ASYNC,
                    )
                    .map_err(err)?
                    .check()
                    .is_ok();
                if ok {
                    grabbed.push((key, mask));
                }
            }
        }
        conn.flush().map_err(err)?;
        Ok(Self {
            conn,
            root,
            keys,
            control,
            v,
            grabbed,
        })
    }
    /// Plain Ctrl+V into the focused app: the clipboard is never touched.
    fn paste_original(&self) -> Result<()> {
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
        let fake = |kind, code| -> Result<()> {
            self.conn
                .xtest_fake_input(kind, code, CURRENT_TIME, self.root, 0, 0, 0)
                .map_err(err)?
                .check()
                .map_err(err)
        };
        for (key, symbols) in &self.keys {
            let modifier = symbols
                .iter()
                .any(|s| matches!(*s, 0xffe1..=0xffe4 | 0xffe7..=0xffee));
            if held(*key) && (modifier || self.grabbed.iter().any(|(k, _)| k == key)) {
                fake(xproto::KEY_RELEASE_EVENT, *key)?;
            }
        }
        fake(xproto::KEY_PRESS_EVENT, self.control)?;
        fake(xproto::KEY_PRESS_EVENT, self.v)?;
        fake(xproto::KEY_RELEASE_EVENT, self.v)?;
        fake(xproto::KEY_RELEASE_EVENT, self.control)?;
        self.conn.flush().map_err(err)
    }
    fn presses(&self) -> Result<usize> {
        let mut n = 0;
        while let Some(event) = self.conn.poll_for_event().map_err(err)? {
            if let XEvent::KeyPress(key) = event {
                let state = u16::from(key.state) & 0xff;
                if self
                    .grabbed
                    .iter()
                    .any(|&(code, mask)| code == key.detail && mask == state)
                {
                    n += 1;
                }
            }
        }
        Ok(n)
    }
}
impl Drop for Passthrough {
    fn drop(&mut self) {
        for &(key, mask) in &self.grabbed {
            let _ = self.conn.ungrab_key(key, self.root, ModMask::from(mask));
        }
        let _ = self.conn.flush();
    }
}

/// Run until the user picks 「重新启动」 or 「退出」 from the tray.
pub fn run(config: &Config, crashes: usize) -> Result<SafeExit> {
    use ksni::blocking::TrayMethods;
    eprintln!("CleanCopy safe mode: {NOTICE}");
    let body = format!("连续崩溃 {crashes} 次。快捷键暂时粘贴原文；可在托盘菜单「重新启动」。");
    match notify(NOTICE, &body) {
        Ok(id) => eprintln!("CleanCopy safe mode: notification {id} shown"),
        Err(error) => eprintln!("CleanCopy safe mode: notification failed: {error}"),
    }
    let (sender, exits) = mpsc::channel();
    let tray = match (SafeTray { sender }).assume_sni_available(true).spawn() {
        Ok(handle) => Some(handle),
        Err(error) => {
            eprintln!("CleanCopy safe mode tray unavailable: {error}");
            None
        }
    };
    let shutdown = |tray: Option<ksni::blocking::Handle<SafeTray>>| {
        if let Some(tray) = tray {
            tray.shutdown().wait();
        }
    };
    let passthrough = match Passthrough::new(config) {
        Ok(p) => p,
        Err(error) => {
            shutdown(tray);
            return Err(error);
        }
    };
    eprintln!(
        "CleanCopy safe mode: hotkey passthrough ready ({} grabs)",
        passthrough.grabbed.len()
    );
    // Test hook: a file whose presence requests a restart (no tray host in CI).
    let restart_file = std::env::var_os("CLEANCOPY_SAFE_MODE_RESTART_FILE");
    let exit = loop {
        let presses = match passthrough.presses() {
            Ok(n) => n,
            Err(error) => {
                shutdown(tray);
                return Err(error);
            }
        };
        for _ in 0..presses {
            match passthrough.paste_original() {
                Ok(()) => eprintln!("CleanCopy safe mode: pasted original text"),
                Err(error) => eprintln!("CleanCopy safe mode: paste failed: {error}"),
            }
        }
        if let Ok(exit) = exits.try_recv() {
            break exit;
        }
        if let Some(path) = &restart_file {
            if std::fs::remove_file(path).is_ok() {
                break SafeExit::Restart;
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    // Free the hotkey and the tray name before a restarted daemon claims them.
    drop(passthrough);
    shutdown(tray);
    Ok(exit)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ksni::Tray as _;

    #[test]
    fn safe_tray_offers_restart_and_quit() {
        let (sender, exits) = mpsc::channel();
        let mut tray = SafeTray { sender };
        let mut labels = Vec::new();
        for item in tray.menu() {
            if let ksni::MenuItem::Standard(item) = item {
                labels.push((item.label.clone(), item.enabled));
                if item.label == "重新启动" {
                    (item.activate)(&mut tray);
                }
            }
        }
        assert_eq!(labels[0], (NOTICE.to_string(), false));
        assert!(labels.contains(&("重新启动".into(), true)));
        assert!(labels.contains(&("退出 CleanCopy".into(), true)));
        assert_eq!(exits.try_recv(), Ok(SafeExit::Restart));
    }
}
