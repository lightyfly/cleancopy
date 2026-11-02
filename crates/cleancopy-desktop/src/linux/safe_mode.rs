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

pub const NOTICE: &str = "CleanCopy 遇到问题，已暂停格式化。";
/// Notification action key for 「重新启动」 (GNOME has no tray, report v6 D-2).
const RESTART_ACTION: &str = "restart";
/// Safe mode retries the daemon once on its own after this long.
const AUTO_RETRY: Duration = Duration::from_secs(5 * 60);

/// Runtime directory for the safe-mode marker and restart request.
fn runtime_dir() -> std::path::PathBuf {
    std::env::var_os("CLEANCOPY_RUNTIME_DIR")
        .or_else(|| std::env::var_os("XDG_RUNTIME_DIR"))
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            // SAFETY: getuid has no preconditions.
            std::env::temp_dir().join(format!("cleancopy-{}", unsafe { libc::getuid() }))
        })
}
fn marker_path() -> std::path::PathBuf {
    runtime_dir().join("cleancopy-safe-mode.pid")
}
fn restart_path() -> std::path::PathBuf {
    std::env::var_os("CLEANCOPY_SAFE_MODE_RESTART_FILE")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| runtime_dir().join("cleancopy-safe-mode.restart"))
}

/// Whether a safe-mode supervisor is running (its marker names a live pid).
pub fn active() -> bool {
    std::fs::read_to_string(marker_path())
        .ok()
        .and_then(|pid| pid.trim().parse::<u32>().ok())
        .is_some_and(|pid| std::path::Path::new(&format!("/proc/{pid}")).exists())
}

/// Reopening Settings (or launching CleanCopy again) while safe mode is
/// active asks it to restart the daemon. Returns whether a request was made.
pub fn request_restart() -> bool {
    if !active() {
        return false;
    }
    let path = restart_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    std::fs::write(&path, b"restart").is_ok()
}

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

/// Body text of the safe-mode notification.
fn body(crashes: usize, auto_retry: bool) -> String {
    let retry = if auto_retry {
        "；5 分钟后会自动重试一次"
    } else {
        ""
    };
    format!(
        "连续崩溃 {crashes} 次。快捷键暂时粘贴原文。点「重新启动」或重新打开设置即可恢复{retry}。"
    )
}

/// Desktop notification over org.freedesktop.Notifications with a
/// 「重新启动」 action button.
fn notify(summary: &str, body: &str) -> std::result::Result<u32, String> {
    desktop_notify(summary, body, 2, &[RESTART_ACTION, "重新启动"], -1)
}

/// org.freedesktop.Notifications Notify; `urgency` 0 low … 2 critical,
/// `timeout_ms` -1 for the server default.
pub fn desktop_notify(
    summary: &str,
    body: &str,
    urgency: u8,
    actions: &[&str],
    timeout_ms: i32,
) -> std::result::Result<u32, String> {
    use std::collections::HashMap;
    let conn = zbus::blocking::Connection::session().map_err(|e| e.to_string())?;
    let hints: HashMap<&str, zbus::zvariant::Value> =
        HashMap::from([("urgency", zbus::zvariant::Value::U8(urgency))]);
    let reply = conn
        .call_method(
            Some("org.freedesktop.Notifications"),
            "/org/freedesktop/Notifications",
            Some("org.freedesktop.Notifications"),
            "Notify",
            &(
                "CleanCopy",
                0u32,
                if urgency >= 2 {
                    "dialog-warning"
                } else {
                    "dialog-information"
                },
                summary,
                body,
                actions.to_vec(),
                hints,
                timeout_ms,
            ),
        )
        .map_err(|e| e.to_string())?;
    reply.body().deserialize::<u32>().map_err(|e| e.to_string())
}

/// Forward the notification's 「重新启动」 action to `sender`.
fn listen_for_action(id: u32, sender: mpsc::Sender<SafeExit>) {
    std::thread::spawn(move || {
        let Ok(conn) = zbus::blocking::Connection::session() else {
            return;
        };
        let Ok(proxy) = zbus::blocking::Proxy::new(
            &conn,
            "org.freedesktop.Notifications",
            "/org/freedesktop/Notifications",
            "org.freedesktop.Notifications",
        ) else {
            return;
        };
        let Ok(signals) = proxy.receive_signal("ActionInvoked") else {
            return;
        };
        for message in signals {
            if let Ok((nid, action)) = message.body().deserialize::<(u32, String)>() {
                if nid == id && action == RESTART_ACTION {
                    eprintln!("CleanCopy safe mode: notification action 重新启动");
                    let _ = sender.send(SafeExit::Restart);
                    return;
                }
            }
        }
    });
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

/// Run until the user picks 「重新启动」 (notification action or tray),
/// reopens Settings, or 「退出」; with `auto_retry`, also restart after 5 minutes.
pub fn run(config: &Config, crashes: usize, auto_retry: bool) -> Result<SafeExit> {
    use ksni::blocking::TrayMethods;
    eprintln!("CleanCopy safe mode: {NOTICE}");
    let (sender, exits) = mpsc::channel();
    match notify(NOTICE, &body(crashes, auto_retry)) {
        Ok(id) => {
            eprintln!("CleanCopy safe mode: notification {id} shown");
            listen_for_action(id, sender.clone());
        }
        Err(error) => eprintln!("CleanCopy safe mode: notification failed: {error}"),
    }
    let tray = match (SafeTray { sender }).assume_sni_available(true).spawn() {
        Ok(handle) => Some(handle),
        Err(error) => {
            eprintln!("CleanCopy safe mode tray unavailable: {error}");
            None
        }
    };
    let shutdown = |tray: Option<ksni::blocking::Handle<SafeTray>>| {
        let _ = std::fs::remove_file(marker_path());
        if let Some(tray) = tray {
            tray.shutdown().wait();
        }
    };
    let restart_file = restart_path();
    let _ = std::fs::remove_file(&restart_file);
    let _ = std::fs::create_dir_all(runtime_dir());
    let _ = std::fs::write(marker_path(), std::process::id().to_string());
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
    let retry_after = std::env::var("CLEANCOPY_SAFE_MODE_RETRY_MS")
        .ok()
        .and_then(|ms| ms.parse().ok())
        .map_or(AUTO_RETRY, Duration::from_millis);
    let started = std::time::Instant::now();
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
        if std::fs::remove_file(&restart_file).is_ok() {
            eprintln!("CleanCopy safe mode: restart requested (Settings reopened)");
            break SafeExit::Restart;
        }
        if auto_retry && started.elapsed() >= retry_after {
            eprintln!("CleanCopy safe mode: automatic retry");
            break SafeExit::Restart;
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

    #[test]
    fn notification_text_and_restart_request() {
        assert_eq!(NOTICE, "CleanCopy 遇到问题，已暂停格式化。");
        assert!(!body(5, true).contains("托盘"));
        assert!(body(5, true).contains("5 分钟后"));
        assert!(!body(5, false).contains("5 分钟后"));
        let dir = std::env::temp_dir().join(format!("cc-safe-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("CLEANCOPY_RUNTIME_DIR", &dir);
        std::env::remove_var("CLEANCOPY_SAFE_MODE_RESTART_FILE");
        // No safe mode running: reopening Settings requests nothing.
        assert!(!request_restart());
        std::fs::write(marker_path(), std::process::id().to_string()).unwrap();
        assert!(active());
        assert!(request_restart());
        assert!(restart_path().exists());
        // A stale marker (dead pid) is ignored.
        std::fs::remove_file(restart_path()).unwrap();
        std::fs::write(marker_path(), "999999999").unwrap();
        assert!(!request_restart());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
