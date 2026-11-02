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
    /// Override the settings file for the daemon or settings window.
    #[arg(long)]
    config: Option<std::path::PathBuf>,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    // Select X11 explicitly; Wayland support is not compiled in.
    #[cfg(all(unix, not(target_os = "macos")))]
    std::env::set_var("WINIT_UNIX_BACKEND", "x11");
    #[cfg(target_os = "linux")]
    if args.preview.is_none() && !args.settings {
        let path = args.config.map(Ok).unwrap_or_else(config_path)?;
        cleancopy_desktop::linux::run(Config::load(&path)?, Some(path))?;
        return Ok(());
    }
    let view = args.preview.unwrap_or(View::Settings);
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
    };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size(size)
            .with_resizable(false)
            .with_decorations(false),
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
