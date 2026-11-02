//! Runtime system-font discovery; no system font is bundled with the app.
use eframe::egui;
use std::path::{Path, PathBuf};

/// Real semibold/bold faces (600/700), not letter-spacing as synthetic emphasis.
pub fn bold_family() -> egui::FontFamily {
    egui::FontFamily::Name("system-bold".into())
}

pub fn install(ctx: &egui::Context) -> Option<PathBuf> {
    let mut candidates = vec![
        PathBuf::from("/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc"),
        PathBuf::from("/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc"),
        PathBuf::from("/System/Library/Fonts/PingFang.ttc"),
        PathBuf::from("C:/Windows/Fonts/msyh.ttc"),
    ];
    discover(Path::new("/usr/share/fonts"), &mut candidates);
    discover(Path::new("/System/Library/Fonts"), &mut candidates);
    discover(Path::new("/Library/Fonts"), &mut candidates);
    discover(Path::new("C:/Windows/Fonts"), &mut candidates);
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        discover(&home.join(".local/share/fonts"), &mut candidates);
        discover(&home.join("Library/Fonts"), &mut candidates);
    }
    let mut fonts = egui::FontDefinitions::default();
    let mut symbols = vec![
        PathBuf::from("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"),
        PathBuf::from("/usr/share/fonts/dejavu/DejaVuSans.ttf"),
        PathBuf::from("/System/Library/Fonts/Apple Symbols.ttf"),
        PathBuf::from("C:/Windows/Fonts/seguisym.ttf"),
    ];
    discover_symbols(Path::new("/usr/share/fonts"), &mut symbols);
    if let Some(home) = std::env::var_os("HOME") {
        discover_symbols(
            &PathBuf::from(home).join(".local/share/fonts"),
            &mut symbols,
        );
    }
    // DejaVu covers all menu symbols, while Noto Symbols can supplement it.
    // Append these after the regular fonts so normal UI typography stays intact.
    let mut seen = std::collections::HashSet::new();
    symbols.retain(|path| seen.insert(path.clone()));
    for (index, path) in symbols.iter().enumerate() {
        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };
        let name = format!("system-symbols-{index}");
        fonts
            .font_data
            .insert(name.clone(), egui::FontData::from_owned(bytes).into());
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            fonts.families.entry(family).or_default().push(name.clone());
        }
    }
    let mut bold_candidates = vec![
        PathBuf::from("/usr/share/fonts/opentype/noto/NotoSansCJK-Bold.ttc"),
        PathBuf::from("/usr/share/fonts/noto-cjk/NotoSansCJK-Bold.ttc"),
        PathBuf::from("/System/Library/Fonts/Supplemental/Arial Bold.ttf"),
        PathBuf::from("C:/Windows/Fonts/msyhbd.ttc"),
        PathBuf::from("C:/Windows/Fonts/arialbd.ttf"),
    ];
    bold_candidates.extend(candidates.iter().filter(|path| is_bold(path)).cloned());
    bold_candidates.extend([
        PathBuf::from("/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf"),
        PathBuf::from("/usr/share/fonts/dejavu/DejaVuSans-Bold.ttf"),
    ]);
    let mut seen_bold = std::collections::HashSet::new();
    bold_candidates.retain(|path| seen_bold.insert(path.clone()));
    let mut bold_names = Vec::new();
    for (index, path) in bold_candidates.iter().enumerate() {
        if let Ok(bytes) = std::fs::read(path) {
            let name = format!("system-bold-{index}");
            fonts
                .font_data
                .insert(name.clone(), egui::FontData::from_owned(bytes).into());
            bold_names.push(name);
        }
    }
    if bold_names.is_empty() {
        eprintln!("No system bold font found; Word preview emphasis uses regular fallback");
    }
    let mut loaded_cjk = None;
    for path in candidates.into_iter().filter(|path| !is_bold(path)) {
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        fonts.font_data.insert(
            "system-cjk".into(),
            egui::FontData::from_owned(bytes).into(),
        );
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            fonts
                .families
                .entry(family)
                .or_default()
                .insert(0, "system-cjk".into());
        }
        loaded_cjk = Some(path);
        break;
    }
    bold_names.extend(fonts.families[&egui::FontFamily::Proportional].clone());
    fonts.families.insert(bold_family(), bold_names);
    ctx.set_fonts(fonts);
    if loaded_cjk.is_none() {
        eprintln!("No system CJK font found; using egui fallback fonts");
    }
    loaded_cjk
}

fn is_bold(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            let name = name.to_ascii_lowercase();
            name.contains("bold") || name.contains("semibold") || name.contains("demi")
        })
}

fn discover(directory: &Path, output: &mut Vec<PathBuf>) {
    discover_matching(directory, output, false);
}

fn discover_symbols(directory: &Path, output: &mut Vec<PathBuf>) {
    discover_matching(directory, output, true);
}

fn discover_matching(directory: &Path, output: &mut Vec<PathBuf>, symbols: bool) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().map(|entry| entry.path()).collect();
    entries.sort();
    for path in entries {
        if path.is_dir() && !path.is_symlink() {
            discover_matching(&path, output, symbols);
        } else if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| {
                let name = name.to_ascii_lowercase();
                let matches = if symbols {
                    name == "dejavusans.ttf" || name.contains("notosanssymbols")
                } else {
                    name.contains("notosanscjk")
                        || name.contains("notosanssc")
                        || (is_bold(&path)
                            && [
                                "pingfang",
                                "hiragino",
                                "arial",
                                "dejavusans",
                                "liberationsans",
                                "msyh",
                                "simsun",
                                "heiti",
                            ]
                            .iter()
                            .any(|family| name.contains(family)))
                };
                matches && ["ttc", "ttf", "otf"].iter().any(|ext| name.ends_with(ext))
            })
        {
            output.push(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cjk_emphasis_uses_distinct_rasterized_bold_glyphs_when_installed() {
        // This is a native-font integration check, not a requirement to install
        // Linux font packages on macOS/Windows test runners.
        if !Path::new("/usr/share/fonts/opentype/noto/NotoSansCJK-Bold.ttc").exists() {
            return;
        }
        let ctx = egui::Context::default();
        install(&ctx);
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            ctx.fonts(|fonts| {
                let normal = fonts.layout_no_wrap(
                    "剪贴板监听".into(),
                    egui::FontId::proportional(24.0),
                    egui::Color32::BLACK,
                );
                let bold = fonts.layout_no_wrap(
                    "剪贴板监听".into(),
                    egui::FontId::new(24.0, bold_family()),
                    egui::Color32::BLACK,
                );
                let normal_glyphs: Vec<_> =
                    normal.rows.iter().flat_map(|row| &row.glyphs).collect();
                let bold_glyphs: Vec<_> = bold.rows.iter().flat_map(|row| &row.glyphs).collect();
                assert_eq!(normal_glyphs.len(), 5);
                assert_eq!(bold_glyphs.len(), 5);
                for (normal, bold) in normal_glyphs.iter().zip(&bold_glyphs) {
                    assert_ne!(
                        normal.uv_rect, bold.uv_rect,
                        "bold must resolve to a different rasterized glyph, not regular fallback"
                    );
                }
            });
        });
    }
}
