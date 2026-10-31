//! Run in an isolated display: xvfb-run -a cargo test -p cleancopy-cli --test clipboard -- --ignored
#![cfg(target_os = "linux")]

use cleancopy_clipboard::{ClipboardWriter, NativeClipboard};
use std::io::Write;
use std::process::{Command, Output, Stdio};

fn invoke(args: &[&str], input: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_cleancopy"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

#[test]
#[ignore = "requires an isolated X11 display; replaces its clipboard"]
fn clipboard_survives_cli_exit_and_modes_replace_formats() {
    let input = "# 标题\n\n- **First**\n- [x] Done\n";
    for (args, html, text) in [
        (vec![], true, cleancopy_core::render_plain(input)),
        (vec!["--plain"], false, cleancopy_core::render_plain(input)),
        (vec!["--html"], true, cleancopy_core::render_plain(input)),
        (
            vec!["--markdown"],
            false,
            cleancopy_core::render_markdown(input),
        ),
    ] {
        invoke(&args, input);
        let clipboard = NativeClipboard::new().unwrap();
        assert_eq!(clipboard.read_text().unwrap(), text);
        let formats = String::from_utf8(invoke(&["inspect"], "").stdout).unwrap();
        assert_eq!(formats.contains("text/html\t"), html);
        assert!(formats.contains(&format!("UTF8_STRING\t{} bytes", text.len())));
        if html {
            assert!(formats.contains(&format!(
                "text/html\t{} bytes",
                cleancopy_core::render_html(input).len()
            )));
        }
        // Printing must leave the previous clipboard content untouched.
        invoke(&["--print", "html"], "unrelated");
        assert_eq!(clipboard.read_text().unwrap(), text);
    }
}
