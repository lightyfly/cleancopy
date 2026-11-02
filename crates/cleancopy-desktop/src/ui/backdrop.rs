//! Menu backdrop strategy. The design uses `rgba(255,255,255,.92)` with a
//! 30px backdrop blur. Each platform maps that to its native material:
//!
//! * macOS: `NSVisualEffectView` (material `.menu`, blending `.behindWindow`).
//! * Windows 11: Mica via `DwmSetWindowAttribute(DWMWA_SYSTEMBACKDROP_TYPE)`.
//! * Linux/X11: no portable compositor blur, so a solid fill is used.
//!
//! The macOS and Windows hooks go through the `window-vibrancy` crate and are
//! only compile-checked from Linux (`cargo check --target ...`); they have
//! not been run on a real macOS or Windows desktop (see docs/visual-diff.md).
use eframe::egui::Color32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backdrop {
    /// macOS NSVisualEffectView behind a translucent panel.
    VisualEffect,
    /// Windows 11 Mica system backdrop.
    Mica,
    /// Opaque fill; used where no native blur is available.
    Solid,
}

impl Backdrop {
    pub const fn for_platform() -> Self {
        if cfg!(target_os = "macos") {
            Backdrop::VisualEffect
        } else if cfg!(target_os = "windows") {
            Backdrop::Mica
        } else {
            Backdrop::Solid
        }
    }
    /// Panel fill to paint over the backdrop: the design's
    /// `rgba(255,255,255,.92)` over a native material, opaque white otherwise.
    pub const fn panel_fill(self) -> Color32 {
        match self {
            Backdrop::VisualEffect | Backdrop::Mica => {
                Color32::from_rgba_premultiplied(235, 235, 235, 235)
            }
            Backdrop::Solid => Color32::WHITE,
        }
    }
    /// Whether the window must be created transparent so the material shows.
    pub const fn needs_transparent_window(self) -> bool {
        !matches!(self, Backdrop::Solid)
    }
}

/// Applies the native material to a freshly created eframe window.
/// Returns `Err` with the platform error text when the OS refuses it.
#[cfg(target_os = "macos")]
pub fn apply(window: &impl raw_window_handle::HasWindowHandle) -> Result<(), String> {
    use window_vibrancy::{apply_vibrancy, NSVisualEffectMaterial, NSVisualEffectState};
    apply_vibrancy(
        window,
        NSVisualEffectMaterial::Menu,
        Some(NSVisualEffectState::Active),
        Some(12.0),
    )
    .map_err(|e| e.to_string())
}

/// Applies the native material to a freshly created eframe window.
/// Returns `Err` with the platform error text when the OS refuses it
/// (Mica needs Windows 11 build 22000+).
#[cfg(target_os = "windows")]
pub fn apply(window: &impl raw_window_handle::HasWindowHandle) -> Result<(), String> {
    window_vibrancy::apply_mica(window, Some(false)).map_err(|e| e.to_string())
}

/// No native material on Linux/X11; the solid fill is used.
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn apply<W>(_window: &W) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_material_platforms_use_transparent_translucent_window() {
        let backdrop = Backdrop::for_platform();
        let native = cfg!(any(target_os = "macos", target_os = "windows"));
        assert_eq!(backdrop.needs_transparent_window(), native);
        assert_eq!(backdrop.panel_fill().a() < 255, native);
    }
    #[test]
    #[cfg(target_os = "linux")]
    fn linux_uses_the_solid_fallback() {
        assert_eq!(Backdrop::for_platform(), Backdrop::Solid);
        assert_eq!(Backdrop::for_platform().panel_fill(), Color32::WHITE);
    }
}
