//! Opt-in process-only desktop acceptance harness. Drives the shipped daemon on Xvfb.
#![cfg(target_os = "linux")]
use cleancopy_desktop::config::{Config, Format};
use std::{fs, path::{Path, PathBuf}, process::{Child, Command, Stdio}, thread, time::{Duration, Instant}};
use x11rb::{connection::Connection, protocol::xproto::{ConnectionExt, ImageFormat}};

struct Harness { children: Vec<Child>, dir: tempfile::TempDir, display: String, bin: PathBuf, root: PathBuf }
fn wait(label: &str, mut f: impl FnMut() -> bool) {
    let start = Instant::now();
    while !f() { assert!(start.elapsed() < Duration::from_secs(15), "timeout: {label}"); thread::sleep(Duration::from_millis(5)); }
}
impl Drop for Harness {
    fn drop(&mut self) {
        for child in self.children.iter_mut().rev() { let _ = child.kill(); let _ = child.wait(); }
        for entry in fs::read_dir(self.dir.path()).unwrap().flatten() {
            if entry.path().extension().is_some_and(|s| s == "log") { println!("{}:\n{}", entry.path().display(), fs::read_to_string(entry.path()).unwrap_or_default()); }
        }
    }
}
impl Harness {
    fn spawn(&mut self, cmd: &mut Command, name: &str) -> usize {
        let log = fs::File::create(self.dir.path().join(format!("{name}.log"))).unwrap();
        let child = cmd.stderr(Stdio::from(log.try_clone().unwrap())).stdout(Stdio::from(log)).spawn().unwrap();
        self.children.push(child); self.children.len()-1
    }
    fn xdo(&self, args: &[&str]) -> Vec<u8> {
        let out = Command::new("xdotool").env("DISPLAY", &self.display).args(args).output().unwrap();
        assert!(out.status.success(), "xdotool {args:?}: {out:?}"); out.stdout
    }
    fn key(&self, keys: &[&str]) { let mut args = vec!["key", "--clearmodifiers"]; args.extend(keys); self.xdo(&args); }
    fn menu(&self) -> Option<u32> {
        let out = Command::new("xdotool").env("DISPLAY", &self.display).args(["search", "--onlyvisible", "--name", "^CleanCopy floating menu$"]).output().unwrap();
        String::from_utf8(out.stdout).unwrap().lines().next().and_then(|s| s.parse().ok())
    }
    fn pixel(&self, window: u32, x: i16, y: i16) -> Vec<u8> {
        let (c, _) = x11rb::connect(Some(&self.display)).unwrap();
        c.get_image(ImageFormat::Z_PIXMAP, window, x, y, 1, 1, u32::MAX).unwrap().reply().unwrap().data
    }
    fn open(&self, double: bool, secondary: bool, shot: &str) -> u32 {
        self.xdo(&["mousemove", "350", "200"]);
        let start = Instant::now();
        self.key(if double { &["ctrl+shift+v", "ctrl+shift+v"] } else { &["ctrl+shift+v"] });
        wait("menu mapped", || self.menu().is_some());
        println!("MEASURE hotkey→menu-visible {shot}: {:.2} ms", start.elapsed().as_secs_f64()*1000.0);
        let id = self.menu().unwrap();
        // Real rendered contents: format-panel interior is white only when expanded.
        wait("menu painted with expected contents", || {
            let p = self.pixel(id, 400, 130);
            if secondary { p[..3] == [255,255,255] } else { p[..3] == [242,237,237] }
        });
        self.shot(shot); id
    }
    fn shot(&self, name: &str) {
        let path = self.root.join(format!("docs/screenshots/e2e-{name}.png"));
        assert!(Command::new(self.bin.join("examples/screenshot")).env("DISPLAY", &self.display).arg(&path).status().unwrap().success());
    }
    fn close(&self) { self.key(&["Escape"]); wait("menu dismissed", || self.menu().is_none()); }
    fn read(&self, target: &str) -> (Vec<u8>, Vec<u8>) {
        let out = Command::new(self.bin.join("examples/target")).env("DISPLAY", &self.display).args(["read",target]).output().unwrap();
        assert!(out.status.success(), "read {target}: {out:?}"); (out.stdout, out.stderr)
    }
    fn snapshot(&self) -> Vec<(String, Vec<u8>, Vec<u8>)> {
        let (targets, _) = self.read("TARGETS");
        let mut names: Vec<_> = String::from_utf8(targets).unwrap().lines().map(str::to_owned).collect(); names.sort();
        names.into_iter().map(|name| {let (bytes,meta)=self.read(&name); (name,bytes,meta)}).collect()
    }
    fn restored(&self, before: &[(String,Vec<u8>,Vec<u8>)]) {
        wait("original text restored", || self.read("UTF8_STRING").0 == before.iter().find(|t|t.0=="UTF8_STRING").unwrap().1);
        assert_eq!(self.snapshot(), before, "all advertised targets, bytes and native metadata must restore exactly");
        println!("RESTORE all {} advertised targets: raw bytes + property type/format exact", before.len());
    }
    fn fixture(&mut self, source: &[u8], name: &str) {
        let dir=self.dir.path().join(name); fs::create_dir(&dir).unwrap();
        fs::write(dir.join("text"),source).unwrap();
        fs::write(dir.join("opaque"),[0,255,128,13,0,7]).unwrap();
        fs::write(dir.join("words"),[0u16,0x1234,0xffff].into_iter().flat_map(u16::to_ne_bytes).collect::<Vec<_>>()).unwrap();
        fs::write(dir.join("longs"),[0u32,0x12345678,u32::MAX].into_iter().flat_map(u32::to_ne_bytes).collect::<Vec<_>>()).unwrap();
        fs::write(dir.join("targets.tsv"),"UTF8_STRING\tUTF8_STRING\t8\ttext\napplication/x-opaque\tapplication/x-bytes\t8\topaque\napplication/x-words\tINTEGER\t16\twords\napplication/x-longs\tCARDINAL\t32\tlongs\n").unwrap();
        self.spawn(Command::new(self.bin.join("examples/target")).env("DISPLAY",&self.display).arg("fixture").arg(&dir),name);
        wait("fixture ready",||dir.join("ready").exists());
    }
    fn target(&mut self, class: &str) -> PathBuf {
        let dir=self.dir.path().join(class);
        self.spawn(Command::new(self.bin.join("examples/target")).env("DISPLAY",&self.display).args(["app",class]).arg(&dir),class);
        wait("target mapped",||dir.join("ready").exists()); self.focus(&dir); dir
    }
    fn focus(&self, dir: &Path) { let id=fs::read_to_string(dir.join("window")).unwrap(); self.xdo(&["windowactivate","--sync",id.trim()]); }
    fn paste(&self, dir: &Path, n: usize) -> Vec<u8> {
        let path=dir.join(format!("paste-{n}.bin")); wait("native target paste",||path.exists()); fs::read(path).unwrap()
    }
}

