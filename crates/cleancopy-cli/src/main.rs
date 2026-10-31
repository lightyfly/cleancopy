use clap::{ArgGroup, Parser, ValueEnum};
use cleancopy_clipboard::{ClipboardWriter, Payload, StubClipboard};
use std::ffi::OsStr;
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Debug, Clone, Copy, ValueEnum)]
enum PrintFormat {
    Plain,
    Markdown,
}

#[derive(Debug, Parser)]
#[command(name = "cleancopy", version, about = "Render Markdown for copying")]
#[command(group(ArgGroup::new("mode").args(["print", "plain", "html", "markdown"])))]
struct Args {
    /// Print rendered text instead of writing to the clipboard
    #[arg(long, value_enum)]
    print: Option<PrintFormat>,
    /// Copy plain text (not implemented yet)
    #[arg(long)]
    plain: bool,
    /// Copy HTML (not implemented yet)
    #[arg(long)]
    html: bool,
    /// Copy Markdown (not implemented yet)
    #[arg(long)]
    markdown: bool,
    /// Read from a file, or '-' for stdin; defaults to stdin
    file: Option<PathBuf>,
}

fn run(args: Args) -> Result<(), (u8, String)> {
    let input = match args.file {
        Some(path) if path.as_os_str() != OsStr::new("-") => std::fs::read_to_string(&path)
            .map_err(|error| (2, format!("cannot read {}: {error}", path.display())))?,
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

    let output = match args.print {
        Some(PrintFormat::Plain) => cleancopy_core::render_plain(&input),
        Some(PrintFormat::Markdown) => cleancopy_core::render_markdown(&input),
        None => {
            return StubClipboard
                .write(&Payload::default())
                .map_err(|error| (2, error.to_string()));
        }
    };

    io::stdout()
        .lock()
        .write_all(output.as_bytes())
        .map_err(|error| (2, format!("cannot write stdout: {error}")))
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
