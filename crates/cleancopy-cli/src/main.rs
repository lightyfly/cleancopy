use clap::{ArgGroup, Parser, Subcommand, ValueEnum};
use cleancopy_clipboard::{ClipboardWriter, NativeClipboard, Payload, WriteMode};
use std::ffi::OsStr;
use std::io::{self, IsTerminal, Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Debug, Clone, Copy, ValueEnum)]
enum PrintFormat {
    Plain,
    Html,
    Markdown,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// List available clipboard formats and their sizes in bytes
    Inspect,
}

#[derive(Debug, Parser)]
#[command(name = "cleancopy", version, about = "Render Markdown for copying")]
#[command(args_conflicts_with_subcommands = true)]
#[command(group(ArgGroup::new("mode").args(["print", "plain", "html", "markdown"])))]
struct Args {
    /// Print rendered text instead of writing to the clipboard
    #[arg(long, value_enum)]
    print: Option<PrintFormat>,
    /// Copy only rendered plain text
    #[arg(long)]
    plain: bool,
    /// Copy HTML with a plain-text fallback
    #[arg(long)]
    html: bool,
    /// Copy original Markdown through the plain-text channel
    #[arg(long)]
    markdown: bool,
    /// Read a file or '-' for stdin; otherwise use piped stdin or the clipboard
    file: Option<PathBuf>,
    #[command(subcommand)]
    command: Option<Command>,
    #[cfg(target_os = "linux")]
    #[arg(long, hide = true)]
    clipboard_holder: bool,
}

fn clipboard_error(error: impl std::fmt::Display) -> (u8, String) {
    (2, error.to_string())
}

fn run(args: Args) -> Result<(), (u8, String)> {
    #[cfg(target_os = "linux")]
    if args.clipboard_holder {
        return cleancopy_clipboard::run_holder().map_err(clipboard_error);
    }

    if let Some(Command::Inspect) = args.command {
        let clipboard = NativeClipboard::new().map_err(clipboard_error)?;
        let mut stdout = io::stdout().lock();
        for format in clipboard.list_formats().map_err(clipboard_error)? {
            writeln!(stdout, "{}\t{} bytes", format.name, format.bytes)
                .map_err(|error| (2, format!("cannot write stdout: {error}")))?;
        }
        return Ok(());
    }

    let input = match args.file {
        Some(path) if path.as_os_str() != OsStr::new("-") => std::fs::read_to_string(&path)
            .map_err(|error| (2, format!("cannot read {}: {error}", path.display())))?,
        None if io::stdin().is_terminal() => NativeClipboard::new()
            .and_then(|clipboard| clipboard.read_text())
            .map_err(clipboard_error)?,
        _ => {
            let mut input = String::new();
            io::stdin()
                .read_to_string(&mut input)
                .map_err(|error| (2, format!("cannot read stdin: {error}")))?;
            input
        }
    };

    if input.trim().is_empty() {
        return Err((1, "input is empty or whitespace only".into()));
    }

    if let Some(format) = args.print {
        let output = match format {
            PrintFormat::Plain => cleancopy_core::render_plain(&input),
            PrintFormat::Html => cleancopy_core::render_html(&input),
            PrintFormat::Markdown => cleancopy_core::render_markdown(&input),
        };
        return io::stdout()
            .lock()
            .write_all(output.as_bytes())
            .map_err(|error| (2, format!("cannot write stdout: {error}")));
    }

    let payload = Payload {
        html: cleancopy_core::render_html(&input),
        plain: cleancopy_core::render_plain(&input),
        markdown: cleancopy_core::render_markdown(&input),
    };
    let mode = if args.plain {
        WriteMode::Plain
    } else if args.html {
        WriteMode::Html
    } else if args.markdown {
        WriteMode::Markdown
    } else {
        WriteMode::Multi
    };

    #[cfg(target_os = "linux")]
    {
        let executable = std::env::current_exe().map_err(clipboard_error)?;
        cleancopy_clipboard::spawn_holder(&executable, &payload, mode).map_err(clipboard_error)
    }
    #[cfg(not(target_os = "linux"))]
    NativeClipboard::new()
        .and_then(|clipboard| clipboard.write_mode(&payload, mode))
        .map_err(clipboard_error)
}

fn main() -> ExitCode {
    let args = match Args::try_parse() {
        Ok(args) => args,
        Err(error) => {
            let code = if error.use_stderr() { 3 } else { 0 };
            let _ = error.print();
            return ExitCode::from(code);
        }
    };

    match run(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err((code, message)) => {
            eprintln!("cleancopy: {message}");
            ExitCode::from(code)
        }
    }
}
