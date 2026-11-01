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
}
impl Mock {
    fn check(&mut self, effect: &'static str) -> Result<(), &'static str> {
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
        if let Some(app) = self.focus_after_write.take() {
            self.app = app;
        }
        Ok(())
    }
    fn send_paste(&mut self) -> Result<(), Self::Error> {
        self.check("paste")?;
        self.pastes += 1;
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
    assert_eq!(e.next_deadline(), Some(1400));
    timer(&mut e, 1399);
    assert_eq!(e.platform().pastes, 0);
    timer(&mut e, 1400);
    assert_eq!(e.platform().pastes, 1);
    assert_eq!(e.platform().clipboard.formats.len(), 3);
    assert!(e.platform().clipboard.formats.contains_key(TEXT_PLAIN));
    assert!(
        String::from_utf8_lossy(&e.platform().clipboard.formats[TEXT_HTML]).contains("MsoHeading1")
    );
    assert!(e.platform().clipboard.formats[TEXT_RTF].starts_with(b"{\\rtf1"));
    assert_eq!(e.next_deadline(), Some(1550));
    timer(&mut e, 1549);
    assert_ne!(e.platform().clipboard, original);
    timer(&mut e, 1550);
    assert_eq!(e.platform().clipboard, original);
    assert_eq!(e.next_deadline(), None);
    assert_eq!(e.config().recent_pastes[0].timestamp, 1400);
}

#[test]
fn unknown_secondary_remembers_and_honors_configured_items() {
    let mut e = engine("unknown", "## Heading");
    hot(&mut e, 0);
    timer(&mut e, 400);
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
    timer(&mut e, 1000);
    assert_eq!(e.platform().pastes, 2);
    assert_eq!(e.platform().menus.len(), 2); // show + checkbox refresh only
}

#[test]
fn inclusive_double_press_always_full_and_never_double_pastes() {
    for delta in [0, 1, 399, 400] {
        let mut e = engine("WINWORD.EXE", "# heading");
        hot(&mut e, 100);
        hot(&mut e, 100 + delta);
        let menu = e.platform().menus.last().unwrap();
        assert_eq!(menu.kind, MenuKind::Full);
        assert_eq!(menu.formats.len(), 4);
        assert_eq!(menu.default_choice, Choice::Format(Format::WordRichText));
        assert_eq!(menu.content_label, "Markdown");
        assert!(menu.original_enabled);
        timer(&mut e, 500);
        assert_eq!(e.platform().pastes, 0);
        choose(&mut e, 501, Choice::Original);
        timer(&mut e, 900);
        assert_eq!(e.platform().pastes, 1);
        assert!(e.platform().writes.is_empty());
    }
}

#[test]
fn silent_false_first_menu_second_ignored_and_full_formats_respect_disabled_secondary() {
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
    hot(&mut e, 100);
    timer(&mut e, 400);
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
    timer(&mut e, 400);
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
    timer(&mut e, 400);
    assert_eq!(e.platform().pastes, 1);
    assert!(e.platform().writes.is_empty());
    hot(&mut e, 500);
    hot(&mut e, 600);
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
        timer(&mut e, 400);
        assert_eq!(e.platform().clipboard, snapshot);
        assert!(e.platform().writes.is_empty());
        hot(&mut e, 500);
        hot(&mut e, 600);
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
    timer(&mut e, 400);
    timer(&mut e, 550);
    let current = Snapshot {
        formats: [("opaque/current".into(), vec![255, 0, 42])].into(),
    };
    e.platform_mut().clipboard = current.clone();
    e.platform_mut().app = app("other");
    e.handle(Event::Undo { t: 600 }).unwrap();
    assert_eq!(e.platform().undos, 0);
    e.platform_mut().app = app("WINWORD.EXE");
    hot(&mut e, 650);
    hot(&mut e, 700);
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
    hot(&mut e, 0);
    e.handle(Event::Cancel { t: 1 }).unwrap();
    timer(&mut e, 1000);
    assert_eq!(e.platform().pastes, 0);
    hot(&mut e, 1100);
    hot(&mut e, 1200);
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
    timer(&mut e, 1900);
    assert_eq!(e.platform().pastes, 0);
    hot(&mut e, 2000);
    timer(&mut e, 2400);
    e.handle(Event::Pause {
        t: 2401,
        until: None,
    })
    .unwrap();
    hot(&mut e, 2402);
    timer(&mut e, 2550); // pausing cannot abandon restoration
    assert_eq!(e.platform().pastes, 1);
    assert_eq!(e.next_deadline(), None);
    hot(&mut e, 3000);
    assert_eq!(e.next_deadline(), None);
    e.handle(Event::Resume { t: 3001 }).unwrap();
    hot(&mut e, 3002);
    timer(&mut e, 100);
    assert_eq!(e.next_deadline(), Some(3402));
}

#[test]
fn foreground_and_clipboard_captured_before_delay_and_checked_before_paste() {
    for change_focus in [true, false] {
        let mut e = engine("WINWORD.EXE", "# first");
        hot(&mut e, 0);
        if change_focus {
            e.platform_mut().app = app("other");
        } else {
            e.platform_mut().clipboard = Snapshot::text("# newer");
        }
        timer(&mut e, 400);
        assert_eq!(e.platform().pastes, 0);
        assert!(e.platform().writes.is_empty());
        assert_eq!(e.platform().notices.len(), 1);
    }
    let mut e = engine("WINWORD.EXE", "# first");
    hot(&mut e, 0);
    e.platform_mut().app.name = "renamed display".into();
    timer(&mut e, 400);
    assert_eq!(e.platform().pastes, 1);
    let mut e = engine("", "# first");
    e.platform_mut().app.exe = None;
    hot(&mut e, 0);
    hot(&mut e, 1);
    choose(&mut e, 2, Choice::Original);
    assert_eq!(e.platform().pastes, 0);
}

#[test]
fn no_overlapping_transaction_and_external_change_not_overwritten() {
    let mut e = engine("WINWORD.EXE", "# first");
    hot(&mut e, 0);
    timer(&mut e, 400);
    hot(&mut e, 401);
    let newest = Snapshot::text("new external copy");
    e.platform_mut().clipboard = newest.clone();
    timer(&mut e, 550);
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
        timer(&mut e, 400);
        if effect == "notify" {
            e.platform_mut().clipboard = Snapshot::text("external");
        }
        e.platform_mut().fail = Some(effect);
        assert_eq!(e.handle(Event::Timer { t: 550 }), Err(effect));
        assert_eq!(e.next_deadline(), Some(700));
        hot(&mut e, 551);
        timer(&mut e, 700);
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
        hot(&mut e, 0);
        e.platform_mut().fail = Some(effect);
        assert_eq!(e.handle(Event::Timer { t: 400 }), Err(effect));
        timer(&mut e, 550);
        assert_eq!(e.platform().clipboard, original);
        assert_eq!(e.platform().pastes, 0);
    }
    let mut e = engine("unknown", "# title");
    hot(&mut e, 0);
    e.platform_mut().fail = Some("show");
    assert_eq!(e.handle(Event::Timer { t: 400 }), Err("show"));
    hot(&mut e, 500);
    hot(&mut e, 501);
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
    hot(&mut e, 0);
    e.platform_mut().focus_after_write = Some(app("other"));
    timer(&mut e, 400);
    assert_eq!(e.platform().pastes, 0);
    assert_eq!(e.next_deadline(), Some(550));
    timer(&mut e, 550);
    assert_eq!(e.platform().clipboard, original);
}

#[test]
fn cancellation_hide_failure_invalidates_menu_callbacks() {
    let mut e = engine("unknown", "# original");
    hot(&mut e, 0);
    hot(&mut e, 1);
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
        timer(&mut e, 400);
        timer(&mut e, 550);
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
        if silent_mode {
            timer(&mut e, 400);
        }
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
        hot(&mut e, 0);
        hot(&mut e, 1);
        let menu = e.platform().menus.last().unwrap();
        assert_eq!(menu.default_choice, Choice::Original);
        assert_eq!(menu.content_label, label);
    }
}

#[test]
fn late_second_press_consumes_pending_only_once() {
    let mut e = engine("WINWORD.EXE", "# title");
    hot(&mut e, 0);
    hot(&mut e, 401);
    timer(&mut e, 402);
    assert_eq!(e.platform().pastes, 1);
    timer(&mut e, 551);
    assert_eq!(e.platform().pastes, 1);
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
        hot(&mut e, 0);
        hot(&mut e, 1);
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
    timer(&mut e, 400);
    assert!(e.platform().writes.is_empty());
}
