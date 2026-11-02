use clap::Parser;
use cleancopy_desktop::{
    config::{config_path, Config},
    ui::{demo_config, DesktopApp, View},
};
use eframe::egui;

#[derive(Parser)]
#[command(about = "CleanCopy desktop clipboard helper")]
struct Args {
    /// Standalone demo data; edits are session-only and never touch user config.
    #[arg(long, value_enum)]
    preview: Option<View>,
    /// Open settings instead of running the Linux background helper.
    #[arg(long, conflicts_with = "preview")]
    settings: bool,
    /// Open the first-run onboarding card (also offered from the tray).
    #[arg(long, conflicts_with_all = ["preview", "settings"])]
    onboarding: bool,
    /// Override the settings file for the daemon or settings window.
    #[arg(long)]
    config: Option<std::path::PathBuf>,
    /// Run the daemon directly, without the crash-restart supervisor.
    #[arg(long)]
    no_supervisor: bool,
}

/// Set on the supervised daemon child so it does not supervise itself.
#[cfg(target_os = "linux")]
const SUPERVISED_ENV: &str = "CLEANCOPY_SUPERVISED";

/// Keep the daemon alive: restart it when it exits abnormally (panic, signal),
/// with a short backoff, giving up after repeated crashes in a short window so
/// a deterministic startup crash cannot spin forever (report v4 C-1).
#[cfg(target_os = "linux")]
fn supervise() -> Result<(), Box<dyn std::error::Error>> {
    use std::time::{Duration, Instant};
    const MAX_CRASHES: usize = 5;
    const WINDOW: Duration = Duration::from_secs(60);
    let exe = std::env::current_exe()?;
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let mut crashes: Vec<Instant> = Vec::new();
    loop {
        let status = std::process::Command::new(&exe)
            .args(&args)
            .env(SUPERVISED_ENV, "1")
            .status()?;
        if status.success() {
            return Ok(());
        }
        eprintln!("CleanCopy daemon exited abnormally ({status}); restarting");
        let now = Instant::now();
        crashes.retain(|t| now.duration_since(*t) < WINDOW);
        crashes.push(now);
        if crashes.len() >= MAX_CRASHES {
            return Err(
                format!("daemon crashed {MAX_CRASHES} times within 60 s; giving up").into(),
            );
        }
        std::thread::sleep(Duration::from_millis(250 * crashes.len() as u64));
    }
}

/// The supervised child dies with its supervisor, so killing the supervisor
/// never leaves an orphan daemon holding the hotkey grab.
#[cfg(target_os = "linux")]
fn die_with_parent() {
    // SAFETY: prctl(PR_SET_PDEATHSIG) only changes this process's own signal.
    unsafe {
        libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
    }
    // The parent may already have exited before prctl took effect.
    if std::os::unix::process::parent_id() == 1 {
        std::process::exit(0);
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    // Select X11 explicitly; Wayland support is not compiled in.
    #[cfg(all(unix, not(target_os = "macos")))]
    std::env::set_var("WINIT_UNIX_BACKEND", "x11");
    #[cfg(target_os = "linux")]
    if args.preview.is_none() && !args.settings && !args.onboarding {
        if std::env::var_os(SUPERVISED_ENV).is_some() {
            die_with_parent();
        } else if !args.no_supervisor {
            return supervise();
        }
        let path = args.config.map(Ok).unwrap_or_else(config_path)?;
        cleancopy_desktop::linux::run(Config::load(&path)?, Some(path))?;
        return Ok(());
    }
    let view = args.preview.unwrap_or(if args.onboarding {
        View::Onboarding
    } else {
        View::Settings
    });
    let (config, path) = if args.preview.is_some() {
        (demo_config(), None)
    } else {
        let path = args.config.map(Ok).unwrap_or_else(config_path)?;
        (Config::load(&path)?, Some(path))
    };
    let size = match view {
        View::Settings => cleancopy_desktop::ui::settings::DEFAULT_SIZE,
        View::Menu | View::MenuSecondary => egui::vec2(960.0, 600.0),
        View::Tray => egui::vec2(720.0, 520.0),
        View::Onboarding => cleancopy_desktop::ui::onboarding::SIZE,
    };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size(size)
            .with_resizable(false)
            .with_title(match view {
                View::Settings => "CleanCopy 设置",
                View::Onboarding => "欢迎使用 CleanCopy",
                _ => "CleanCopy",
            })
            // Native decorations on Linux/Windows (M-6); macOS draws its own.
            .with_decorations(!cleancopy_desktop::ui::CUSTOM_TITLE_BAR)
            .with_transparent(
                cleancopy_desktop::ui::backdrop::Backdrop::for_platform()
                    .needs_transparent_window(),
            ),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    eframe::run_native(
        "CleanCopy",
        options,
        Box::new(move |cc| Ok(Box::new(DesktopApp::new(cc, view, config, path)))),
    )?;
    Ok(())
}
