//! Opt-in native integration: owns an isolated X server and independent peers.
#![cfg(target_os = "linux")]

use cleancopy_desktop::config::{AppIdentity, AppRule, Config, Format};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

struct Processes(Vec<Child>, PathBuf);
impl Drop for Processes {
    fn drop(&mut self) {
        if thread::panicking() {
            for name in [
                "linux.log",
                "linux.stdout",
                "fixture.log",
                "target.log",
                "wm.log",
            ] {
                eprintln!(
                    "{name}: {}",
                    fs::read_to_string(self.1.join(name)).unwrap_or_default()
                );
            }
        }
        for child in self.0.iter_mut().rev() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
impl Processes {
    fn spawn(&mut self, command: &mut Command) {
        self.0
            .push(command.spawn().expect("launch native test peer"));
    }
}
/// Longer than the idle daemon's 1 s tick, which is when it rereads the config.
const CONFIG_RELOAD: Duration = Duration::from_millis(1500);
fn wait_for(description: &str, condition: impl FnMut() -> bool) {
    wait_every(description, 5, condition);
}
/// Restore polls must stay slow: each one reads the clipboard, and restoration
/// waits for the owner to be idle for 100 ms.
fn wait_every(description: &str, poll_ms: u64, mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while !condition() {
        assert!(Instant::now() < deadline, "timed out: {description}");
        thread::sleep(Duration::from_millis(poll_ms));
    }
}
fn xdo(display: &str, args: &[&str]) {
    let output = Command::new("xdotool")
        .env("DISPLAY", display)
        .args(args)
        .output()
        .unwrap();
    assert!(output.status.success(), "xdotool: {output:?}");
}
fn read_target(binary: &Path, display: &str, target: &str) -> Vec<u8> {
    let output = Command::new(binary)
        .env("DISPLAY", display)
        .args(["read", target])
        .output()
        .unwrap();
    assert!(output.status.success(), "selection read: {output:?}");
    output.stdout
}

#[test]
#[ignore = "requires Xvfb, xfwm4, xdotool; builds native examples"]
fn hotkey_pastes_formatted_text_and_restores_all_native_bytes() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    assert!(Command::new("cargo")
        .current_dir(&root)
        .args(["build", "-p", "cleancopy-desktop", "--examples"])
        .status()
        .unwrap()
        .success());
    let target_dir = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .map(|path| {
            if path.is_absolute() {
                path
            } else {
                root.join(path)
            }
        })
        .unwrap_or_else(|| root.join("target"));
    let peer = target_dir.join("debug/examples/target");
    let runner = target_dir.join("debug/examples/linux");
    let dir = tempfile::tempdir().unwrap();
    let log = |name: &str| Stdio::from(fs::File::create(dir.path().join(name)).unwrap());
    let display_file = dir.path().join("display");
    let mut processes = Processes(Vec::new(), dir.path().to_path_buf());
    processes.spawn(
        Command::new("Xvfb")
            .args([
                "-displayfd",
                "1",
                "-screen",
                "0",
                "1280x900x24",
                "-nolisten",
                "tcp",
            ])
            .stdout(Stdio::from(fs::File::create(&display_file).unwrap()))
            .stderr(log("xvfb.log")),
    );
    wait_for("isolated X server", || {
        fs::read_to_string(&display_file).is_ok_and(|s| !s.trim().is_empty())
    });
    let display = format!(":{}", fs::read_to_string(display_file).unwrap().trim());
    processes.spawn(
        Command::new("xfwm4")
            .env("DISPLAY", &display)
            .stderr(log("wm.log"))
            .stdout(Stdio::null()),
    );
    // xdotool windowactivate refuses to run until the WM advertises this hint.
    wait_for("window manager EWMH readiness", || {
        Command::new("xprop")
            .env("DISPLAY", &display)
            .args(["-root", "_NET_SUPPORTED"])
            .output()
            .is_ok_and(|out| String::from_utf8_lossy(&out.stdout).contains("_NET_ACTIVE_WINDOW"))
    });
    let fixture = dir.path().join("fixture");
    let app = dir.path().join("app");
    fs::create_dir_all(&fixture).unwrap();
    let source = b"# Heading\r\n\r\n**bold**\r\n";
    let opaque = [0, 255, 128, 13, 0, 7];
    let native16: Vec<_> = [0u16, 0x1234, 0xffff]
        .into_iter()
        .flat_map(u16::to_ne_bytes)
        .collect();
    let native32: Vec<_> = [0u32, 0x12345678, u32::MAX]
        .into_iter()
        .flat_map(u32::to_ne_bytes)
        .collect();
    fs::write(fixture.join("text"), source).unwrap();
    fs::write(fixture.join("opaque"), opaque).unwrap();
    fs::write(fixture.join("words"), &native16).unwrap();
    fs::write(fixture.join("longs"), &native32).unwrap();
    fs::write(fixture.join("targets.tsv"), "UTF8_STRING\tUTF8_STRING\t8\ttext\napplication/x-opaque\tapplication/x-bytes\t8\topaque\napplication/x-words\tINTEGER\t16\twords\napplication/x-longs\tCARDINAL\t32\tlongs\n").unwrap();
    processes.spawn(
        Command::new(&peer)
            .env("DISPLAY", &display)
            .arg("fixture")
            .arg(&fixture)
            .stderr(log("fixture.log")),
    );
    wait_for("fixture ownership", || fixture.join("ready").exists());
    let original_targets = read_target(&peer, &display, "TARGETS");
    let native_metadata = |target: &str| {
        let output = Command::new(&peer)
            .env("DISPLAY", &display)
            .args(["read", target])
            .output()
            .unwrap();
        assert!(output.status.success(), "native metadata: {output:?}");
        output.stderr
    };
    let metadata_before: Vec<_> = [
        "UTF8_STRING",
        "application/x-opaque",
        "application/x-words",
        "application/x-longs",
    ]
    .map(native_metadata)
    .into_iter()
    .collect();
    processes.spawn(
        Command::new(&peer)
            .env("DISPLAY", &display)
            .args(["app", "gedit"])
            .arg(&app)
            .stderr(log("target.log")),
    );
    wait_for("mapped target", || app.join("ready").exists());
    let window = fs::read_to_string(app.join("window")).unwrap();
    xdo(&display, &["windowactivate", "--sync", window.trim()]);
    let config = Config {
        app_rules: vec![AppRule {
            app: AppIdentity {
                wm_class: Some("gedit".into()),
                ..Default::default()
            },
            format: Format::ChineseNumbered,
        }],
        // Otherwise the daemon relaunches this example as `--onboarding`.
        onboarding_done: true,
        ..Config::default()
    };
    let config_path = dir.path().join("config.toml");
    config.save(&config_path).unwrap();
    let runner_log = dir.path().join("linux.log");
    processes.spawn(
        Command::new(&runner)
            .env("DISPLAY", &display)
            .arg(&config_path)
            .stdout(log("linux.stdout"))
            .stderr(Stdio::from(fs::File::create(&runner_log).unwrap())),
    );
    wait_for("registered hotkey", || {
        fs::read_to_string(&runner_log).is_ok_and(|s| s.contains("hotkey ready"))
    });
    let focus = || {
        let output = Command::new("xdotool")
            .env("DISPLAY", &display)
            .arg("getwindowfocus")
            .output()
            .unwrap();
        assert!(output.status.success());
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    };
    assert_eq!(focus(), window.trim());
    xdo(&display, &["key", "ctrl+shift+v"]);
    wait_for("formatted native paste", || {
        app.join("paste-1.bin").exists()
    });
    let received = fs::read(app.join("paste-1.bin")).unwrap();
    let expected = cleancopy_core::render_numbered(
        std::str::from_utf8(source).unwrap(),
        config.numbering_style,
    );
    assert_eq!(received, expected.as_bytes());
    assert_ne!(received, source);
    assert_eq!(
        focus(),
        window.trim(),
        "silent paste must keep target focused"
    );
    let visible_menu = Command::new("xdotool")
        .env("DISPLAY", &display)
        .args([
            "search",
            "--onlyvisible",
            "--name",
            "^CleanCopy floating menu$",
        ])
        .output()
        .unwrap();
    assert!(
        !visible_menu.status.success(),
        "silent paste must not map a menu"
    );
    wait_every("original clipboard restoration", 200, || {
        read_target(&peer, &display, "UTF8_STRING") == source
    });
    let mut expected_targets: Vec<_> = String::from_utf8(original_targets)
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect();
    let mut restored_targets: Vec<_> = String::from_utf8(read_target(&peer, &display, "TARGETS"))
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect();
    expected_targets.sort();
    restored_targets.sort();
    assert_eq!(restored_targets, expected_targets);
    let metadata_after: Vec<_> = [
        "UTF8_STRING",
        "application/x-opaque",
        "application/x-words",
        "application/x-longs",
    ]
    .map(native_metadata)
    .into_iter()
    .collect();
    assert_eq!(
        metadata_after, metadata_before,
        "native property type/format must survive"
    );
    assert_eq!(read_target(&peer, &display, "application/x-opaque"), opaque);
    assert_eq!(
        read_target(&peer, &display, "application/x-words"),
        native16
    );
    assert_eq!(
        read_target(&peer, &display, "application/x-longs"),
        native32
    );
    assert!(
        !app.join("paste-2.bin").exists(),
        "one hotkey must paste once"
    );
    // Probe both lock-mask variants and an actually held Shift, not just a tap.
    xdo(&display, &["key", "Caps_Lock", "Num_Lock"]);
    xdo(&display, &["keydown", "Shift_L"]);
    xdo(&display, &["key", "ctrl+v"]);
    wait_for("paste with held Shift and lock masks", || {
        app.join("paste-2.bin").exists()
    });
    xdo(&display, &["keyup", "Shift_L"]);
    assert_eq!(
        fs::read(app.join("paste-2.bin")).unwrap(),
        expected.as_bytes()
    );
    wait_every("restoration after held Shift", 200, || {
        read_target(&peer, &display, "UTF8_STRING") == source
    });
    xdo(&display, &["key", "Caps_Lock", "Num_Lock"]);
    // A second process must report the checked passive-grab conflict.
    let mut conflict = Command::new(&runner)
        .env("DISPLAY", &display)
        .arg(&config_path)
        .stdout(Stdio::null())
        .stderr(log("conflict.log"))
        .spawn()
        .unwrap();
    let conflict_deadline = Instant::now() + Duration::from_secs(10);
    let status = loop {
        if let Some(status) = conflict.try_wait().unwrap() {
            break status;
        }
        if Instant::now() > conflict_deadline {
            let _ = conflict.kill();
            let _ = conflict.wait();
            panic!("conflicting global hotkey must fail promptly");
        }
        thread::sleep(Duration::from_millis(20));
    };
    assert!(!status.success());
    let conflict_log = fs::read_to_string(dir.path().join("conflict.log")).unwrap();
    assert!(conflict_log.contains("BadAccess"), "{conflict_log}");
    // The dedicated native full-menu hotkey must not run a conversion or steal focus.
    xdo(&display, &["mousemove", "350", "200"]);
    let menu_started = Instant::now();
    xdo(
        &display,
        &["key", "--clearmodifiers", "--delay", "0", "ctrl+alt+v"],
    );
    let visible_menu = || {
        Command::new("xdotool")
            .env("DISPLAY", &display)
            .args([
                "search",
                "--onlyvisible",
                "--name",
                "^CleanCopy floating menu$",
            ])
            .output()
            .unwrap()
    };
    // Keep the id from the search that succeeded; a second search can come back empty.
    let mut menu_search = Vec::new();
    wait_for("nonactivating floating menu", || {
        let output = visible_menu();
        menu_search = output.stdout;
        output.status.success()
    });
    assert_eq!(
        focus(),
        window.trim(),
        "floating menu must not steal input focus"
    );
    let menu_window: u32 = String::from_utf8(menu_search)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    {
        use x11rb::{
            connection::Connection,
            protocol::xproto::{ConnectionExt, ImageFormat},
        };
        let (conn, screen) = x11rb::connect(Some(&display)).unwrap();
        // Mapping alone is not evidence of visible menu contents. Observe the
        // native drawable's collapsed format-panel background after painting.
        wait_for("painted full menu", || {
            // Main card interior (white); the window outside the cards is
            // shaped away (report v4 M-5).
            let pixel = conn
                .get_image(ImageFormat::Z_PIXMAP, menu_window, 36, 130, 1, 1, u32::MAX)
                .unwrap()
                .reply()
                .unwrap();
            pixel.data[..3] == [255, 255, 255]
        });
        println!(
            "MEASURE Ctrl+Alt+V→full-menu-visible: {:.2} ms (native mapped window + expected painted panel)",
            menu_started.elapsed().as_secs_f64() * 1000.0
        );
        let attributes = conn
            .get_window_attributes(menu_window)
            .unwrap()
            .reply()
            .unwrap();
        assert!(
            attributes.override_redirect,
            "floating menu must bypass window-manager activation"
        );
        let position = conn
            .translate_coordinates(menu_window, conn.setup().roots[screen].root, 0, 0)
            .unwrap()
            .reply()
            .unwrap();
        assert!(
            (350..=380).contains(&position.dst_x),
            "menu x={}",
            position.dst_x
        );
        assert!(
            (200..=230).contains(&position.dst_y),
            "menu y={}",
            position.dst_y
        );
    }
    let screenshot = target_dir.join("debug/examples/screenshot");
    assert!(Command::new(screenshot)
        .env("DISPLAY", &display)
        .args(["/tmp/cleancopy-native-menu.png", &menu_window.to_string()])
        .status()
        .unwrap()
        .success());
    xdo(&display, &["key", "Escape"]);
    wait_for("menu dismissal", || !visible_menu().status.success());
    assert_eq!(focus(), window.trim());
    assert!(
        !app.join("paste-3.bin").exists(),
        "Ctrl+Alt+V must show the full menu without issuing a paste"
    );
    // Reload both configured bindings in the running native driver. Use letter
    // keys: on GitHub's Ubuntu runners a successful grab of Ctrl+Shift+F8 never
    // delivered KeyPress. Ctrl+Alt+Fn is out too: XKB consumes those as
    // virtual-terminal switches, even under Xvfb with a successful passive grab.
    let mut updated = config.clone();
    updated.hotkey = "Ctrl+Shift+B".into();
    updated.full_menu_hotkey = "Ctrl+Super+N".into();
    updated.save(&config_path).unwrap();
    // This allows the file-reload poll to run, not any engine press arbitration.
    thread::sleep(CONFIG_RELOAD);
    xdo(&display, &["key", "--clearmodifiers", "ctrl+shift+b"]);
    wait_for("live configured primary paste", || {
        app.join("paste-3.bin").exists()
    });
    assert_eq!(
        fs::read(app.join("paste-3.bin")).unwrap(),
        expected.as_bytes()
    );
    wait_every("restoration after configured primary", 200, || {
        read_target(&peer, &display, "UTF8_STRING") == source
    });
    xdo(&display, &["key", "--clearmodifiers", "ctrl+super+n"]);
    wait_for("live configured full-menu binding", || {
        visible_menu().status.success()
    });
    xdo(&display, &["key", "Escape"]);
    wait_for("configured menu dismissal", || {
        !visible_menu().status.success()
    });

    // Extra modifiers must not be mistaken for either configured chord.
    for chord in ["ctrl+shift+super+b", "ctrl+super+shift+n"] {
        xdo(&display, &["key", "--clearmodifiers", chord]);
        let until = Instant::now() + Duration::from_millis(150);
        while Instant::now() < until {
            assert!(
                !visible_menu().status.success(),
                "unexpected activation: {chord}"
            );
            assert!(
                !app.join("paste-4.bin").exists(),
                "unexpected paste: {chord}"
            );
            thread::sleep(Duration::from_millis(5));
        }
    }

    // Reserve the replacement full-menu chord in an independent X client.
    // The update adds its primary first, so this also tests partial-grab cleanup.
    {
        use x11rb::{
            connection::Connection,
            protocol::xproto::{ConnectionExt, GrabMode, ModMask},
        };
        let (conn, screen) = x11rb::connect(Some(&display)).unwrap();
        let root_window = conn.setup().roots[screen].root;
        let setup = conn.setup();
        let mapping = conn
            .get_keyboard_mapping(setup.min_keycode, setup.max_keycode - setup.min_keycode + 1)
            .unwrap()
            .reply()
            .unwrap();
        let keycode = |symbol| {
            mapping
                .keysyms
                .chunks(usize::from(mapping.keysyms_per_keycode))
                .position(|symbols| symbols.contains(&symbol))
                .map(|index| setup.min_keycode + u8::try_from(index).unwrap())
                .expect("function key in native keyboard map")
        };
        // The released default binding must be available to another client.
        // Grab it here before injecting it: the target peer intentionally treats
        // any Ctrl+V (including Alt) as paste, so forwarding it is not a daemon paste.
        conn.grab_key(
            false,
            root_window,
            ModMask::CONTROL | ModMask::M1,
            keycode(0x76),
            GrabMode::ASYNC,
            GrabMode::ASYNC,
        )
        .unwrap()
        .check()
        .expect("live reload must release the old full-menu binding");
        xdo(&display, &["key", "--clearmodifiers", "ctrl+alt+v"]);
        thread::sleep(Duration::from_millis(150));
        assert!(!visible_menu().status.success());
        assert!(!app.join("paste-4.bin").exists());
        let blocked = keycode(0xffc9); // F12
        conn.grab_key(
            false,
            root_window,
            ModMask::CONTROL | ModMask::M4,
            blocked,
            GrabMode::ASYNC,
            GrabMode::ASYNC,
        )
        .unwrap()
        .check()
        .unwrap();
        let mut conflicting = updated.clone();
        conflicting.hotkey = "Ctrl+Shift+F10".into();
        conflicting.full_menu_hotkey = "Ctrl+Super+F12".into();
        conflicting.save(&config_path).unwrap();
        wait_for("live hotkey conflict reported", || {
            fs::read_to_string(&runner_log).is_ok_and(|text| {
                text.contains("CleanCopy settings:") && text.contains("BadAccess")
            })
        });
        xdo(&display, &["key", "--clearmodifiers", "ctrl+shift+b"]);
        wait_for("primary retained after failed update", || {
            app.join("paste-4.bin").exists()
        });
        assert_eq!(
            fs::read(app.join("paste-4.bin")).unwrap(),
            expected.as_bytes()
        );
        wait_every("restoration after rollback", 200, || {
            read_target(&peer, &display, "UTF8_STRING") == source
        });
        xdo(&display, &["key", "--clearmodifiers", "ctrl+super+n"]);
        wait_for("full-menu retained after failed update", || {
            visible_menu().status.success()
        });
        xdo(&display, &["key", "Escape"]);
        wait_for("rollback menu dismissal", || {
            !visible_menu().status.success()
        });
        // Stop retries before probing cleanup of the partially registered F10.
        updated.save(&config_path).unwrap();
        thread::sleep(CONFIG_RELOAD);
        conn.grab_key(
            false,
            root_window,
            ModMask::CONTROL | ModMask::SHIFT,
            keycode(0xffc7),
            GrabMode::ASYNC,
            GrabMode::ASYNC,
        )
        .unwrap()
        .check()
        .expect("failed update must release newly added F10 grab");
    }
    assert_eq!(focus(), window.trim());
    println!("PASS configured hotkey live update, exact modifier matching, conflict rollback + partial-grab cleanup");
    println!(
        "native Ctrl+Shift+V → {} formatted bytes; restored all 4 native targets byte-exactly; CapsLock+NumLock+held Shift, BadAccess conflict, native Ctrl+Alt+V pointer-positioned nonfocus menu/Escape passed",
        received.len()
    );
}