#[test]
#[ignore = "native app process matrix; run scripts/e2e-desktop.sh"]
fn desktop_runtime_matrix() {
    let root=PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap();
    let bin=std::env::var_os("CARGO_TARGET_DIR").map(PathBuf::from).map(|p|if p.is_absolute(){p}else{root.join(p)}).unwrap_or_else(||root.join("target")).join("debug");
    let dir=tempfile::tempdir().unwrap();
    let display_file=dir.path().join("display");
    let xvfb=Command::new("Xvfb").args(["-displayfd","1","-screen","0","1280x900x24","-nolisten","tcp"]).stdout(fs::File::create(&display_file).unwrap()).spawn().unwrap();
    let mut h=Harness{children:vec![xvfb],dir,display:String::new(),bin,root};
    wait("Xvfb display",||fs::read_to_string(&display_file).is_ok_and(|s|!s.trim().is_empty()));
    h.display=format!(":{}",fs::read_to_string(display_file).unwrap().trim());
    h.spawn(Command::new("xfwm4").env("DISPLAY",&h.display),"wm"); thread::sleep(Duration::from_millis(500));
    let source=b"# Heading\r\n\r\n**bold**\r\n";
    let numbered="一、Heading\n\nbold\n".as_bytes();
    h.fixture(source,"fixture"); let before=h.snapshot();
    let gedit=h.target("gedit");
    let config=h.dir.path().join("config.toml"); Config::default().save(&config).unwrap();
    h.spawn(Command::new(h.bin.join("cleancopy-app")).env("DISPLAY",&h.display).env("LIBGL_ALWAYS_SOFTWARE","1").arg("--config").arg(&config),"daemon");
    wait("real daemon hotkey readiness",||fs::read_to_string(h.dir.path().join("daemon.log")).is_ok_and(|s|s.contains("hotkey ready")));
    // Observe mapping throughout silent conversion, not merely at the end.
    h.key(&["ctrl+shift+v"]);
    wait("gedit silent paste",||{ assert!(h.menu().is_none(),"silent operation mapped a menu"); gedit.join("paste-1.bin").exists() });
    assert_eq!(h.paste(&gedit,1),numbered); h.restored(&before); println!("PASS (a) gedit→numbered plain, no mapped menu");
    let word=h.target("WINWORD.EXE"); h.key(&["ctrl+shift+v"]);
    let html=String::from_utf8(h.paste(&word,1)).unwrap(); assert!(html.contains("<h1 ") && html.contains("MsoHeading1") && html.contains("<strong>bold</strong>"),"{html}");
    h.restored(&before); println!("PASS (b) Word target receives semantic Word HTML");
    let unknown=h.target("unknown-e2e");
    let id=h.open(false,true,"secondary");
    assert_eq!(h.xdo(&["getwindowfocus"]),format!("{}\n",fs::read_to_string(unknown.join("window")).unwrap()).as_bytes());
    h.xdo(&["mousemove","--window",&id.to_string(),"380","261","click","1"]);
    thread::sleep(Duration::from_millis(100)); h.shot("remember-checked"); h.key(&["2"]);
    assert_eq!(h.paste(&unknown,1),numbered); h.restored(&before);
    wait("remember persisted",||Config::load(&config).unwrap().app_rules.iter().any(|r|r.app.wm_class.as_deref()==Some("unknown-e2e") && r.format==Format::ChineseNumbered));
    h.key(&["ctrl+shift+v"]); assert_eq!(h.paste(&unknown,2),numbered); assert!(h.menu().is_none()); h.restored(&before);
    println!("PASS (d) mouse remember checkbox + key2 persisted rule; next paste silent");
    h.open(true,false,"full"); h.close(); h.restored(&before); println!("PASS (e) double press full menu (format panel collapsed)");
    // Undo is selected with the actual mouse, then observed as Ctrl+Z + original Ctrl+V.
    let id=h.open(true,false,"undo");
    h.xdo(&["mousemove","--window",&id.to_string(),"150","166","click","1"]);
    assert_eq!(h.paste(&unknown,3),source);
    let events=fs::read_to_string(unknown.join("events.tsv")).unwrap(); assert!(events.contains("\tundo\n"),"{events}");
    let doc=fs::read(unknown.join("document.bin")).unwrap(); assert_eq!(doc,[numbered,source].concat()); h.restored(&before);
    println!("PASS (g) actual Ctrl+Z reverted second conversion; original native paste replaced it");
    let mut c=Config::load(&config).unwrap(); c.silent_mode=false; c.save(&config).unwrap(); thread::sleep(Duration::from_millis(600));
    for i in 1..=2 { h.open(false,false,&format!("nonsilent-{i}")); h.close(); }
    println!("PASS (f) silent_mode=false maps full menu on each single press");
    c=Config::load(&config).unwrap(); c.silent_mode=true; c.save(&config).unwrap(); thread::sleep(Duration::from_millis(600));
    let large=format!("# Heading\n\n{}\n", "x".repeat(9988)); assert_eq!(large.chars().count(),10000);
    h.fixture(large.as_bytes(),"large"); let large_before=h.snapshot(); h.focus(&gedit);
    let start=Instant::now(); h.key(&["ctrl+shift+v"]); let output=h.paste(&gedit,2);
    println!("MEASURE 10k-char hotkey→convert+native-paste: {:.2} ms (includes single-press arbitration)",start.elapsed().as_secs_f64()*1000.0);
    assert_eq!(output,format!("一、Heading\n\n{}\n","x".repeat(9988)).as_bytes()); h.restored(&large_before);
    for (name,args) in [("settings",vec!["--settings"]),("preview",vec!["--preview","menu-secondary"])] {
        let child=h.spawn(Command::new(h.bin.join("cleancopy-app")).env("DISPLAY",&h.display).args(args).arg("--config").arg(&config),name);
        wait("settings/preview window",||Command::new("xdotool").env("DISPLAY",&h.display).args(["search","--onlyvisible","--name","^CleanCopy$"]).output().unwrap().status.success());
        thread::sleep(Duration::from_millis(300)); h.shot(name); h.children[child].kill().unwrap(); h.children[child].wait().unwrap();
        println!("PASS {name} launch probe");
    }
    println!("PASS desktop runtime matrix: real target/debug/cleancopy-app --config; all native peers isolated");
}
