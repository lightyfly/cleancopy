use cleancopy_core::NumberingStyle;
use cleancopy_desktop::config::{
    AppIdentity, AppRule, Config, ConfigError, Fallback, Format, RecentPaste, SecondaryItem,
};

fn identity(bundle: Option<&str>, exe: Option<&str>, class: Option<&str>) -> AppIdentity {
    AppIdentity {
        name: "display name is never an identifier".into(),
        bundle_id: bundle.map(str::to_owned),
        exe: exe.map(str::to_owned),
        wm_class: class.map(str::to_owned),
    }
}

#[test]
fn defaults_and_platform_identities() {
    let config = Config::default();
    assert!(config.silent_mode);
    assert_eq!(config.numbering_style, NumberingStyle::Chinese);
    assert_eq!(config.fallback, Fallback::Popup);
    assert_eq!(
        config
            .secondary_items
            .iter()
            .map(|item| (item.format, item.enabled))
            .collect::<Vec<_>>(),
        vec![
            (Format::ChineseNumbered, true),
            (Format::WordRichText, true),
            (Format::Markdown, true),
            (Format::PlainStripped, true),
            (Format::HtmlSource, false),
        ]
    );
    assert_eq!(
        config.hotkey,
        if cfg!(target_os = "macos") {
            "Super+Shift+V"
        } else {
            "Ctrl+Shift+V"
        }
    );
    let cases = [
        (
            identity(Some("COM.APPLE.TEXTEDIT"), None, None),
            Format::ChineseNumbered,
        ),
        (
            identity(None, Some(r"C:\Windows\System32\NOTEPAD.EXE"), None),
            Format::ChineseNumbered,
        ),
        (
            identity(None, Some("/usr/bin/gedit"), None),
            Format::ChineseNumbered,
        ),
        (
            identity(None, None, Some("org.gnome.TextEditor")),
            Format::ChineseNumbered,
        ),
        (
            identity(Some("com.microsoft.Word"), None, None),
            Format::WordRichText,
        ),
        (
            identity(None, Some(r"C:\Office\WINWORD.EXE"), None),
            Format::WordRichText,
        ),
        (
            identity(None, None, Some("winword.exe")),
            Format::WordRichText,
        ),
        (
            identity(Some("com.kingsoft.wpsoffice.mac"), None, None),
            Format::WordRichText,
        ),
        (
            identity(None, Some(r"C:\WPS\wps.exe"), None),
            Format::WordRichText,
        ),
        (
            identity(None, Some("/opt/kingsoft/wps"), None),
            Format::WordRichText,
        ),
        (
            identity(None, None, Some("kingsoft-wps")),
            Format::WordRichText,
        ),
        (identity(Some("notion.id"), None, None), Format::Markdown),
        (
            identity(None, Some(r"C:\Users\person\Notion.exe"), None),
            Format::Markdown,
        ),
        (identity(None, None, Some("Notion")), Format::Markdown),
        (identity(Some("md.obsidian"), None, None), Format::Markdown),
        (
            identity(None, Some(r"C:\Apps\Obsidian.exe"), None),
            Format::Markdown,
        ),
        (identity(None, None, Some("obsidian")), Format::Markdown),
    ];
    for (app, expected) in cases {
        assert_eq!(config.format_for(&app), Some(expected), "{app:?}");
    }
}

#[test]
fn matching_is_exact_never_display_name_or_substring() {
    let config = Config::default();
    for app in [
        AppIdentity {
            name: "Microsoft Word".into(),
            ..AppIdentity::default()
        },
        identity(Some("com.microsoft.Word.evil"), None, None),
        identity(None, Some("my-notepad.exe"), None),
        identity(None, Some("winword.exe.backup"), None),
        identity(None, None, Some("Notion-helper")),
        identity(None, Some("/opt/obsidian/other-program"), None),
        identity(Some(""), Some(""), Some("")),
    ] {
        assert_eq!(config.format_for(&app), None, "{app:?}");
    }
    let mut fixed = config;
    fixed.fallback = Fallback::Fixed(Format::PlainStripped);
    assert_eq!(
        fixed.format_for(&AppIdentity::default()),
        Some(Format::PlainStripped)
    );
    assert_eq!(
        fixed.format_for(&identity(Some("notion.id"), None, None)),
        Some(Format::Markdown)
    );
}

