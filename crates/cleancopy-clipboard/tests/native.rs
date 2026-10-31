//! These tests replace the system clipboard. Run in an isolated Xvfb display.
#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
#[ignore = "requires a graphical clipboard; replaces its contents"]
fn native_modes_and_byte_sizes() {
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
    // Large target reads exercise the X11 property size path (and INCR if the
    // owner uses it). Arboard itself uses one large change-property request.
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
}
