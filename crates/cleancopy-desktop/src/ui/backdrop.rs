//! Menu backdrop strategy. The design uses `rgba(255,255,255,.92)` with a
//! 30px backdrop blur. Each platform maps that to its native material:
//!
//! * macOS: `NSVisualEffectView` (material `.menu`, blending `.behindWindow`).
//! * Windows 11: Mica via `DwmSetWindowAttribute(DWMWA_SYSTEMBACKDROP_TYPE)`.
//! * Linux/X11: no portable compositor blur, so a solid fill is used.
//!
//! Only the Linux fallback is built and verified in this repository; the
//! macOS and Windows hooks are not implemented yet (see docs/visual-diff.md).
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
    /// Panel fill to paint over the backdrop. Until the native hooks are
    /// wired, every platform paints opaque white so text never sits on an
    /// unblurred desktop.
    pub const fn panel_fill(self) -> Color32 {
        match self {
            Backdrop::VisualEffect | Backdrop::Mica | Backdrop::Solid => Color32::WHITE,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[cfg(target_os = "linux")]
    fn linux_uses_the_solid_fallback() {
        assert_eq!(Backdrop::for_platform(), Backdrop::Solid);
        assert_eq!(Backdrop::for_platform().panel_fill(), Color32::WHITE);
    }
}
