//! Opt-in process-only desktop acceptance harness. Drives the shipped daemon on Xvfb.
#![cfg(target_os = "linux")]
use cleancopy_desktop::config::{Config, Format};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};
use x11rb::protocol::xproto::{ConnectionExt, ImageFormat};

struct Harness {
    children: Vec<Child>,
    dir: tempfile::TempDir,
    display: String,
    bin: PathBuf,
    root: PathBuf,
    /// Screenshot output: `target/e2e-screenshots` by default; tracked
    /// `docs/screenshots` only with `CLEANCOPY_UPDATE_SCREENSHOTS=1`
    /// (`scripts/e2e-desktop.sh --update-screenshots`).
    shots: PathBuf,
}
/// Matches the app's own windows ("CleanCopy", "CleanCopy 设置",
/// "欢迎使用 CleanCopy") but not the pasteprobe target or the floating menu.
/// xdotool matches case-insensitively and byte-wise (no UTF-8 in the regex).
const APP_WINDOW: &str = "CleanCopy($| [^nf])";

fn screenshot_dir(root: &Path) -> PathBuf {
    let dir = if std::env::var_os("CLEANCOPY_UPDATE_SCREENSHOTS").is_some_and(|v| v == "1") {
        root.join("docs/screenshots")
    } else {
        root.join("target/e2e-screenshots")
    };
    fs::create_dir_all(&dir).unwrap();
    dir
}
fn wait(label: &str, mut f: impl FnMut() -> bool) {
    let start = Instant::now();
    while !f() {
        assert!(
            start.elapsed() < Duration::from_secs(15),
            "timeout: {label}"
        );
        thread::sleep(Duration::from_millis(5));
    }
}
impl Drop for Harness {
    fn drop(&mut self) {
        for child in self.children.iter_mut().rev() {
            let _ = child.kill();
            let _ = child.wait();
        }
        for entry in fs::read_dir(self.dir.path()).unwrap().flatten() {
            if entry.path().extension().is_some_and(|s| s == "log") {
                println!(
                    "{}:\n{}",
                    entry.path().display(),
                    fs::read_to_string(entry.path()).unwrap_or_default()
                );
            }
        }
    }
}
impl Harness {
    fn spawn(&mut self, cmd: &mut Command, name: &str) -> usize {
        let log = fs::File::create(self.dir.path().join(format!("{name}.log"))).unwrap();
        let child = cmd
            .stderr(Stdio::from(log.try_clone().unwrap()))
            .stdout(Stdio::from(log))
            .spawn()
            .unwrap();
        self.children.push(child);
        self.children.len() - 1
    }
    fn xdo(&self, args: &[&str]) -> Vec<u8> {
        let out = Command::new("xdotool")
            .env("DISPLAY", &self.display)
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "xdotool {args:?}: {out:?}");
        out.stdout
    }
    fn key(&self, keys: &[&str]) {
        let mut args = vec!["key", "--clearmodifiers", "--delay", "0"];
        args.extend(keys);
        self.xdo(&args);
    }
    fn menu(&self) -> Option<u32> {
        let out = Command::new("xdotool")
            .env("DISPLAY", &self.display)
            .args([
                "search",
                "--onlyvisible",
                "--name",
                "^CleanCopy floating menu$",
            ])
            .output()
            .unwrap();
        String::from_utf8(out.stdout)
            .unwrap()
            .lines()
            .next()
            .and_then(|s| s.parse().ok())
    }
    fn pixel(&self, window: u32, x: i16, y: i16) -> Vec<u8> {
        let (c, _) = x11rb::connect(Some(&self.display)).unwrap();
        c.get_image(ImageFormat::Z_PIXMAP, window, x, y, 1, 1, u32::MAX)
            .unwrap()
            .reply()
            .unwrap()
            .data
    }
    /// The menu's input region covers only the cards, not the 640x360 canvas.
    fn assert_shaped(&self, window: u32) {
        use x11rb::protocol::shape::{ConnectionExt as _, SK};
        let (c, _) = x11rb::connect(Some(&self.display)).unwrap();
        let rects = c
            .shape_get_rectangles(window, SK::INPUT)
            .unwrap()
            .reply()
            .unwrap()
            .rectangles;
        let area: u32 = rects
            .iter()
            .map(|r| u32::from(r.width) * u32::from(r.height))
            .sum();
        assert!(
            !rects.is_empty() && area < 640 * 360 / 2,
            "menu input region must be limited to the cards: {rects:?}"
        );
    }
    fn open(&self, full_hotkey: bool, secondary: bool, shot: &str) -> u32 {
        let ready_count = self.metadata_completions();
        let id = self.open_measured(full_hotkey, secondary, shot, false).0;
        self.filled(ready_count, shot);
        id
    }
    /// Wait for the asynchronous clipboard read to fill the menu, then capture it.
    fn filled(&self, ready_count: usize, shot: &str) {
        wait("asynchronous menu metadata", || {
            self.metadata_completions() > ready_count
        });
        // Let the filled model paint before capturing.
        thread::sleep(Duration::from_millis(60));
        self.shot(shot);
    }
    fn metadata_completions(&self) -> usize {
        fs::read_to_string(self.dir.path().join("daemon.log"))
            .unwrap_or_default()
            .matches("CleanCopy menu metadata ready")
            .count()
    }
    fn open_measured(
        &self,
        full_hotkey: bool,
        secondary: bool,
        label: &str,
        screenshot: bool,
    ) -> (u32, Duration) {
        assert!(self.menu().is_none(), "timing requires a hidden menu");
        self.xdo(&["mousemove", "350", "200"]);
        let start = Instant::now();
        self.key(if full_hotkey {
            &["ctrl+alt+v"]
        } else {
            &["ctrl+shift+v"]
        });
        // Keep the id from the successful search: a second `xdotool search`
        // can come back empty while unrelated fixture windows are destroyed.
        let mut found = None;
        wait("menu mapped", || {
            found = self.menu();
            found.is_some()
        });
        let id = found.unwrap();
        // Real rendered contents: format-panel interior is white only when expanded.
        wait("menu painted with expected contents", || {
            // The main card interior is white once painted; the format panel
            // at 580,100 is white only when expanded. Outside the cards the
            // window is shaped away (report v4 M-5), so no grey margin.
            let card = self.pixel(id, 36, 130);
            let p = self.pixel(id, 580, 100);
            card[..3] == [255, 255, 255] && ((p[..3] == [255, 255, 255]) == secondary)
        });
        self.assert_shaped(id);
        let elapsed = start.elapsed();
        let route = if full_hotkey {
            "Ctrl+Alt+V→full-menu-visible"
        } else if secondary {
            "unknown-app→secondary-visible"
        } else {
            "Ctrl+Shift+V→full-menu-visible"
        };
        println!(
            "MEASURE {route} {label}: {:.2} ms (native mapped window + expected painted panel)",
            elapsed.as_secs_f64() * 1000.0
        );
        if secondary {
            assert!(
                elapsed <= Duration::from_millis(100),
                "unknown-app→secondary-visible exceeded 100 ms: {elapsed:?}"
            );
        }
        if screenshot {
            self.shot(label);
        }
        (id, elapsed)
    }
    fn shot(&self, name: &str) {
        let path = self.shots.join(format!("e2e-{name}.png"));
        // xfwm4 composites redirected windows, so a root GetImage omits the
        // menu. Capture the menu window itself whenever one is mapped.
        let mut command = Command::new(self.bin.join("examples/screenshot"));
        command.env("DISPLAY", &self.display).arg(&path);
        if let Some(menu) = self.menu() {
            command.arg(menu.to_string());
        }
        assert!(command.status().unwrap().success());
    }
    fn close(&self) {
        self.key(&["Escape"]);
        wait("menu dismissed", || self.menu().is_none());
    }
    fn read(&self, target: &str) -> (Vec<u8>, Vec<u8>) {
        let out = Command::new(self.bin.join("examples/target"))
            .env("DISPLAY", &self.display)
            .args(["read", target])
            .output()
            .unwrap();
        assert!(out.status.success(), "read {target}: {out:?}");
        (out.stdout, out.stderr)
    }
    fn snapshot(&self) -> Vec<(String, Vec<u8>, Vec<u8>)> {
        let (targets, _) = self.read("TARGETS");
        let mut names: Vec<_> = String::from_utf8(targets)
            .unwrap()
            .lines()
            .map(str::to_owned)
            .collect();
        names.sort();
        names
            .into_iter()
            .map(|name| {
                let (mut bytes, meta) = self.read(&name);
                // TARGETS is protocol capability metadata, not a clipboard payload.
                // Its atom order is unspecified; compare the exact advertised set.
                if name == "TARGETS" {
                    let text = String::from_utf8(bytes).unwrap();
                    let mut targets: Vec<_> = text.lines().collect();
                    targets.sort();
                    bytes = targets.join("\n").into_bytes();
                }
                (name, bytes, meta)
            })
            .collect()
    }
    fn restored(&self, before: &[(String, Vec<u8>, Vec<u8>)]) {
        wait("original text restored", || {
            self.read("UTF8_STRING").0 == before.iter().find(|t| t.0 == "UTF8_STRING").unwrap().1
        });
        assert_eq!(
            self.snapshot(),
            before,
            "all advertised targets, bytes and native metadata must restore exactly"
        );
        println!(
            "RESTORE all {} advertised targets: raw bytes + property type/format exact",
            before.len()
        );
    }
    fn fixture(&mut self, source: &[u8], name: &str) {
        let dir = self.dir.path().join(name);
        fs::create_dir(&dir).unwrap();
        fs::write(dir.join("text"), source).unwrap();
        fs::write(dir.join("opaque"), [0, 255, 128, 13, 0, 7]).unwrap();
        fs::write(
            dir.join("words"),
            [0u16, 0x1234, 0xffff]
                .into_iter()
                .flat_map(u16::to_ne_bytes)
                .collect::<Vec<_>>(),
        )
        .unwrap();
        fs::write(
            dir.join("longs"),
            [0u32, 0x12345678, u32::MAX]
                .into_iter()
                .flat_map(u32::to_ne_bytes)
                .collect::<Vec<_>>(),
        )
        .unwrap();
        fs::write(dir.join("targets.tsv"),"UTF8_STRING\tUTF8_STRING\t8\ttext\napplication/x-opaque\tapplication/x-bytes\t8\topaque\napplication/x-words\tINTEGER\t16\twords\napplication/x-longs\tCARDINAL\t32\tlongs\n").unwrap();
        self.spawn(
            Command::new(self.bin.join("examples/target"))
                .env("DISPLAY", &self.display)
                .arg("fixture")
                .arg(&dir),
            name,
        );
        wait("fixture ready", || dir.join("ready").exists());
    }
    fn target(&mut self, class: &str) -> PathBuf {
        let dir = self.dir.path().join(class);
        self.spawn(
            Command::new(self.bin.join("examples/target"))
                .env("DISPLAY", &self.display)
                .args(["app", class])
                .arg(&dir),
            class,
        );
        wait("target mapped", || dir.join("ready").exists());
        self.focus(&dir);
        dir
    }
    fn focus(&self, dir: &Path) {
        let id = fs::read_to_string(dir.join("window")).unwrap();
        self.xdo(&["windowactivate", "--sync", id.trim()]);
    }
    fn paste(&self, dir: &Path, n: usize) -> Vec<u8> {
        let path = dir.join(format!("paste-{n}.bin"));
        wait("native target paste", || path.exists());
        fs::read(path).unwrap()
    }
}

