//! Runtime system-font discovery; no system font is bundled with the app.
use eframe::egui;
use std::path::{Path, PathBuf};

pub fn install(ctx: &egui::Context) -> Option<PathBuf> {
    let mut candidates = vec![
        PathBuf::from("/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc"),
        PathBuf::from("/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc"),
        PathBuf::from("/System/Library/Fonts/PingFang.ttc"),
        PathBuf::from("C:/Windows/Fonts/msyh.ttc"),
    ];
    discover(Path::new("/usr/share/fonts"), &mut candidates);
    if let Some(home) = std::env::var_os("HOME") {
        discover(
            &PathBuf::from(home).join(".local/share/fonts"),
            &mut candidates,
        );
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
    let mut loaded_cjk = None;
    for path in candidates {
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
    ctx.set_fonts(fonts);
    if loaded_cjk.is_none() {
        eprintln!("No system CJK font found; using egui fallback fonts");
    }
    loaded_cjk
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
                    name.contains("notosanscjk") || name.contains("notosanssc")
                };
                matches && ["ttc", "ttf", "otf"].iter().any(|ext| name.ends_with(ext))
            })
        {
            output.push(path);
        }
    }
}
