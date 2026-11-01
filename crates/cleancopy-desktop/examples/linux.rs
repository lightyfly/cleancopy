//! Opt-in X11 engine/menu runner; pass an optional config TOML path.
#[cfg(target_os = "linux")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use cleancopy_desktop::{config::Config, linux};
    let path = std::env::args_os().nth(1).map(std::path::PathBuf::from);
    let config = match &path {
        Some(path) => Config::load(path)?,
        None => Config::default(),
    };
    linux::run(config, path)?;
    Ok(())
}
#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("The native Linux runner requires X11 on Linux.");
}
