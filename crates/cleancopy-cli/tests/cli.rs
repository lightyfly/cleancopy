use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

fn invoke(args: &[&str], input: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_cleancopy"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn cleancopy");
    // Invalid arguments can close stdin before the parent writes to it.
    let _ = child.stdin.take().unwrap().write_all(input.as_bytes());
    child.wait_with_output().expect("wait for cleancopy")
}

fn assert_error(output: Output, code: i32) {
    assert_eq!(output.status.code(), Some(code));
    assert!(output.stdout.is_empty());
    assert!(!output.stderr.is_empty());
}

struct TempInput(PathBuf);

impl TempInput {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "cleancopy-cli-{}-{}.md",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::write(&path, "# From a file\n\nHello **world**.\n").unwrap();
        Self(path)
    }
}

impl Drop for TempInput {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

#[test]
fn plain_reads_stdin_by_default_and_explicit_dash() {
    let input = "# Title\n\nHello **world**.\n";
    for args in [vec!["--print", "plain"], vec!["--print", "plain", "-"]] {
        let output = invoke(&args, input);
        assert!(output.status.success());
        assert_eq!(output.stdout, cleancopy_core::render_plain(input).as_bytes());
        assert!(output.stderr.is_empty());
    }
}

#[test]
fn plain_reads_file_instead_of_stdin() {
    let file = TempInput::new();
    let input = fs::read_to_string(&file.0).unwrap();
    let output = invoke(&["--print", "plain", file.0.to_str().unwrap()], "ignored");
    assert!(output.status.success());
    assert_eq!(output.stdout, cleancopy_core::render_plain(&input).as_bytes());
    assert!(output.stderr.is_empty());
}

#[test]
fn markdown_print_uses_markdown_renderer() {
    let input = "# Heading\n\n- **First**\n- Second\n";
    let output = invoke(&["--print", "markdown"], input);
    assert!(output.status.success());
    assert_eq!(output.stdout, cleancopy_core::render_markdown(input).as_bytes());
    assert!(output.stderr.is_empty());
}

#[test]
fn empty_input_exits_one() {
    for input in ["", " \t\r\n", "\u{2003}\n"] {
        assert_error(invoke(&["--print", "plain"], input), 1);
    }
    assert_error(invoke(&[], " \n"), 1);
}

#[test]
fn empty_file_exits_one() {
    let file = TempInput::new();
    fs::write(&file.0, " \t\n").unwrap();
    assert_error(invoke(&["--print", "plain", file.0.to_str().unwrap()], ""), 1);
}

#[test]
fn invalid_arguments_exit_three() {
    for args in [
        vec!["--unknown"],
        vec!["--print"],
        vec!["--print", "html"],
        vec!["--print", "invalid"],
        vec!["--plain", "--html"],
        vec!["--print", "plain", "--markdown"],
        vec!["one.md", "two.md"],
    ] {
        assert_error(invoke(&args, ""), 3);
    }
}

#[test]
fn clipboard_modes_explicitly_report_not_implemented() {
    for args in [vec![], vec!["--plain"], vec!["--html"], vec!["--markdown"]] {
        let output = invoke(&args, "Hello clipboard\n");
        assert!(String::from_utf8_lossy(&output.stderr).contains("not implemented"));
        assert_error(output, 2);
    }
}

#[test]
fn missing_file_exits_two() {
    let file = TempInput::new();
    fs::remove_file(&file.0).unwrap();
    assert_error(invoke(&["--print", "plain", file.0.to_str().unwrap()], ""), 2);
}

#[test]
fn invalid_utf8_file_exits_two() {
    let file = TempInput::new();
    fs::write(&file.0, [0xff, 0xfe]).unwrap();
    assert_error(invoke(&["--print", "plain", file.0.to_str().unwrap()], ""), 2);
}

#[test]
fn help_and_version_exit_zero() {
    for args in [vec!["--help"], vec!["-h"], vec!["--version"], vec!["-V"]] {
        let output = invoke(&args, "");
        assert!(output.status.success());
        assert!(!output.stdout.is_empty());
        assert!(output.stderr.is_empty());
    }
}
