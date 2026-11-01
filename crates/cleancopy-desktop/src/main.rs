use clap::Parser;
use cleancopy_desktop::{
    config::{config_path, Config},
    ui::{demo_config, DesktopApp, View},
};
use eframe::egui;

#[derive(Parser)]
#[command(about = "CleanCopy stage B UI (no native hotkey, clipboard, or tray integration)")]
struct Args {
    /// Standalone demo data; edits are session-only and never touch user config.
    #[arg(long, value_enum)]
    preview: Option<View>,
    /// Override the settings file for the normal settings window.
    #[arg(long)]
    config: Option<std::path::PathBuf>,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let view = args.preview.unwrap_or(View::Settings);
    let (config, path) = if args.preview.is_some() {
        (demo_config(), None)
    } else {
        let path = args.config.map(Ok).unwrap_or_else(config_path)?;
        (Config::load(&path)?, Some(path))
    };
    let size = match view {
        View::Settings => egui::vec2(1120.0, 720.0),
        View::Menu | View::MenuSecondary => egui::vec2(960.0, 600.0),
        View::Tray => egui::vec2(720.0, 520.0),
    };
    // Select X11 explicitly on Unix; Wayland support is not compiled in.
    #[cfg(all(unix, not(target_os = "macos")))]
    std::env::set_var("WINIT_UNIX_BACKEND", "x11");
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
