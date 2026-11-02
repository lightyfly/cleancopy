//! Shared text colours. Body text is #2B2B33 (lighter than the mock's #1D1D1F
//! so egui's coverage-boosted CJK glyphs do not read as bold); secondary and
//! muted text are chosen so every pairing in [`BACKGROUNDS`] meets WCAG 2.x AA
//! (4.5:1 for normal-size text).
use eframe::egui::Color32;

pub const TEXT: Color32 = Color32::from_rgb(0x2B, 0x2B, 0x33);
pub const SUB: Color32 = Color32::from_rgb(98, 98, 106);
pub const MUTED: Color32 = Color32::from_rgb(106, 106, 114);
pub const ACCENT: Color32 = Color32::from_rgb(91, 91, 214);

/// Backgrounds text is drawn on, with names for reports.
pub const BACKGROUNDS: [(&str, Color32); 6] = [
    ("white panel", Color32::WHITE),
    ("preview pane #FAFAFB", Color32::from_rgb(250, 250, 251)),
    ("key cap #F7F7F8", Color32::from_rgb(247, 247, 248)),
    ("segmented track #F0F0F3", Color32::from_rgb(240, 240, 243)),
    (
        "selected row (accent 8% on white)",
        Color32::from_rgb(240, 240, 251),
    ),
    (
        "tray/menu backdrop #EDEDF2",
        Color32::from_rgb(237, 237, 242),
    ),
];

fn channel(v: u8) -> f64 {
    let v = f64::from(v) / 255.0;
    if v <= 0.039_28 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}
/// WCAG relative luminance.
pub fn luminance(c: Color32) -> f64 {
    0.2126 * channel(c.r()) + 0.7152 * channel(c.g()) + 0.0722 * channel(c.b())
}
/// WCAG contrast ratio (1.0–21.0).
pub fn contrast(a: Color32, b: Color32) -> f64 {
    let (x, y) = (luminance(a), luminance(b));
    let (hi, lo) = if x > y { (x, y) } else { (y, x) };
    (hi + 0.05) / (lo + 0.05)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contrast_formula_matches_known_values() {
        assert!((contrast(Color32::BLACK, Color32::WHITE) - 21.0).abs() < 1e-9);
        assert!((contrast(Color32::WHITE, Color32::WHITE) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn text_colours_meet_wcag_aa_on_every_background() {
        for (name, colour) in [("TEXT", TEXT), ("SUB", SUB), ("MUTED", MUTED)] {
            for (bg_name, bg) in BACKGROUNDS {
                let ratio = contrast(colour, bg);
                println!("{name} on {bg_name}: {ratio:.2}:1");
                assert!(ratio >= 4.5, "{name} on {bg_name} = {ratio:.2}");
            }
        }
        let on_accent = contrast(Color32::WHITE, ACCENT);
        println!("white on ACCENT: {on_accent:.2}:1");
        assert!(on_accent >= 4.5);
        // Hierarchy: body darker than secondary, secondary darker than muted.
        assert!(contrast(TEXT, Color32::WHITE) > contrast(SUB, Color32::WHITE));
        assert!(contrast(SUB, Color32::WHITE) > contrast(MUTED, Color32::WHITE));
    }
}
