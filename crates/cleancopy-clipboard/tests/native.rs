//! These tests replace the system clipboard. Run in an isolated Xvfb display.

// Hold a client for the entire test binary, including gaps between tests: Xvfb
// otherwise resets when the last clipboard connection closes. Serialize owners
// even with cargo's default parallel test runner.
#[cfg(target_os = "linux")]
fn x11_test_guard() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    static CONNECTION: std::sync::OnceLock<x11rb::rust_connection::RustConnection> =
        std::sync::OnceLock::new();
    let lock = LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    CONNECTION.get_or_init(|| x11rb::connect(None).unwrap().0);
    lock
}
#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
#[ignore = "requires a graphical clipboard; replaces its contents"]
fn native_modes_and_byte_sizes() {
    #[cfg(target_os = "linux")]
    let _guard = x11_test_guard();
    use cleancopy_clipboard::{ClipboardWriter, NativeClipboard, Payload, WriteMode};
    let clipboard = NativeClipboard::new().unwrap();
    let payload = Payload {
        html: "<b>中文🙂</b>".into(),
        plain: "中文🙂".into(),
        markdown: "**中文🙂**".into(),
    };
    clipboard.write(&payload).unwrap();
    assert_eq!(clipboard.read_text().unwrap(), payload.plain);
    let formats = clipboard.list_formats().unwrap();
    let html = formats
        .iter()
        .find(|f| matches!(f.name.as_str(), "text/html" | "public.html"))
        .unwrap();
    assert_eq!(html.bytes, payload.html.len());
    #[cfg(target_os = "linux")]
    {
        for name in [
            "UTF8_STRING",
            "text/plain;charset=utf-8",
            "text/plain;charset=UTF-8",
            "text/plain",
            "TEXT",
        ] {
            assert_eq!(
                formats.iter().find(|f| f.name == name).unwrap().bytes,
                payload.plain.len()
            );
        }
        assert_eq!(
            formats
                .iter()
                .find(|f| f.name == "text/markdown")
                .unwrap()
                .bytes,
            payload.markdown.len()
        );
        assert_eq!(
            formats.iter().find(|f| f.name == "TARGETS").unwrap().bytes,
            formats.len() * 4
        );
    }
    #[cfg(target_os = "macos")]
    assert_eq!(
        formats
            .iter()
            .find(|f| f.name == "net.daringfireball.markdown")
            .unwrap()
            .bytes,
        payload.markdown.len()
    );
    for mode in [WriteMode::Plain, WriteMode::Markdown] {
        clipboard.write_mode(&payload, mode).unwrap();
        let text = if mode == WriteMode::Plain {
            &payload.plain
        } else {
            &payload.markdown
        };
        assert_eq!(&clipboard.read_text().unwrap(), text);
        assert!(!clipboard.list_formats().unwrap().iter().any(|f| matches!(
            f.name.as_str(),
            "text/html" | "public.html" | "net.daringfireball.markdown"
        )));
    }
    clipboard.write_mode(&payload, WriteMode::Html).unwrap();
    assert_eq!(clipboard.read_text().unwrap(), payload.plain);
    assert!(clipboard
        .list_formats()
        .unwrap()
        .iter()
        .any(|f| matches!(f.name.as_str(), "text/html" | "public.html")));
    // Native X11 serves these targets using the INCR delete/chunk handshake.
    let large = Payload {
        html: format!("<p>{}</p>", "中".repeat(350_000)),
        plain: "🙂".repeat(300_000),
        markdown: String::new(),
    };
    clipboard.write(&large).unwrap();
    let formats = clipboard.list_formats().unwrap();
    assert_eq!(
        formats
            .iter()
            .find(|f| matches!(f.name.as_str(), "text/html" | "public.html"))
            .unwrap()
            .bytes,
        large.html.len()
    );
    assert_eq!(clipboard.read_text().unwrap(), large.plain);
    #[cfg(target_os = "linux")]
    {
        let replacement = NativeClipboard::new().unwrap();
        replacement.write_mode(&payload, WriteMode::Plain).unwrap();
        // Stopping a superseded owner must not clear a newer selection.
        drop(clipboard);
        assert_eq!(replacement.read_text().unwrap(), payload.plain);
        drop(replacement);
        let reader = NativeClipboard::new().unwrap();
        assert!(reader.list_formats().unwrap().is_empty());
        assert!(reader.read_text().is_err());
    }
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "requires an isolated X11 display; replaces clipboard contents"]
fn html_only_snapshot_has_no_synthetic_text_targets() {
    use cleancopy_clipboard::{ClipboardWriter, NativeClipboard, NativeSnapshot, NativeTarget};

    let _guard = x11_test_guard();
    let source = NativeClipboard::new().unwrap();
    let reader = NativeClipboard::new().unwrap();
    // Exercise both direct properties and the real INCR handshake using only
    // the public crate API. HTML-to-Markdown conversion belongs to the engine.
    for html in [
        "<h1>中文🙂</h1><p><b>HTML only</b></p>".to_owned(),
        format!("<p>{}</p>", "中文🙂".repeat(20_000)),
    ] {
        let snapshot = NativeSnapshot {
            targets: vec![NativeTarget {
                target: b"text/html".to_vec(),
                type_name: b"text/html".to_vec(),
                format: 8,
                data: html.as_bytes().to_vec(),
            }],
        };
        source.write_snapshot(&snapshot).unwrap();
        let formats = reader.list_formats().unwrap();
        assert_eq!(formats.len(), 2, "only TARGETS and text/html: {formats:?}");
        assert_eq!(
            formats
                .iter()
                .find(|format| format.name == "text/html")
                .unwrap()
                .bytes,
            html.len()
        );
        assert!(reader.read_text().is_err(), "must not fabricate plain text");
        assert_eq!(reader.read_snapshot().unwrap(), snapshot);
        let restored = NativeClipboard::new().unwrap();
        restored.write_snapshot(&snapshot).unwrap();
        assert_eq!(reader.read_snapshot().unwrap(), snapshot);
    }
}
