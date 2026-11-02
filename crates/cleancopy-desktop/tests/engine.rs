use cleancopy_desktop::{
    config::{AppIdentity, Config, Format},
    engine::*,
};

#[derive(Default)]
struct Mock {
    app: AppIdentity,
    clipboard: Snapshot,
    writes: Vec<Snapshot>,
    menus: Vec<MenuModel>,
    pastes: usize,
    undos: usize,
    hides: usize,
    notices: Vec<String>,
    fail: Option<&'static str>,
    focus_after_write: Option<AppIdentity>,
    secure_after_write: Option<std::rc::Rc<std::cell::Cell<bool>>>,
    now: u64,
    pasted: Vec<Snapshot>,
    effects: Vec<&'static str>,
    /// Background clipboard reads: None = unsupported (synchronous reads).
    async_read: Option<AsyncRead>,
}
#[derive(Default)]
struct AsyncRead {
    started: usize,
    in_flight: bool,
    ready: bool,
}
impl Mock {
    fn check(&mut self, effect: &'static str) -> Result<(), &'static str> {
        self.effects.push(effect);
        if self.fail == Some(effect) {
            self.fail = None;
            Err(effect)
        } else {
            Ok(())
        }
    }
}
impl Platform for Mock {
    type Error = &'static str;
    fn now(&self) -> u64 {
        self.now
    }
    fn frontmost_app(&mut self) -> Result<AppIdentity, Self::Error> {
        self.check("front")?;
        Ok(self.app.clone())
    }
    fn read_clipboard(&mut self) -> Result<Snapshot, Self::Error> {
        self.check("read")?;
        Ok(self.clipboard.clone())
    }
    fn write_clipboard(&mut self, s: Snapshot) -> Result<(), Self::Error> {
        self.check("write")?;
        self.writes.push(s.clone());
        self.clipboard = s;
        if let Some(state) = self.secure_after_write.take() {
            state.set(true);
        }
        if let Some(app) = self.focus_after_write.take() {
            self.app = app;
        }
        Ok(())
    }
    fn send_paste(&mut self) -> Result<(), Self::Error> {
        self.check("paste")?;
        self.pastes += 1;
        self.pasted.push(self.clipboard.clone());
        Ok(())
    }
    fn send_undo(&mut self) -> Result<(), Self::Error> {
        self.check("undo")?;
        self.undos += 1;
        Ok(())
    }
    fn show_menu(&mut self, m: MenuModel) -> Result<(), Self::Error> {
        self.check("show")?;
        self.menus.push(m);
        Ok(())
    }
    fn hide_menu(&mut self) -> Result<(), Self::Error> {
        self.check("hide")?;
        self.hides += 1;
        Ok(())
    }
    fn notify(&mut self, s: &str) -> Result<(), Self::Error> {
        self.check("notify")?;
        self.notices.push(s.into());
        Ok(())
    }
    fn begin_clipboard_read(&mut self) -> bool {
        let Some(read) = self.async_read.as_mut() else {
            return false;
        };
        read.started += 1;
        read.in_flight = true;
        true
    }
    fn poll_clipboard_read(&mut self) -> Option<Result<Snapshot, Self::Error>> {
        let read = self.async_read.as_mut()?;
        if !(read.in_flight && read.ready) {
            return None;
        }
        read.in_flight = false;
        Some(Ok(self.clipboard.clone()))
    }
}
fn app(exe: &str) -> AppIdentity {
    AppIdentity {
        name: "identical display name".into(),
        exe: Some(exe.into()),
        ..Default::default()
    }
}
fn engine(exe: &str, text: &str) -> Engine<Mock> {
    Engine::new(
        Config::default(),
        Mock {
            app: app(exe),
            clipboard: Snapshot::text(text),
            ..Default::default()
        },
    )
}
fn hot(e: &mut Engine<Mock>, t: u64) {
    e.handle(Event::HotkeyPressed { t }).unwrap();
}
fn full(e: &mut Engine<Mock>, t: u64) {
    e.handle(Event::FullMenuHotkeyPressed { t }).unwrap();
}
fn timer(e: &mut Engine<Mock>, t: u64) {
    e.handle(Event::Timer { t }).unwrap();
}
fn choose(e: &mut Engine<Mock>, t: u64, choice: Choice) {
    let id = e.platform().menus.last().unwrap().id;
    e.handle(Event::MenuChoice { t, id, choice }).unwrap();
}

#[test]
fn recognized_silent_word_restores_every_original_byte_after_150ms() {
    let mut e = engine("WINWORD.EXE", "# Title\n\n**bold**");
    e.platform_mut()
        .clipboard
        .formats
        .insert("native/opaque".into(), vec![0, 255, 128, 13]);
    e.platform_mut()
        .clipboard
        .formats
        .insert(TEXT_HTML.into(), vec![255, 0, 1]);
    e.platform_mut()
        .clipboard
        .formats
        .insert(TEXT_RTF.into(), b"original rtf\0".to_vec());
    e.platform_mut()
        .clipboard
        .formats
        .insert(TEXT_MARKDOWN.into(), b"# Title\r\n\r\n**bold**\n".to_vec());
    let original = e.platform().clipboard.clone();
    hot(&mut e, 1000);
    assert!(e.platform().menus.is_empty());
    assert_eq!(e.platform().pastes, 1);
    assert_eq!(e.platform().clipboard.formats.len(), 3);
    assert!(e.platform().clipboard.formats.contains_key(TEXT_PLAIN));
    assert!(
        String::from_utf8_lossy(&e.platform().clipboard.formats[TEXT_HTML]).contains("MsoHeading1")
    );
    assert!(e.platform().clipboard.formats[TEXT_RTF].starts_with(b"{\\rtf1"));
    assert_eq!(e.next_deadline(), Some(1150));
    timer(&mut e, 1149);
    assert_ne!(e.platform().clipboard, original);
    timer(&mut e, 1150);
    assert_eq!(e.platform().clipboard, original);
    assert_eq!(e.next_deadline(), None);
    assert_eq!(e.config().recent_pastes[0].timestamp, 1000);
}

#[test]
fn unknown_secondary_remembers_and_honors_configured_items() {
    let mut e = engine("unknown", "## Heading");
    hot(&mut e, 0);
    assert_eq!(e.platform().menus.len(), 1);
    assert_eq!(e.platform().pastes, 0);
    assert!(e.platform().writes.is_empty());
    assert_eq!(e.next_deadline(), None);
    let menu = e.platform().menus.last().unwrap().clone();
    assert_eq!(menu.kind, MenuKind::Secondary);
    assert!(menu.show_remember);
    assert!(!menu.formats.iter().any(|i| i.format == Format::HtmlSource));
    e.handle(Event::Remember {
        t: 401,
        id: menu.id,
        checked: true,
    })
    .unwrap();
    choose(&mut e, 402, Choice::Format(Format::ChineseNumbered));
    assert_eq!(
        e.config().format_for(&app("unknown")),
        Some(Format::ChineseNumbered)
    );
    timer(&mut e, 552);
    hot(&mut e, 600);
    assert_eq!(e.platform().pastes, 2);
    assert_eq!(e.platform().menus.len(), 2); // show + checkbox refresh only
}

#[test]
fn explicit_full_hotkey_immediately_opens_full_without_pasting() {
    for exe in ["WINWORD.EXE", "unknown"] {
        let mut e = engine(exe, "# heading");
        let original = e.platform().clipboard.clone();
        full(&mut e, 100);
        assert_eq!(e.platform().menus.len(), 1);
        let menu = &e.platform().menus[0];
        assert_eq!(menu.kind, MenuKind::Full);
        assert_eq!(menu.formats.len(), 4);
        assert_eq!(menu.content_label, "Markdown");
        assert!(menu.original_enabled);
        if exe == "WINWORD.EXE" {
            assert_eq!(menu.default_choice, Choice::Format(Format::WordRichText));
        }
        assert_eq!(e.platform().clipboard, original);
        assert_eq!(e.platform().pastes, 0);
        assert!(e.platform().writes.is_empty());
        assert_eq!(e.next_deadline(), None);
        hot(&mut e, 101);
        full(&mut e, 102);
        assert_eq!(e.platform().menus.len(), 1);
        timer(&mut e, 500);
        assert_eq!(e.platform().pastes, 0);
        choose(&mut e, 501, Choice::Original);
        assert_eq!(e.platform().pastes, 1);
        assert!(e.platform().writes.is_empty());
        assert_eq!(e.next_deadline(), None);
    }
}

#[test]
fn silent_false_primary_immediately_opens_full_and_busy_hotkeys_are_ignored() {
    let config = Config {
        silent_mode: false,
        ..Config::default()
    };
    let mut e = Engine::new(
        config,
        Mock {
            app: app("unknown"),
            clipboard: Snapshot::text("# heading"),
            ..Default::default()
        },
    );
    hot(&mut e, 0);
    assert_eq!(e.platform().menus.len(), 1);
    assert_eq!(e.platform().menus[0].kind, MenuKind::Full);
    assert_eq!(e.next_deadline(), None);
    hot(&mut e, 100);
    full(&mut e, 101);
    assert_eq!(e.platform().menus.len(), 1);
    assert_eq!(e.platform().menus[0].kind, MenuKind::Full);
    assert!(!e.platform().menus[0]
        .formats
        .iter()
        .any(|i| i.format == Format::HtmlSource));
    assert_eq!(e.platform().pastes, 0);
    choose(&mut e, 401, Choice::Format(Format::PlainStripped));
    assert_eq!(e.platform().pastes, 1);
}

#[test]
fn markdown_bytes_not_canonicalized() {
    let source = "# heading\r\n\r\n*em*   \r\n";
    let mut e = engine("notion", source);
    hot(&mut e, 0);
    assert_eq!(e.platform().pastes, 1);
    assert!(e.platform().menus.is_empty());
    assert_eq!(e.next_deadline(), Some(150));
    assert_eq!(
        e.platform().clipboard.formats[TEXT_PLAIN],
        source.as_bytes()
    );
    assert_eq!(
        e.platform().clipboard.formats[TEXT_MARKDOWN],
        source.as_bytes()
    );
}

#[test]
fn plain_defaults_raw_but_menu_conversion_is_selectable_and_muted() {
    let mut e = engine("WINWORD.EXE", "ordinary text");
    hot(&mut e, 0);
    assert_eq!(e.platform().pastes, 1);
    assert!(e.platform().writes.is_empty());
    assert!(e.platform().menus.is_empty());
    assert_eq!(e.next_deadline(), None);
    full(&mut e, 500);
    assert_eq!(e.platform().menus[0].kind, MenuKind::Full);
    assert!(e
        .platform()
        .menus
        .last()
        .unwrap()
        .formats
        .iter()
        .all(|i| i.enabled && i.muted));
    choose(&mut e, 601, Choice::Format(Format::WordRichText));
    assert!(e.platform().clipboard.formats.contains_key(TEXT_RTF));
}

#[test]
fn huge_and_images_raw_only_even_with_recognized_app() {
    for snapshot in [
        Snapshot::text(format!("# {}", "x".repeat(MAX_TEXT_BYTES))),
        Snapshot {
            formats: [("image/png".into(), vec![137, 80, 78, 71, 255])].into(),
        },
    ] {
        let mut e = engine("WINWORD.EXE", "");
        e.platform_mut().clipboard = snapshot.clone();
        hot(&mut e, 0);
        assert_eq!(e.platform().pastes, 1);
        assert!(e.platform().menus.is_empty());
        assert_eq!(e.next_deadline(), None);
        assert_eq!(e.platform().clipboard, snapshot);
        assert!(e.platform().writes.is_empty());
        full(&mut e, 500);
        assert!(e
            .platform()
            .menus
            .last()
            .unwrap()
            .formats
            .iter()
            .all(|i| !i.enabled && i.muted));
        choose(&mut e, 601, Choice::Format(Format::WordRichText));
        assert_eq!(e.platform().pastes, 1);
        choose(&mut e, 602, Choice::Original);
        assert_eq!(e.platform().pastes, 2);
    }
}

#[test]
fn undo_is_once_same_target_and_restores_current_not_old_clipboard() {
    let mut e = engine("WINWORD.EXE", "# original");
    let original = e.platform().clipboard.clone();
    hot(&mut e, 0);
    timer(&mut e, 150);
    let current = Snapshot {
        formats: [("opaque/current".into(), vec![255, 0, 42])].into(),
    };
    e.platform_mut().clipboard = current.clone();
    e.platform_mut().app = app("other");
    e.handle(Event::Undo { t: 600 }).unwrap();
    assert_eq!(e.platform().undos, 0);
    e.platform_mut().app = app("WINWORD.EXE");
    full(&mut e, 700);
    assert!(e.platform().menus.last().unwrap().undo_enabled);
    choose(&mut e, 701, Choice::Undo);
    assert_eq!(e.platform().undos, 1);
    assert_eq!(e.platform().pastes, 2);
    assert_eq!(e.platform().clipboard, original);
    timer(&mut e, 851);
    assert_eq!(e.platform().clipboard, current);
    e.handle(Event::Undo { t: 900 }).unwrap();
    assert_eq!(e.platform().undos, 1);
}

#[test]
fn pause_cancel_stale_menu_and_late_timers_are_harmless() {
    let mut e = engine("WINWORD.EXE", "# heading");
    let original = e.platform().clipboard.clone();
    full(&mut e, 0);
    assert_eq!(e.platform().menus[0].kind, MenuKind::Full);
    e.handle(Event::Cancel { t: 1 }).unwrap();
    timer(&mut e, 1000);
    assert_eq!(e.platform().pastes, 0);
    full(&mut e, 1100);
    let id = e.platform().menus.last().unwrap().id;
    e.handle(Event::Pause {
        t: 1201,
        until: Some(2000),
    })
    .unwrap();
    e.handle(Event::MenuChoice {
        t: 1202,
        id,
        choice: Choice::Original,
    })
    .unwrap();
    hot(&mut e, 1500);
    full(&mut e, 1501);
    timer(&mut e, 1900);
    assert_eq!(e.platform().pastes, 0);
    assert_eq!(e.platform().menus.len(), 2);
    assert_eq!(e.next_deadline(), None);
    hot(&mut e, 2000);
    assert_eq!(e.platform().pastes, 1);
    assert_eq!(e.next_deadline(), Some(2150));
    e.handle(Event::Pause {
        t: 2001,
        until: None,
    })
    .unwrap();
    hot(&mut e, 2002);
    full(&mut e, 2003);
    timer(&mut e, 2149);
    assert_ne!(e.platform().clipboard, original);
    timer(&mut e, 2150); // Pausing cannot abandon restoration.
    assert_eq!(e.platform().clipboard, original);
    assert_eq!(e.platform().pastes, 1);
    assert_eq!(e.next_deadline(), None);
    hot(&mut e, 3000);
    full(&mut e, 3001);
    assert_eq!(e.platform().menus.len(), 2);
    e.handle(Event::Resume { t: 3002 }).unwrap();
    hot(&mut e, 3003);
    assert_eq!(e.platform().pastes, 2);
    timer(&mut e, 100); // Stale timers cannot restore early.
    assert_eq!(e.next_deadline(), Some(3153));
}

#[test]
fn menu_capture_checks_foreground_and_clipboard_before_paste() {
    for change_focus in [true, false] {
        let mut e = engine("WINWORD.EXE", "# first");
        full(&mut e, 0);
        assert_eq!(e.platform().pastes, 0);
        assert_eq!(e.next_deadline(), None);
        if change_focus {
            e.platform_mut().app = app("other");
        } else {
            e.platform_mut().clipboard = Snapshot::text("# newer");
        }
        choose(&mut e, 1, Choice::Format(Format::WordRichText));
        assert_eq!(e.platform().pastes, 0);
        assert!(e.platform().writes.is_empty());
        assert_eq!(e.platform().notices.len(), 1);
    }
    let mut e = engine("WINWORD.EXE", "# first");
    full(&mut e, 0);
    e.platform_mut().app.name = "renamed display".into();
    choose(&mut e, 1, Choice::Format(Format::WordRichText));
    assert_eq!(e.platform().pastes, 1);
    let mut e = engine("", "# first");
    e.platform_mut().app.exe = None;
    full(&mut e, 0);
    choose(&mut e, 2, Choice::Original);
    assert_eq!(e.platform().pastes, 0);
}

#[test]
fn no_overlapping_transaction_and_external_change_not_overwritten() {
    let mut e = engine("WINWORD.EXE", "# first");
    hot(&mut e, 0);
    assert_eq!(e.platform().pastes, 1);
    assert_eq!(e.next_deadline(), Some(150));
    hot(&mut e, 1);
    full(&mut e, 2);
    assert!(e.platform().menus.is_empty());
    let newest = Snapshot::text("new external copy");
    e.platform_mut().clipboard = newest.clone();
    timer(&mut e, 150);
    timer(&mut e, 10000);
    assert_eq!(e.platform().clipboard, newest);
    assert_eq!(e.platform().pastes, 1);
    assert_eq!(e.platform().writes.len(), 1);
    assert_eq!(e.platform().notices.len(), 1);
}

#[test]
fn restoration_errors_retain_state_retry_and_block_hotkeys() {
    for effect in ["read", "write", "notify"] {
        let mut e = engine("WINWORD.EXE", "# original");
        let original = e.platform().clipboard.clone();
        hot(&mut e, 0);
        if effect == "notify" {
            e.platform_mut().clipboard = Snapshot::text("external");
        }
        e.platform_mut().fail = Some(effect);
        assert_eq!(e.handle(Event::Timer { t: 150 }), Err(effect));
        assert_eq!(e.next_deadline(), Some(300));
        hot(&mut e, 151);
        full(&mut e, 152);
        assert!(e.platform().menus.is_empty());
        timer(&mut e, 300);
        assert_eq!(e.next_deadline(), None);
        if effect != "notify" {
            assert_eq!(e.platform().clipboard, original);
        } else {
            assert_eq!(e.platform().clipboard, Snapshot::text("external"));
        }
        assert_eq!(e.platform().pastes, 1);
    }
}

#[test]
fn fallible_effects_do_not_panic_or_lose_required_restoration() {
    for effect in ["front", "read"] {
        let mut e = engine("WINWORD.EXE", "# title");
        e.platform_mut().fail = Some(effect);
        assert_eq!(e.handle(Event::HotkeyPressed { t: 0 }), Err(effect));
        assert_eq!(e.next_deadline(), None);
        assert_eq!(e.platform().pastes, 0);
    }
    for effect in ["write", "paste"] {
        let mut e = engine("WINWORD.EXE", "# title");
        let original = e.platform().clipboard.clone();
        e.platform_mut().fail = Some(effect);
        assert_eq!(e.handle(Event::HotkeyPressed { t: 0 }), Err(effect));
        timer(&mut e, 150);
        assert_eq!(e.platform().clipboard, original);
        assert_eq!(e.platform().pastes, 0);
    }
    let mut e = engine("unknown", "# title");
    e.platform_mut().fail = Some("show");
    assert_eq!(e.handle(Event::HotkeyPressed { t: 0 }), Err("show"));
    assert_eq!(e.next_deadline(), None);
    full(&mut e, 500);
    e.platform_mut().fail = Some("hide");
    let id = e.platform().menus.last().unwrap().id;
    assert_eq!(
        e.handle(Event::MenuChoice {
            t: 502,
            id,
            choice: Choice::Original
        }),
        Err("hide")
    );
    choose(&mut e, 503, Choice::Original);
    assert_eq!(e.platform().pastes, 1);
}

#[test]
fn focus_change_during_write_cancels_paste_but_still_restores() {
    let mut e = engine("WINWORD.EXE", "# original");
    let original = e.platform().clipboard.clone();
    e.platform_mut().focus_after_write = Some(app("other"));
    hot(&mut e, 0);
    assert_eq!(e.platform().pastes, 0);
    assert_eq!(e.next_deadline(), Some(150));
    assert_eq!(e.next_deadline(), Some(150));
    timer(&mut e, 149);
    assert_ne!(e.platform().clipboard, original);
    timer(&mut e, 150);
    assert_eq!(e.platform().clipboard, original);
    assert_eq!(e.next_deadline(), None);
}

#[test]
fn cancellation_hide_failure_invalidates_menu_callbacks() {
    let mut e = engine("unknown", "# original");
    full(&mut e, 0);
    let id = e.platform().menus.last().unwrap().id;
    e.platform_mut().fail = Some("hide");
    assert_eq!(e.handle(Event::Cancel { t: 2 }), Err("hide"));
    e.handle(Event::MenuChoice {
        t: 3,
        id,
        choice: Choice::Original,
    })
    .unwrap();
    assert_eq!(e.platform().pastes, 0);
}

#[test]
fn undo_failures_are_once_only_and_restore_when_written() {
    for effect in ["undo", "write", "paste"] {
        let mut e = engine("WINWORD.EXE", "# original");
        hot(&mut e, 0);
        timer(&mut e, 150);
        let current = Snapshot::text("new user's clipboard");
        e.platform_mut().clipboard = current.clone();
        e.platform_mut().fail = Some(effect);
        assert_eq!(e.handle(Event::Undo { t: 600 }), Err(effect));
        timer(&mut e, 750);
        assert_eq!(e.platform().clipboard, current);
        let undos = e.platform().undos;
        e.handle(Event::Undo { t: 800 }).unwrap();
        assert_eq!(e.platform().undos, undos);
    }
}

#[test]
fn menu_order_shortcuts_defaults_and_content_labels() {
    use cleancopy_desktop::config::SecondaryItem;
    for silent_mode in [false, true] {
        let config = Config {
            silent_mode,
            secondary_items: vec![
                SecondaryItem {
                    format: Format::HtmlSource,
                    enabled: true,
                },
                SecondaryItem {
                    format: Format::WordRichText,
                    enabled: false,
                },
                SecondaryItem {
                    format: Format::Markdown,
                    enabled: true,
                },
            ],
            ..Config::default()
        };
        let mut e = Engine::new(
            config,
            Mock {
                app: app("unknown"),
                clipboard: Snapshot::text("# heading"),
                ..Default::default()
            },
        );
        hot(&mut e, 0);
        let menu = e.platform().menus.last().unwrap();
        assert_eq!(
            menu.formats
                .iter()
                .map(|item| (item.format, item.shortcut))
                .collect::<Vec<_>>(),
            vec![(Format::HtmlSource, 2), (Format::Markdown, 3)]
        );
        assert_eq!(menu.default_choice, Choice::Format(Format::HtmlSource));
        assert_eq!(menu.content_label, "Markdown");
    }
    for (snapshot, label) in [
        (Snapshot::text("ordinary"), "普通文本"),
        (
            Snapshot::text("x".repeat(MAX_TEXT_BYTES + 1)),
            "文本过大，只能粘贴原文",
        ),
        (
            Snapshot {
                formats: [("image/png".into(), vec![255])].into(),
            },
            "图片，不支持转换",
        ),
    ] {
        let mut e = engine("WINWORD.EXE", "");
        e.platform_mut().clipboard = snapshot;
        full(&mut e, 0);
        let menu = e.platform().menus.last().unwrap();
        assert_eq!(menu.default_choice, Choice::Original);
        assert_eq!(menu.content_label, label);
    }
}

#[test]
fn primary_repeats_do_not_arbitrate_or_open_full_and_explicit_full_respects_restoration() {
    let mut e = engine("WINWORD.EXE", "# title");
    hot(&mut e, 0);
    assert_eq!(e.platform().pastes, 1);
    assert!(e.platform().menus.is_empty());
    assert_eq!(e.next_deadline(), Some(150));
    hot(&mut e, 1);
    full(&mut e, 2);
    assert_eq!(e.platform().pastes, 1);
    assert!(e.platform().menus.is_empty());
    timer(&mut e, 150);
    full(&mut e, 151);
    assert_eq!(e.platform().menus.len(), 1);
    assert_eq!(e.platform().menus[0].kind, MenuKind::Full);
    assert_eq!(e.platform().pastes, 1);
    assert_eq!(e.next_deadline(), None);
    key(&mut e, 152, MenuKey::Escape);
    hot(&mut e, 153);
    assert_eq!(e.platform().pastes, 2);
    assert_eq!(e.platform().menus.len(), 1);
    assert_eq!(e.next_deadline(), Some(303));
}

#[test]
fn each_selectable_format_works_and_invalid_utf8_is_raw() {
    for format in [
        Format::ChineseNumbered,
        Format::PlainStripped,
        Format::Markdown,
        Format::WordRichText,
        Format::HtmlSource,
    ] {
        let config = Config {
            secondary_items: [
                Format::ChineseNumbered,
                Format::WordRichText,
                Format::Markdown,
                Format::PlainStripped,
                Format::HtmlSource,
            ]
            .into_iter()
            .map(|format| cleancopy_desktop::config::SecondaryItem {
                format,
                enabled: true,
            })
            .collect(),
            ..Config::default()
        };
        let mut e = Engine::new(
            config,
            Mock {
                app: app("unknown"),
                clipboard: Snapshot::text("## heading\n\n**bold**"),
                ..Default::default()
            },
        );
        full(&mut e, 0);
        choose(&mut e, 2, Choice::Format(format));
        assert_eq!(e.platform().pastes, 1);
        assert_eq!(e.config().recent_pastes[0].format, Some(format));
    }
    let mut e = engine("WINWORD.EXE", "");
    e.platform_mut()
        .clipboard
        .formats
        .insert(TEXT_PLAIN.into(), vec![255, 0]);
    hot(&mut e, 0);
    assert_eq!(e.platform().pastes, 1);
    assert!(e.platform().menus.is_empty());
    assert!(e.platform().writes.is_empty());
    assert_eq!(e.next_deadline(), None);
}

fn opaque_original(text: &str) -> Snapshot {
    let mut snapshot = Snapshot::text(text);
    snapshot
        .formats
        .insert("native/private".into(), vec![0, 255, 128, 13, 10, 0]);
    snapshot
        .formats
        .insert(TEXT_MARKDOWN.into(), text.as_bytes().to_vec());
    snapshot
        .formats
        .insert(TEXT_HTML.into(), b"original html\0\r\n".to_vec());
    snapshot
        .formats
        .insert(TEXT_RTF.into(), b"original rtf\0".to_vec());
    snapshot
}

fn key(e: &mut Engine<Mock>, t: u64, key: MenuKey) {
    let id = e.platform().menus.last().unwrap().id;
    e.handle(Event::MenuKey { t, id, key }).unwrap();
}

#[test]
fn silent_numbered_rule_renders_expected_chinese_and_restores_all_formats() {
    let mut e = engine("notepad.exe", "");
    let original = opaque_original("# 项目周报\r\n\r\n## 本周进展\r\n- **完成**\r\n");
    e.platform_mut().clipboard = original.clone();
    hot(&mut e, 0);
    assert!(e.platform().menus.is_empty());
    assert_eq!(
        e.platform().pasted,
        vec![Snapshot::text("一、项目周报\n\n（一）本周进展\n1. 完成")]
    );
    assert_eq!(e.next_deadline(), Some(150));
    timer(&mut e, 149);
    assert_ne!(e.platform().clipboard, original);
    timer(&mut e, 150);
    assert_eq!(e.platform().clipboard, original);
    assert_eq!(e.next_deadline(), None);
}

#[test]
fn silent_markdown_rule_preserves_source_and_restores_opaque_original() {
    let source = "# Title\r\n\r\n*emphasis*  \r\n";
    let mut e = engine("Obsidian.exe", "");
    let original = opaque_original(source);
    e.platform_mut().clipboard = original.clone();
    hot(&mut e, 0);
    assert!(e.platform().menus.is_empty());
    assert_eq!(
        e.platform().pasted[0].formats[TEXT_MARKDOWN],
        source.as_bytes()
    );
    assert_eq!(
        e.platform().pasted[0].formats[TEXT_PLAIN],
        source.as_bytes()
    );
    assert_eq!(e.next_deadline(), Some(150));
    timer(&mut e, 149);
    assert_ne!(e.platform().clipboard, original);
    timer(&mut e, 150);
    assert_eq!(e.platform().clipboard, original);
    assert_eq!(e.next_deadline(), None);
}

#[test]
fn numeric_choices_one_through_five_restore_exact_original() {
    for digit in 1..=5 {
        let mut e = engine("unknown", "");
        let original = opaque_original("# Title\r\n\r\n## Heading\r\n\r\n**bold**\r\n");
        e.platform_mut().clipboard = original.clone();
        hot(&mut e, 0);
        assert_eq!(e.platform().menus[0].kind, MenuKind::Secondary);
        assert_eq!(e.platform().pastes, 0);
        assert_eq!(e.next_deadline(), None);
        key(&mut e, 1, MenuKey::Digit(digit));
        assert_eq!(e.platform().pastes, 1);
        assert_eq!(e.platform().hides, 1);
        let pasted = &e.platform().pasted[0];
        match digit {
            1 => assert_eq!(pasted, &original),
            2 => assert_eq!(
                pasted,
                &Snapshot::text("一、Title\n\n（一）Heading\n\nbold")
            ),
            3 => {
                assert!(pasted.formats[TEXT_HTML]
                    .windows(11)
                    .any(|w| w == b"MsoHeading1"));
                assert!(pasted.formats[TEXT_RTF].starts_with(b"{\\rtf1"));
            }
            4 => assert_eq!(pasted.formats[TEXT_MARKDOWN], original.formats[TEXT_PLAIN]),
            5 => assert_eq!(
                pasted,
                &Snapshot::text(cleancopy_core::render_plain(
                    original.source_text().unwrap()
                ))
            ),
            _ => unreachable!(),
        }
        if digit == 1 {
            assert_eq!(e.next_deadline(), None);
        } else {
            assert_eq!(e.next_deadline(), Some(151));
            timer(&mut e, 150);
            assert_ne!(e.platform().clipboard, original);
        }
        timer(&mut e, 151);
        assert_eq!(e.platform().clipboard, original, "choice {digit}");
        assert_eq!(e.next_deadline(), None);
    }
}

#[test]
fn escape_cancels_secondary_and_full_without_any_clipboard_effect() {
    for full in [false, true] {
        let mut e = engine("unknown", "# Title");
        let original = e.platform().clipboard.clone();
        if full {
            self::full(&mut e, 0);
        } else {
            hot(&mut e, 0);
        }
        assert_eq!(e.platform().menus.len(), 1);
        assert_eq!(
            e.platform().menus[0].kind,
            if full {
                MenuKind::Full
            } else {
                MenuKind::Secondary
            }
        );
        assert_eq!(e.next_deadline(), None);
        key(&mut e, 1, MenuKey::Escape);
        timer(&mut e, 1000);
        assert_eq!(e.platform().clipboard, original);
        assert!(e.platform().writes.is_empty());
        assert_eq!(e.platform().pastes, 0);
        assert_eq!(e.platform().hides, 1);
        assert_eq!(e.next_deadline(), None);
    }
}

#[test]
fn silent_off_opens_menu_on_every_completed_press() {
    let mut e = Engine::new(
        Config {
            silent_mode: false,
            ..Config::default()
        },
        Mock {
            app: app("notepad.exe"),
            clipboard: Snapshot::text("# Title"),
            ..Mock::default()
        },
    );
    for t in [0, 1000, 2000] {
        hot(&mut e, t);
        assert_eq!(e.platform().menus.last().unwrap().kind, MenuKind::Full);
        assert_eq!(e.next_deadline(), None);
        key(&mut e, t + 1, MenuKey::Digit(2));
        timer(&mut e, t + 151);
    }
    assert_eq!(e.platform().menus.len(), 3);
    assert_eq!(e.platform().pastes, 3);
}

#[test]
fn unchecked_remember_does_not_create_rule_and_checked_choice_survives_toml() {
    for checked in [false, true] {
        let mut e = engine("custom-editor", "# Title");
        hot(&mut e, 0);
        let menu = e.platform().menus.last().unwrap().clone();
        assert!(menu.show_remember);
        assert!(!menu.remember);
        e.handle(Event::Remember {
            t: 401,
            id: menu.id,
            checked,
        })
        .unwrap();
        key(&mut e, 402, MenuKey::Digit(3));
        timer(&mut e, 552);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        e.config().save(&path).unwrap();
        let config = Config::load(path).unwrap();
        assert_eq!(
            config.format_for(&app("custom-editor")),
            checked.then_some(Format::WordRichText)
        );
        let mut reloaded = Engine::new(
            config,
            Mock {
                app: app("custom-editor"),
                clipboard: Snapshot::text("# Next"),
                ..Mock::default()
            },
        );
        hot(&mut reloaded, 1000);
        assert_eq!(reloaded.platform().pastes, usize::from(checked));
        assert_eq!(reloaded.platform().menus.len(), usize::from(!checked));
    }
}

#[test]
fn undo_effect_order_is_undo_then_original_paste_then_exact_current_restore() {
    let mut e = engine("WINWORD.EXE", "");
    let original = opaque_original("# Original\r\n");
    e.platform_mut().clipboard = original.clone();
    hot(&mut e, 0);
    timer(&mut e, 150);
    let current = opaque_original("new clipboard\r\n");
    e.platform_mut().clipboard = current.clone();
    e.platform_mut().effects.clear();
    e.handle(Event::Undo { t: 600 }).unwrap();
    assert_eq!(e.platform().pasted[1], original);
    let effects = &e.platform().effects;
    let undo = effects.iter().position(|&effect| effect == "undo").unwrap();
    let write = effects
        .iter()
        .position(|&effect| effect == "write")
        .unwrap();
    let paste = effects
        .iter()
        .position(|&effect| effect == "paste")
        .unwrap();
    assert!(undo < write && write < paste);
    timer(&mut e, 750);
    assert_eq!(e.platform().clipboard, current);
    assert_eq!(e.platform().effects.last(), Some(&"write"));
}

#[test]
fn injected_clock_drives_deadlines_without_wall_clock_or_sleep() {
    let mut e = engine("notepad.exe", "# Title");
    e.platform_mut().now = 1000;
    e.hotkey_pressed().unwrap();
    assert_eq!(e.platform().pastes, 1);
    assert!(e.platform().menus.is_empty());
    assert_eq!(e.next_deadline(), Some(1150));
    e.platform_mut().now = 1149;
    e.tick().unwrap();
    assert_ne!(e.platform().clipboard, Snapshot::text("# Title"));
    e.platform_mut().now = 1150;
    e.tick().unwrap();
    assert_eq!(e.platform().clipboard, Snapshot::text("# Title"));
    assert_eq!(e.next_deadline(), None);
    e.platform_mut().now = 1200;
    e.full_menu_hotkey_pressed().unwrap();
    assert_eq!(e.platform().menus.len(), 1);
    assert_eq!(e.platform().menus[0].kind, MenuKind::Full);
    assert_eq!(e.platform().pastes, 1);
    assert_eq!(e.next_deadline(), None);
}

#[test]
fn disabled_mapped_format_does_not_become_an_unselectable_menu_default() {
    let mut config = Config {
        silent_mode: false,
        ..Config::default()
    };
    config
        .secondary_items
        .iter_mut()
        .find(|item| item.format == Format::WordRichText)
        .unwrap()
        .enabled = false;
    let mut e = Engine::new(
        config,
        Mock {
            app: app("WINWORD.EXE"),
            clipboard: Snapshot::text("# Title"),
            ..Mock::default()
        },
    );
    hot(&mut e, 0);
    assert_eq!(
        e.platform().menus[0].default_choice,
        Choice::Format(Format::ChineseNumbered)
    );
    key(&mut e, 1, MenuKey::Enter);
    assert_eq!(e.platform().pastes, 1);
}

#[test]
fn stale_escape_and_invalid_accelerators_cannot_dismiss_or_paste_new_menu() {
    let mut e = engine("unknown", "# Title");
    full(&mut e, 0);
    let old_id = e.platform().menus[0].id;
    key(&mut e, 2, MenuKey::Escape);
    full(&mut e, 3);
    e.handle(Event::MenuKey {
        t: 5,
        id: old_id,
        key: MenuKey::Escape,
    })
    .unwrap();
    for digit in [0, 6, 255] {
        key(&mut e, 6, MenuKey::Digit(digit));
    }
    assert_eq!(e.platform().hides, 1);
    assert_eq!(e.platform().pastes, 0);
    key(&mut e, 7, MenuKey::Enter);
    assert_eq!(e.platform().pastes, 1);
}

#[test]
fn unknown_primary_pastes_plain_and_huge_directly_but_other_content_opens_secondary() {
    for snapshot in [
        Snapshot::text("# heading"),
        Snapshot::text("ordinary text"),
        Snapshot::text(format!("# {}", "x".repeat(MAX_TEXT_BYTES))),
        Snapshot {
            formats: [("image/png".into(), vec![137, 80, 78, 71])].into(),
        },
        Snapshot {
            formats: [(TEXT_PLAIN.into(), vec![255, 0])].into(),
        },
        Snapshot::default(),
    ] {
        let convertible = snapshot.source_text().is_some();
        let markdown = snapshot
            .source_text()
            .is_some_and(cleancopy_core::is_markdown);
        let mut e = engine("unknown", "");
        e.platform_mut().clipboard = snapshot.clone();
        hot(&mut e, 0);
        if !markdown
            && snapshot
                .formats
                .get(TEXT_PLAIN)
                .is_some_and(|b| std::str::from_utf8(b).is_ok())
        {
            assert!(e.platform().menus.is_empty());
            assert_eq!(e.platform().pasted, vec![snapshot]);
            assert!(e.platform().writes.is_empty());
            assert_eq!(e.next_deadline(), None);
            continue;
        }
        assert_eq!(e.platform().menus.len(), 1);
        let menu = &e.platform().menus[0];
        assert_eq!(menu.kind, MenuKind::Secondary);
        assert!(menu.show_remember);
        assert!(menu.formats.iter().all(|item| item.enabled == convertible));
        if !markdown {
            assert_eq!(menu.default_choice, Choice::Original);
            assert!(menu.formats.iter().all(|item| item.muted));
        }
        assert_eq!(e.platform().pastes, 0);
        assert!(e.platform().writes.is_empty());
        assert_eq!(e.platform().clipboard, snapshot);
        assert_eq!(e.next_deadline(), None);
        key(&mut e, 1, MenuKey::Enter);
        assert_eq!(e.platform().pastes, 1);
        if !markdown {
            assert_eq!(e.platform().pasted, vec![snapshot]);
            assert!(e.platform().writes.is_empty());
            assert_eq!(e.next_deadline(), None);
        }
    }
}

#[test]
fn explicit_full_hotkey_overrides_silent_mode_for_all_content() {
    for silent_mode in [false, true] {
        for (exe, snapshot) in [
            ("WINWORD.EXE", Snapshot::text("# heading")),
            ("unknown", Snapshot::text("# heading")),
            ("unknown", Snapshot::text("ordinary text")),
            (
                "WINWORD.EXE",
                Snapshot {
                    formats: [("image/png".into(), vec![137, 80])].into(),
                },
            ),
        ] {
            let mut e = Engine::new(
                Config {
                    silent_mode,
                    ..Config::default()
                },
                Mock {
                    app: app(exe),
                    clipboard: snapshot.clone(),
                    now: 42,
                    ..Mock::default()
                },
            );
            e.full_menu_hotkey_pressed().unwrap();
            assert_eq!(e.platform().menus.len(), 1);
            assert_eq!(e.platform().menus[0].kind, MenuKind::Full);
            assert_eq!(e.platform().pastes, 0);
            assert!(e.platform().writes.is_empty());
            assert_eq!(e.platform().clipboard, snapshot);
            assert_eq!(e.next_deadline(), None);
        }
    }
}

#[test]
fn menu_counts_unicode_scalars_even_for_over_limit_source() {
    for text in [
        "汉字🙂e\u{301}".to_owned(),
        "汉🙂".repeat(MAX_TEXT_BYTES / 7 + 1),
    ] {
        let expected = text.chars().count();
        let mut e = engine("unknown", &text);
        full(&mut e, 0);
        assert_eq!(e.platform().menus[0].source_chars, expected);
        assert!(!e.platform().menus[0].converting);
    }
}

#[test]
fn html_is_preferred_over_plain_and_normalized_before_every_pipeline() {
    for plain in [None, Some("incorrect fallback")] {
        for format in [
            Format::Markdown,
            Format::PlainStripped,
            Format::ChineseNumbered,
            Format::WordRichText,
        ] {
            let html = "<h1>Title</h1><h2>Heading</h2><p><strong>bold</strong> &amp; text</p>";
            let mut e = engine("unknown", "");
            let mut original = Snapshot::default();
            original
                .formats
                .insert(TEXT_HTML.into(), html.as_bytes().to_vec());
            if let Some(plain) = plain {
                original
                    .formats
                    .insert(TEXT_PLAIN.into(), plain.as_bytes().to_vec());
            }
            e.platform_mut().clipboard = original.clone();
            full(&mut e, 0);
            assert_eq!(e.platform().menus[0].content_label, "HTML");
            choose(&mut e, 1, Choice::Format(format));
            let written = &e.platform().pasted[0];
            let text = std::str::from_utf8(&written.formats[TEXT_PLAIN]).unwrap();
            assert!(text.contains("Title"));
            assert!(text.contains("bold"));
            assert!(!text.contains("incorrect fallback"));
            assert!(!text.contains("<h1>"));
            if format == Format::Markdown {
                assert!(text.contains("**bold**"));
            }
            if format == Format::WordRichText {
                assert!(written.formats.contains_key(TEXT_RTF));
            }
            timer(&mut e, 151);
            assert_eq!(e.platform().clipboard, original);
        }
    }
}

#[test]
fn explicit_markdown_takes_precedence_over_html() {
    let mut e = engine("notion", "plain");
    e.platform_mut()
        .clipboard
        .formats
        .insert(TEXT_HTML.into(), b"<h1>wrong</h1>".to_vec());
    e.platform_mut()
        .clipboard
        .formats
        .insert(TEXT_MARKDOWN.into(), b"# correct\r\n".to_vec());
    hot(&mut e, 0);
    assert_eq!(
        e.platform().pasted[0].formats[TEXT_MARKDOWN],
        b"# correct\r\n"
    );
}

fn large_engine() -> Engine<Mock> {
    engine(
        "WINWORD.EXE",
        &format!("# Title\n\n{}", "**bold** text\n\n".repeat(18_000)),
    )
}
fn await_worker(e: &mut Engine<Mock>) {
    for _ in 0..5000 {
        if e.platform().pastes > 0 || e.next_deadline().is_none() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
        let t = e.next_deadline().unwrap();
        timer(e, t);
    }
    panic!("worker did not finish within five seconds");
}

#[test]
fn large_conversion_progress_completes_via_tick_and_restores() {
    let mut e = large_engine();
    let original = e.platform().clipboard.clone();
    hot(&mut e, 0);
    assert_eq!(e.platform().pastes, 0);
    assert!(e.platform().writes.is_empty());
    assert!(e.platform().menus.last().unwrap().converting);
    assert_eq!(e.next_deadline(), Some(16));
    // Other hotkeys and choices cannot overlap the job.
    hot(&mut e, 1);
    full(&mut e, 2);
    choose(&mut e, 3, Choice::Original);
    await_worker(&mut e);
    assert_eq!(e.platform().pastes, 1);
    assert_eq!(e.platform().writes.len(), 1);
    let deadline = e.next_deadline().unwrap();
    timer(&mut e, deadline);
    assert_eq!(e.platform().clipboard, original);
}

#[test]
fn cancel_or_pause_large_conversion_never_writes_or_pastes_and_stale_result_is_safe() {
    for pause in [false, true] {
        let mut e = large_engine();
        let original = e.platform().clipboard.clone();
        hot(&mut e, 0);
        let old_id = e.platform().menus.last().unwrap().id;
        e.handle(if pause {
            Event::Pause { t: 1, until: None }
        } else {
            Event::Cancel { t: 1 }
        })
        .unwrap();
        assert_eq!(e.next_deadline(), None);
        std::thread::sleep(std::time::Duration::from_millis(30));
        timer(&mut e, 100);
        assert!(e.platform().writes.is_empty());
        assert_eq!(e.platform().pastes, 0);
        assert_eq!(e.platform().clipboard, original);
        e.handle(Event::Resume { t: 101 }).unwrap();
        e.platform_mut().clipboard = Snapshot::text("# new");
        full(&mut e, 102);
        e.handle(Event::MenuChoice {
            t: 103,
            id: old_id,
            choice: Choice::Original,
        })
        .unwrap();
        choose(&mut e, 104, Choice::Original);
        assert_eq!(e.platform().pasted, vec![Snapshot::text("# new")]);
    }
}

#[test]
fn background_completion_rechecks_clipboard_and_foreground() {
    for focus in [false, true] {
        let mut e = large_engine();
        hot(&mut e, 0);
        if focus {
            e.platform_mut().app = app("other");
        } else {
            e.platform_mut().clipboard = Snapshot::text("new clipboard");
        }
        await_worker(&mut e);
        assert!(e.platform().writes.is_empty());
        assert_eq!(e.platform().pastes, 0);
        assert_eq!(e.platform().notices.len(), 1);
    }
}

struct MockSecure(std::rc::Rc<std::cell::Cell<bool>>);
impl cleancopy_desktop::secure_input::SecureInput for MockSecure {
    fn enabled(&self) -> bool {
        self.0.get()
    }
}
fn secure_engine(text: &str) -> (Engine<Mock>, std::rc::Rc<std::cell::Cell<bool>>) {
    let state = std::rc::Rc::new(std::cell::Cell::new(false));
    let e = Engine::with_secure_input(
        Config::default(),
        Mock {
            app: app("WINWORD.EXE"),
            clipboard: Snapshot::text(text),
            ..Default::default()
        },
        MockSecure(state.clone()),
    );
    (e, state)
}
#[test]
fn secure_input_blocks_both_hotkeys_before_read_or_menu() {
    for full_hotkey in [false, true] {
        let (mut e, state) = secure_engine("# secret");
        state.set(true);
        if full_hotkey {
            full(&mut e, 0);
        } else {
            hot(&mut e, 0);
        }
        assert_eq!(e.platform().effects, vec!["notify"]);
        assert!(e.platform().menus.is_empty());
        assert!(e.platform().writes.is_empty());
        assert_eq!(e.platform().pastes, 0);
        assert!(e.platform().notices[0].contains("Secure Input"));
    }
}
#[test]
fn secure_input_rechecked_for_menu_and_background_completion() {
    for background in [false, true] {
        let text = if background {
            format!("# Title\n{}", "**bold** ".repeat(30_000))
        } else {
            "# small".into()
        };
        let (mut e, state) = secure_engine(&text);
        if background {
            hot(&mut e, 0);
        } else {
            full(&mut e, 0);
        }
        state.set(true);
        if background {
            await_worker(&mut e);
        } else {
            choose(&mut e, 1, Choice::Format(Format::WordRichText));
        }
        assert!(e.platform().writes.is_empty());
        assert_eq!(e.platform().pastes, 0);
        assert!(e
            .platform()
            .notices
            .last()
            .unwrap()
            .contains("Secure Input"));
    }
}

#[test]
fn secure_input_enabled_during_write_prevents_injection_but_restores_clipboard() {
    let (mut e, state) = secure_engine("# original");
    let original = e.platform().clipboard.clone();
    e.platform_mut().secure_after_write = Some(state);
    hot(&mut e, 0);
    assert_eq!(e.platform().writes.len(), 1);
    assert_eq!(e.platform().pastes, 0);
    assert_eq!(e.next_deadline(), Some(150));
    timer(&mut e, 150);
    assert_eq!(e.platform().clipboard, original);
    assert_eq!(e.platform().writes.len(), 2);
}

const CHATGPT_CODE_HTML: &str = r#"<h2>项目周报</h2><pre class="!overflow-visible"><div class="contain-inline-size rounded-md"><div class="flex items-center px-4 py-2 text-xs">rust</div><div class="sticky top-9"><div><span><button aria-label="Copy"><svg viewBox="0 0 24 24"><path d="M7 5"></path></svg>Copy code</button></span></div></div><div class="overflow-y-auto p-4" dir="ltr"><code class="whitespace-pre! language-rust"><span class="hljs-keyword">fn</span> <span class="hljs-title function_">main</span>() {}
</code></div></div></pre>"#;

fn html_engine(html: &str) -> Engine<Mock> {
    let mut e = engine("unknown", "");
    let mut original = Snapshot::default();
    original
        .formats
        .insert(TEXT_HTML.into(), html.as_bytes().to_vec());
    e.platform_mut().clipboard = original;
    e
}

#[test]
fn html_source_menu_counts_visible_text_not_tags() {
    let mut e = html_engine("<h2>项目周报</h2>\n<p><strong>粗</strong>体 &amp; x</p>");
    full(&mut e, 0);
    // "项目周报" + newline-as-space + "粗体 & x"
    assert_eq!(e.platform().menus[0].source_chars, 11);
}

#[test]
fn chatgpt_code_block_header_never_leaks_into_code() {
    for format in [
        Format::Markdown,
        Format::PlainStripped,
        Format::ChineseNumbered,
        Format::WordRichText,
    ] {
        let mut e = html_engine(CHATGPT_CODE_HTML);
        full(&mut e, 0);
        choose(&mut e, 1, Choice::Format(format));
        let written = &e.platform().pasted[0];
        let text = std::str::from_utf8(&written.formats[TEXT_PLAIN]).unwrap();
        assert!(!text.contains("rustfn"), "{format:?}: {text}");
        assert!(!text.contains("Copy code"), "{format:?}: {text}");
        assert!(text.contains("fn main() {}"), "{format:?}: {text}");
        if format == Format::Markdown {
            assert!(text.contains("```rust\nfn main() {}\n```"), "{text}");
        }
        if format == Format::WordRichText {
            let html = std::str::from_utf8(&written.formats[TEXT_HTML]).unwrap();
            assert!(!html.contains("rustfn"), "{html}");
        }
    }
}

#[test]
fn large_conversion_keeps_the_target_locked_at_hotkey_time() {
    // Unknown app: secondary menu, choose a format, conversion runs in the
    // background. The progress menu must keep the captured target and its
    // "unrecognized" badge (it previously flipped to the app name).
    let mut e = engine(
        "xmessage",
        &format!("# Title\n\n{}", "**bold** text\n\n".repeat(18_000)),
    );
    hot(&mut e, 0);
    let first = e.platform().menus[0].clone();
    assert!(first.target_unrecognized);
    choose(&mut e, 1, Choice::Format(Format::PlainStripped));
    let progress = e.platform().menus.last().unwrap().clone();
    assert!(progress.converting);
    assert_eq!(
        progress.app, first.app,
        "target re-detected during conversion"
    );
    assert!(
        progress.target_unrecognized,
        "badge changed during conversion"
    );
}

fn async_engine(exe: &str, text: &str, silent: bool) -> Engine<Mock> {
    let config = Config {
        silent_mode: silent,
        ..Config::default()
    };
    Engine::new(
        config,
        Mock {
            app: app(exe),
            clipboard: Snapshot::text(text),
            async_read: Some(AsyncRead::default()),
            ..Default::default()
        },
    )
}
fn finish_read(e: &mut Engine<Mock>, t: u64) {
    e.platform_mut().async_read.as_mut().unwrap().ready = true;
    timer(e, t);
}

#[test]
fn full_menu_appears_before_the_clipboard_is_read_then_fills_in() {
    let big = format!("# T\n\n{}", "x".repeat(3 * 1024 * 1024));
    let mut e = async_engine("xmessage", &big, true);
    full(&mut e, 0);
    // Shown immediately, without any synchronous clipboard read.
    assert!(!e.platform().effects.contains(&"read"));
    let loading = e.platform().menus[0].clone();
    assert_eq!(loading.kind, MenuKind::Full);
    assert!(loading.loading);
    assert!(!loading.original_enabled);
    assert!(loading.formats.iter().all(|f| !f.enabled));
    assert_eq!(e.next_deadline(), Some(16));
    // Nothing happens until the background read completes.
    timer(&mut e, 16);
    assert_eq!(e.platform().menus.len(), 1);
    finish_read(&mut e, 32);
    let filled = e.platform().menus.last().unwrap().clone();
    assert_eq!(filled.kind, MenuKind::Full);
    assert!(!filled.loading);
    assert_eq!(filled.source_chars, big.chars().count());
    assert_eq!(filled.app, loading.app);
}

#[test]
fn unknown_app_secondary_menu_appears_first_and_fills_in() {
    let mut e = async_engine("xmessage", "# Title\n\n- a\n", true);
    hot(&mut e, 0);
    let loading = e.platform().menus[0].clone();
    assert_eq!(loading.kind, MenuKind::Secondary);
    assert!(loading.loading && loading.target_unrecognized);
    finish_read(&mut e, 16);
    let filled = e.platform().menus.last().unwrap().clone();
    assert_eq!(filled.kind, MenuKind::Secondary);
    assert!(!filled.loading);
    assert_eq!(filled.content_label, "Markdown");
    assert_eq!(e.platform().pastes, 0);
}

#[test]
fn unknown_app_plain_text_closes_the_loading_menu_and_pastes_original() {
    let mut e = async_engine("xmessage", "just words", true);
    hot(&mut e, 0);
    assert!(e.platform().menus[0].loading);
    finish_read(&mut e, 16);
    assert_eq!(e.platform().menus.len(), 1, "no second menu for plain text");
    assert!(e.platform().hides >= 1);
    assert_eq!(e.platform().pastes, 1);
    assert!(e.platform().writes.is_empty());
}

#[test]
fn escape_during_loading_cancels_and_ignores_the_late_result() {
    let mut e = async_engine("xmessage", "# T\n", true);
    full(&mut e, 0);
    let id = e.platform().menus[0].id;
    e.handle(Event::MenuKey {
        t: 1,
        id,
        key: MenuKey::Escape,
    })
    .unwrap();
    assert!(e.platform().hides >= 1);
    assert_eq!(e.next_deadline(), None);
    finish_read(&mut e, 20);
    assert_eq!(e.platform().menus.len(), 1);
    assert_eq!(e.platform().pastes, 0);
    // Choices on the loading menu id are ignored too.
    e.handle(Event::MenuChoice {
        t: 21,
        id,
        choice: Choice::Original,
    })
    .unwrap();
    assert_eq!(e.platform().pastes, 0);
}

#[test]
fn hotkeys_are_ignored_while_the_clipboard_is_loading() {
    let mut e = async_engine("xmessage", "# T\n", true);
    full(&mut e, 0);
    hot(&mut e, 1);
    full(&mut e, 2);
    assert_eq!(e.platform().async_read.as_ref().unwrap().started, 1);
    assert_eq!(e.platform().menus.len(), 1);
}

#[test]
fn recognized_app_silent_paste_never_shows_a_loading_menu() {
    let mut e = async_engine("WINWORD.EXE", "# Title\n", true);
    hot(&mut e, 0);
    assert!(e.platform().menus.is_empty());
    assert_eq!(e.platform().async_read.as_ref().unwrap().started, 0);
    assert_eq!(e.platform().pastes, 1);
}

/// Report v4 B-2: a cancel that arrives after the worker finished but before
/// the engine polled the result must still win; nothing is pasted.
#[test]
fn cancel_after_worker_finished_but_before_poll_never_pastes() {
    let mut e = large_engine();
    let original = e.platform().clipboard.clone();
    hot(&mut e, 0);
    std::thread::sleep(std::time::Duration::from_millis(1500));
    e.handle(Event::Cancel { t: 1 }).unwrap();
    timer(&mut e, 100);
    timer(&mut e, 10_000);
    assert_eq!(e.platform().pastes, 0);
    assert!(e.platform().writes.is_empty());
    assert_eq!(e.platform().clipboard, original);
}