#[test]
fn remember_prefers_specific_ids_without_rewriting_broad_rules() {
    let mut config = Config {
        app_rules: vec![],
        ..Config::default()
    };
    let broad = identity(None, Some("shared.exe"), None);
    config.remember(&broad, Format::ChineseNumbered);
    let specific = identity(
        Some("vendor.editor"),
        Some(r"C:\Apps\SHARED.EXE"),
        Some("Editor"),
    );
    config.remember(&specific, Format::Markdown);
    assert_eq!(config.app_rules.len(), 2);
    assert_eq!(config.format_for(&specific), Some(Format::Markdown));
    assert_eq!(config.format_for(&broad), Some(Format::ChineseNumbered));
    config.remember(
        &identity(Some("VENDOR.EDITOR"), None, None),
        Format::HtmlSource,
    );
    assert_eq!(config.app_rules.len(), 2);
    assert_eq!(config.format_for(&specific), Some(Format::HtmlSource));
    config.remember(
        &identity(None, Some("/opt/shared.exe"), None),
        Format::WordRichText,
    );
    assert_eq!(config.app_rules.len(), 2);
    assert_eq!(config.format_for(&broad), Some(Format::WordRichText));
    config.remember(
        &identity(None, None, Some("NewClass")),
        Format::PlainStripped,
    );
    config.remember(&identity(None, None, Some("NEWCLASS")), Format::Markdown);
    assert_eq!(config.app_rules.len(), 3);
    config.remember(
        &AppIdentity {
            name: "Only a name".into(),
            ..AppIdentity::default()
        },
        Format::Markdown,
    );
    assert_eq!(config.app_rules.len(), 3);
}

#[test]
fn remember_does_not_change_other_aliases_in_multi_identifier_rules() {
    let mut config = Config {
        app_rules: vec![AppRule {
            app: identity(Some("vendor.editor"), Some("shared.exe"), None),
            format: Format::Markdown,
        }],
        ..Config::default()
    };
    config.remember(
        &identity(Some("vendor.editor"), None, None),
        Format::HtmlSource,
    );
    assert_eq!(
        config.format_for(&identity(Some("vendor.editor"), None, None)),
        Some(Format::HtmlSource)
    );
    assert_eq!(
        config.format_for(&identity(None, Some("shared.exe"), None)),
        Some(Format::Markdown)
    );
    config.remember(
        &identity(None, Some("shared.exe"), None),
        Format::PlainStripped,
    );
    assert_eq!(
        config.format_for(&identity(Some("vendor.editor"), Some("shared.exe"), None)),
        Some(Format::HtmlSource)
    );
    config.remember(
        &identity(None, Some("Éditeur.exe"), None),
        Format::WordRichText,
    );
    assert_eq!(
        config.format_for(&identity(None, Some(r"C:\Apps\éditeur.exe"), None)),
        Some(Format::WordRichText)
    );
}

#[test]
fn same_millisecond_history_keeps_latest_calls() {
    let mut config = Config::default();
    for format in [Format::Markdown, Format::PlainStripped, Format::HtmlSource] {
        config.record_paste(&AppIdentity::default(), Some(format), 42);
    }
    assert_eq!(
        config
            .recent_pastes
            .iter()
            .map(|paste| paste.format)
            .collect::<Vec<_>>(),
        vec![Some(Format::HtmlSource), Some(Format::PlainStripped)]
    );
}

#[test]
fn atomic_roundtrip_replaces_existing_file_and_creates_parent() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("new-parent/config.toml");
    let mut config = Config::default();
    config.save(&path).unwrap();
    config.fallback = Fallback::Fixed(Format::HtmlSource);
    config.numbering_style = NumberingStyle::ChineseCompact;
    config.paused_until = Some(1_800_000_000_123);
    config.remember(
        &identity(Some("custom.app"), None, None),
        Format::PlainStripped,
    );
    config.record_paste(&identity(None, Some("notepad.exe"), None), None, 100);
    config.record_paste(
        &identity(Some("custom.app"), None, None),
        Some(Format::PlainStripped),
        200,
    );
    config.save(&path).unwrap();
    let loaded = Config::load(&path).unwrap();
    assert_eq!(loaded.numbering_style, NumberingStyle::ChineseCompact);
    assert_eq!(loaded.fallback, config.fallback);
    assert_eq!(loaded.paused_until, config.paused_until);
    assert_eq!(loaded.app_rules, config.app_rules);
    assert_eq!(loaded.recent_pastes, config.recent_pastes);
    assert_eq!(loaded.secondary_items, config.secondary_items);
    assert_eq!(
        std::fs::read_dir(path.parent().unwrap()).unwrap().count(),
        1
    );
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("plain_stripped"));
    assert!(text.contains("html_source"));
    config.numbering_style = NumberingStyle::Decimal;
    config.save(&path).unwrap();
    assert_eq!(
        Config::load(&path).unwrap().numbering_style,
        NumberingStyle::Decimal
    );
}

