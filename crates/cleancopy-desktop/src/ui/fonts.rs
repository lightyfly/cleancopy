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
    for path in candidates {
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let mut fonts = egui::FontDefinitions::default();
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
        ctx.set_fonts(fonts);
        return Some(path);
    }
    eprintln!("No system CJK font found; using egui fallback fonts");
    None
}

fn discover(directory: &Path, output: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().map(|entry| entry.path()).collect();
    entries.sort();
    for path in entries {
        if path.is_dir() && !path.is_symlink() {
            discover(&path, output);
        } else if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| {
                let name = name.to_ascii_lowercase();
                (name.contains("notosanscjk") || name.contains("notosanssc"))
                    && ["ttc", "ttf", "otf"].iter().any(|ext| name.ends_with(ext))
            })
        {
            output.push(path);
        }
    }
}