#[test]
#[ignore = "native app process matrix; run scripts/e2e-desktop.sh"]
fn desktop_runtime_matrix() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    // `cargo test` alone does not refresh executable examples. Never drive stale peers.
    assert!(Command::new("cargo")
        .current_dir(&root)
        .args([
            "build",
            "-p",
            "cleancopy-desktop",
            "--bin",
            "cleancopy-app",
            "--example",
            "target",
            "--example",
            "screenshot",
        ])
        .status()
        .unwrap()
        .success());
    let bin = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .map(|p| if p.is_absolute() { p } else { root.join(p) })
        .unwrap_or_else(|| root.join("target"))
        .join("debug");
    let dir = tempfile::tempdir().unwrap();
    let display_file = dir.path().join("display");
    let xvfb = Command::new("Xvfb")
        .args([
            "-displayfd",
            "1",
            "-screen",
            "0",
            "1280x800x24",
            "-nolisten",
            "tcp",
        ])
        .stdout(fs::File::create(&display_file).unwrap())
        .spawn()
        .unwrap();
    let mut h = Harness {
        children: vec![xvfb],
        dir,
        display: String::new(),
        bin,
        shots: screenshot_dir(&root),
        root,
    };
    wait("Xvfb display", || {
        fs::read_to_string(&display_file).is_ok_and(|s| !s.trim().is_empty())
    });
    h.display = format!(":{}", fs::read_to_string(display_file).unwrap().trim());
    h.spawn(Command::new("xfwm4").env("DISPLAY", &h.display), "wm");
    wait("window manager EWMH readiness", || {
        Command::new("xprop")
            .env("DISPLAY", &h.display)
            .args(["-root", "_NET_SUPPORTING_WM_CHECK"])
            .output()
            .is_ok_and(|output| String::from_utf8_lossy(&output.stdout).contains("window id #"))
    });
    // H1 is an unnumbered document title; H2 starts Chinese section numbering.
    let source = b"# Title\r\n\r\n## Heading\r\n\r\n**bold**\r\n";
    let numbered = "Title\n\n一、Heading\n\nbold".as_bytes();
    h.fixture(source, "fixture");
    let before = h.snapshot();
    let gedit = h.target("gedit");
    let config = h.dir.path().join("config.toml");
    // Onboarding is covered separately below; the matrix daemon is a returning user.
    Config {
        onboarding_done: true,
        ..Config::default()
    }
    .save(&config)
    .unwrap();
    let daemon = h.spawn(
        Command::new(h.bin.join("cleancopy-app"))
            .env("DISPLAY", &h.display)
            .env("LIBGL_ALWAYS_SOFTWARE", "1")
            .arg("--config")
            .arg(&config),
        "daemon",
    );
    wait("real daemon hotkey readiness", || {
        fs::read_to_string(h.dir.path().join("daemon.log"))
            .is_ok_and(|s| s.contains("hotkey ready"))
    });
    // Observe mapping throughout silent conversion, not merely at the end.
    h.key(&["ctrl+shift+v"]);
    wait("gedit silent paste", || {
        assert!(h.menu().is_none(), "silent operation mapped a menu");
        gedit.join("paste-1.bin").exists()
    });
    assert_eq!(h.paste(&gedit, 1), numbered);
    h.restored(&before);
    println!("PASS (a) gedit→numbered plain, no mapped menu");
    let word = h.target("WINWORD.EXE");
    h.key(&["ctrl+shift+v"]);
    let html = String::from_utf8(h.paste(&word, 1)).unwrap();
    assert!(
        // Single leading H1 = document title (Word "Title" style).
        html.contains("class=\"MsoTitle\"")
            && html.contains("<h2 class=\"MsoHeading2\"")
            && html.contains("<strong>bold</strong>"),
        "{html}"
    );
    h.restored(&before);
    println!("PASS (b) Word target receives semantic Word HTML");
    let unknown = h.target("unknown-e2e");
    let ready_count = h.metadata_completions();
    let (id, first_secondary) = h.open_measured(false, true, "secondary", false);
    h.filled(ready_count, "secondary");
    assert_eq!(
        h.xdo(&["getwindowfocus"]),
        format!("{}\n", fs::read_to_string(unknown.join("window")).unwrap()).as_bytes()
    );
    h.xdo(&[
        "mousemove",
        "--window",
        &id.to_string(),
        "380",
        "261",
        "click",
        "1",
    ]);
    thread::sleep(Duration::from_millis(100));
    h.shot("remember-checked");
    h.key(&["2"]);
    assert_eq!(h.paste(&unknown, 1), numbered);
    h.restored(&before);
    wait("remember persisted", || {
        Config::load(&config).unwrap().app_rules.iter().any(|r| {
            r.app.wm_class.as_deref() == Some("unknown-e2e") && r.format == Format::ChineseNumbered
        })
    });
    h.key(&["ctrl+shift+v"]);
    assert_eq!(h.paste(&unknown, 2), numbered);
    assert!(h.menu().is_none());
    h.restored(&before);
    println!("PASS (d) mouse remember checkbox + key2 persisted rule; next paste silent");
    h.open(true, false, "full");
    h.close();
    h.restored(&before);
    println!("PASS (e) native Ctrl+Alt+V full menu (format panel collapsed)");
    // Undo is selected with the actual mouse, then observed as Ctrl+Z + original Ctrl+V.
    let id = h.open(true, false, "undo");
    h.xdo(&[
        "mousemove",
        "--window",
        &id.to_string(),
        "150",
        "166",
        "click",
        "1",
    ]);
    assert_eq!(h.paste(&unknown, 3), source);
    let events = fs::read_to_string(unknown.join("events.tsv")).unwrap();
    assert!(events.contains("\tundo\n"), "{events}");
    let doc = fs::read(unknown.join("document.bin")).unwrap();
    assert_eq!(doc, [numbered, source].concat());
    h.restored(&before);
    println!(
        "PASS (g) actual Ctrl+Z reverted second conversion; original native paste replaced it"
    );
    let mut c = Config::load(&config).unwrap();
    c.silent_mode = false;
    c.save(&config).unwrap();
    // An idle daemon re-reads its config once per second (IDLE_TICK).
    thread::sleep(Duration::from_millis(1300));
    for i in 1..=2 {
        h.open(false, false, &format!("nonsilent-{i}"));
        h.close();
    }
    println!("PASS (f) silent_mode=false maps full menu on each single press");
    c = Config::load(&config).unwrap();
    c.silent_mode = true;
    c.save(&config).unwrap();
    thread::sleep(Duration::from_millis(1300));
    let large = format!("## Heading\n\n{}\n", "x".repeat(9987));
    assert_eq!(large.chars().count(), 10000);
    h.fixture(large.as_bytes(), "large");
    let large_before = h.snapshot();
    h.focus(&gedit);
    let start = Instant::now();
    h.key(&["ctrl+shift+v"]);
    let output = h.paste(&gedit, 2);
    let elapsed = start.elapsed();
    println!(
        "MEASURE 10k-char hotkey→convert+native-paste: {:.2} ms (native target received clipboard bytes)",
        elapsed.as_secs_f64() * 1000.0
    );
    assert!(
        elapsed <= Duration::from_millis(300),
        "10k convert+native paste exceeded 300 ms: {elapsed:?}"
    );
    assert_eq!(
        output,
        format!("一、Heading\n\n{}", "x".repeat(9987)).as_bytes()
    );
    h.restored(&large_before);
    // HTML-only web clipboard: no UTF8_STRING/plain/Markdown representation.
    let html_dir = h.dir.path().join("html-only");
    fs::create_dir(&html_dir).unwrap();
    fs::write(
        html_dir.join("html"),
        "<html><body><h2>Web heading</h2><p><strong>web bold</strong> text</p></body></html>",
    )
    .unwrap();
    fs::write(
        html_dir.join("targets.tsv"),
        "text/html\ttext/html\t8\thtml\n",
    )
    .unwrap();
    h.spawn(
        Command::new(h.bin.join("examples/target"))
            .env("DISPLAY", &h.display)
            .arg("fixture")
            .arg(&html_dir),
        "html-only",
    );
    wait("HTML-only owner ready", || html_dir.join("ready").exists());
    let html_original = h.read("text/html");
    let html_targets = String::from_utf8(h.read("TARGETS").0).unwrap();
    assert!(
        !html_targets.contains("UTF8_STRING"),
        "fixture must be HTML-only: {html_targets}"
    );
    assert!(
        !html_targets.contains("text/plain"),
        "fixture must be HTML-only: {html_targets}"
    );
    h.key(&["ctrl+shift+v"]);
    let web_output = String::from_utf8(h.paste(&gedit, 3)).unwrap();
    assert!(web_output.contains("一、Web heading"), "{web_output}");
    assert!(web_output.contains("web bold text"), "{web_output}");
    wait("HTML restored", || {
        let output = Command::new(h.bin.join("examples/target"))
            .env("DISPLAY", &h.display)
            .args(["read", "text/html"])
            .output()
            .unwrap();
        output.status.success() && output.stdout == html_original.0
    });
    assert_eq!(h.read("text/html"), html_original);
    println!("PASS S-02 real HTML-only X11 clipboard → Markdown → numbered native paste; original HTML restored");

    // Secondary-menu latency distribution: the first press after launch plus
    // 19 warm presses, all on a target with no App rule.
    // (d) remembered a rule for `unknown`; use a second unrecognised target.
    let fresh = h.target("timing-e2e");
    h.fixture(b"## Secondary timing\n\n- item\n", "secondary-timing");
    h.focus(&fresh);
    let mut samples = vec![first_secondary];
    for run in 2..=20 {
        let (_, elapsed) = h.open_measured(false, true, &format!("secondary-run{run}"), false);
        samples.push(elapsed);
        h.close();
    }
    samples.sort();
    let ms = |d: Duration| d.as_secs_f64() * 1000.0;
    println!(
        "MEASURE secondary x{} (first press after launch = {:.2} ms): min {:.2} / median {:.2} / max {:.2} ms",
        samples.len(),
        ms(first_secondary),
        ms(samples[0]),
        ms((samples[9] + samples[10]) / 2),
        ms(samples[samples.len() - 1])
    );
    // N-1: a 3 MB clipboard on an unrecognised target still shows the
    // secondary menu first and fills it in afterwards.
    let mut big = b"## Large secondary\n\n".to_vec();
    big.resize(3 * 1024 * 1024, b'x');
    h.fixture(&big, "secondary-3mb");
    h.focus(&fresh);
    let ready = h.metadata_completions();
    let (_, elapsed) = h.open_measured(false, true, "secondary-3MB", false);
    assert!(
        elapsed <= Duration::from_millis(100),
        "3MB secondary exceeded 100 ms: {elapsed:?}"
    );
    wait("3MB secondary metadata", || {
        h.metadata_completions() > ready
    });
    h.close();

    for mib in [3, 5] {
        let mut source = b"## Large clipboard\n\n".to_vec();
        source.resize(mib * 1024 * 1024, b'x');
        h.fixture(&source, &format!("full-{mib}mb"));
        let (_, elapsed) = h.open_measured(true, false, &format!("{mib}MB"), false);
        assert!(
            elapsed <= Duration::from_millis(100),
            "{mib}MB full menu exceeded 100 ms: {elapsed:?}"
        );
        // Escape during snapshot transfer must invalidate the pending completion.
        h.close();
        thread::sleep(Duration::from_millis(100));
        assert!(h.menu().is_none(), "cancelled metadata reopened the menu");
    }

    // More than 200 KiB takes the worker route; click the real 取消 button.
    // Many independent blocks keep conversion busy long enough to exercise cancellation.
    let cancellable = format!(
        "## Background\n\n{}",
        "**bold** _italic_\n\n".repeat(180_000)
    );
    h.fixture(cancellable.as_bytes(), "background-cancel");
    h.focus(&gedit);
    let background_menu = h.open(true, false, "background-before");
    h.key(&["2"]);
    // Wait for the worker-state footer to paint, rather than merely cancelling
    // the pre-conversion menu. The track replaces the normal footer text.
    thread::sleep(Duration::from_millis(60));
    assert!(
        h.menu().is_some(),
        "large conversion finished before cancellation probe"
    );
    h.shot("background-progress");
    h.xdo(&[
        "mousemove",
        "--window",
        &background_menu.to_string(),
        "308",
        "201",
        "click",
        "1",
    ]);
    wait("background cancellation dismissed menu", || {
        h.menu().is_none()
    });
    thread::sleep(Duration::from_millis(500));
    assert!(
        !gedit.join("paste-4.bin").exists(),
        "cancelled conversion pasted"
    );
    assert_eq!(h.read("UTF8_STRING").0, cancellable.as_bytes());
    println!(
        "PASS S-01 real large conversion cancelled by clicking 取消; no paste and clipboard unchanged"
    );

    // The settings screenshot must show the shipped defaults, not the rules
    // this test remembered for its fake `unknown-e2e` target.
    let clean_config = h.root.join("target/e2e-settings-default.toml");
    let _ = std::fs::remove_file(&clean_config);
    for (name, args) in [
        ("settings", vec!["--settings"]),
        ("preview", vec!["--preview", "menu-secondary"]),
        ("tray", vec!["--preview", "tray"]),
    ] {
        let child = h.spawn(
            Command::new(h.bin.join("cleancopy-app"))
                .env("DISPLAY", &h.display)
                .args(args)
                .arg("--config")
                .arg(if name == "settings" {
                    &clean_config
                } else {
                    &config
                }),
            name,
        );
        wait("settings/preview window", || {
            Command::new("xdotool")
                .env("DISPLAY", &h.display)
                .args(["search", "--onlyvisible", "--name", APP_WINDOW])
                .output()
                .unwrap()
                .status
                .success()
        });
        thread::sleep(Duration::from_millis(300));
        h.shot(name);
        if name == "settings" {
            let saved = std::fs::read_to_string(&clean_config).unwrap_or_default();
            assert!(!saved.contains("unknown-e2e"), "{saved}");
        }
        h.children[child].kill().unwrap();
        h.children[child].wait().unwrap();
        println!("PASS {name} launch probe");
    }

    // First run: a daemon with a fresh config spawns the onboarding window.
    h.children[daemon].kill().unwrap();
    h.children[daemon].wait().unwrap();
    let fresh = h.dir.path().join("fresh.toml");
    let first = h.spawn(
        Command::new(h.bin.join("cleancopy-app"))
            .env("DISPLAY", &h.display)
            .env("LIBGL_ALWAYS_SOFTWARE", "1")
            .arg("--config")
            .arg(&fresh),
        "first-run-daemon",
    );
    let mut window = 0;
    wait("first-run onboarding window", || {
        let out = Command::new("xdotool")
            .env("DISPLAY", &h.display)
            .args(["search", "--onlyvisible", "--name", APP_WINDOW])
            .output()
            .unwrap();
        window = String::from_utf8(out.stdout)
            .unwrap()
            .lines()
            .next()
            .and_then(|l| l.parse().ok())
            .unwrap_or(0);
        window != 0
    });
    // Stop the daemon so its global grab doesn't take the step-3 hotkey; the
    // onboarding window is its own process and stays open.
    h.children[first].kill().unwrap();
    h.children[first].wait().unwrap();
    thread::sleep(Duration::from_millis(500));
    let id = window.to_string();
    let capture = |n: usize| {
        let path = h.shots.join(format!("onboarding-{n}.png"));
        assert!(Command::new(h.bin.join("examples/screenshot"))
            .env("DISPLAY", &h.display)
            .arg(&path)
            .arg(&id)
            .status()
            .unwrap()
            .success());
    };
    let click = |x: &str, y: &str| {
        h.xdo(&["mousemove", "--window", &id, x, y, "click", "1"]);
        thread::sleep(Duration::from_millis(600));
    };
    capture(1);
    click("420", "333"); // 下一步
    capture(2);
    click("420", "333");
    click("320", "200"); // focus the try-it input
                         // "Copy" the sample, then press the real hotkey inside the input.
    let mut xclip = Command::new("xclip")
        .env("DISPLAY", &h.display)
        .args(["-selection", "clipboard"])
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    {
        use std::io::Write;
        xclip
            .stdin
            .take()
            .unwrap()
            .write_all("# 项目周报\n## 本周进展\n".as_bytes())
            .unwrap();
    }
    xclip.wait().unwrap();
    h.xdo(&["windowfocus", "--sync", &id]);
    // Hold the chord across frames like a person does; egui reports paste
    // modifiers per frame, and an instant xdotool chord releases them first.
    h.xdo(&["keydown", "ctrl+shift+v"]);
    thread::sleep(Duration::from_millis(150));
    h.xdo(&["keyup", "ctrl+shift+v"]);
    thread::sleep(Duration::from_millis(600));
    capture(3);
    assert!(!Config::load(&fresh).unwrap().onboarding_done);
    click("420", "333"); // 完成
    wait("onboarding closed", || {
        !Command::new("xdotool")
            .env("DISPLAY", &h.display)
            .args(["search", "--onlyvisible", "--name", APP_WINDOW])
            .output()
            .unwrap()
            .status
            .success()
    });
    assert!(Config::load(&fresh).unwrap().onboarding_done);
    println!("PASS onboarding: first-run daemon spawned it; 3 steps captured; 完成 persisted onboarding_done");
    println!("PASS desktop runtime matrix: real target/debug/cleancopy-app --config; all native peers isolated");
}