#[test]
fn history_is_newest_first_and_limited_to_two() {
    let mut config = Config::default();
    let app = AppIdentity::default();
    config.record_paste(&app, None, 20);
    config.record_paste(&app, Some(Format::Markdown), 30);
    config.record_paste(&app, Some(Format::WordRichText), 10);
    assert_eq!(
        config
            .recent_pastes
            .iter()
            .map(|entry| entry.timestamp)
            .collect::<Vec<_>>(),
        vec![30, 20]
    );
}

#[test]
fn load_defaults_and_normalizes_old_config_preserving_menu_order() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("config.toml");
    assert!(Config::load(&path).unwrap().silent_mode);
    std::fs::write(&path, "").unwrap();
    assert!(Config::load(&path).unwrap().silent_mode);
    std::fs::write(
        &path,
        r#"
        silent_mode = false
        [[secondary_items]]
        format = "markdown"
        enabled = false
        [[secondary_items]]
        format = "markdown"
        enabled = true
        [[secondary_items]]
        format = "chinese_numbered"
        enabled = true
    "#,
    )
    .unwrap();
    let loaded = Config::load(&path).unwrap();
    assert!(!loaded.silent_mode);
    assert_eq!(loaded.secondary_items.len(), 5);
    assert_eq!(
        loaded.secondary_items[0],
        SecondaryItem {
            format: Format::Markdown,
            enabled: false
        }
    );
    assert_eq!(loaded.secondary_items[1].format, Format::ChineseNumbered);
    assert!(!loaded.app_rules.is_empty());
}

#[test]
fn loading_and_saving_normalize_oversized_history() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("config.toml");
    let config = Config {
        recent_pastes: [1, 3, 2]
            .into_iter()
            .map(|timestamp| RecentPaste {
                app: AppIdentity::default(),
                format: None,
                timestamp,
            })
            .collect(),
        ..Config::default()
    };
    std::fs::write(&path, toml::to_string(&config).unwrap()).unwrap();
    assert_eq!(
        Config::load(&path)
            .unwrap()
            .recent_pastes
            .iter()
            .map(|entry| entry.timestamp)
            .collect::<Vec<_>>(),
        vec![3, 2]
    );
    config.save(&path).unwrap();
    assert_eq!(Config::load(&path).unwrap().recent_pastes.len(), 2);
}

#[test]
fn invalid_config_and_io_failures_are_errors_not_silent_defaults() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("config.toml");
    for text in [
        "not valid = [",
        "hotkey = ''",
        "numbering_style = 'unknown'",
        "[[app_rules]]\nformat = 'markdown'\n[app_rules.app]\nname = 'Word'",
    ] {
        std::fs::write(&path, text).unwrap();
        assert!(Config::load(&path).is_err(), "{text}");
    }
    assert!(matches!(Config::load(temp.path()), Err(ConfigError::Io(_))));
    let original = "do not replace this invalid file";
    std::fs::write(&path, original).unwrap();
    let config = Config {
        hotkey: String::new(),
        ..Config::default()
    };
    assert!(config.save(&path).is_err());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
    assert!(Config::default().save(temp.path()).is_err());
    let config = Config {
        app_rules: vec![
            AppRule {
                app: identity(None, Some("x"), None),
                format: Format::Markdown
            };
            1025
        ],
        ..Config::default()
    };
    assert!(config.save(&path).is_err());
    let config = Config {
        app_rules: vec![AppRule {
            app: identity(Some(&"x".repeat(4097)), None, None),
            format: Format::Markdown,
        }],
        ..Config::default()
    };
    assert!(config.save(&path).is_err());
}
