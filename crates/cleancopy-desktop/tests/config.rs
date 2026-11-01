use std::error::Error;

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

fn formats() -> [Format; 5] {
    [
        Format::ChineseNumbered,
        Format::WordRichText,
        Format::Markdown,
        Format::PlainStripped,
        Format::HtmlSource,
    ]
}

fn assert_same_config(actual: &Config, expected: &Config) {
    assert_eq!(actual.silent_mode, expected.silent_mode);
    assert_eq!(actual.numbering_style, expected.numbering_style);
    assert_eq!(actual.app_rules, expected.app_rules);
    assert_eq!(actual.fallback, expected.fallback);
    assert_eq!(actual.secondary_items, expected.secondary_items);
    assert_eq!(actual.hotkey, expected.hotkey);
    assert_eq!(actual.paused_until, expected.paused_until);
    assert_eq!(actual.recent_pastes, expected.recent_pastes);
}

#[test]
fn default_toml_roundtrip_preserves_every_field_and_silent_override() {
    let config = Config::default();
    let text = toml::to_string_pretty(&config).unwrap();
    assert!(text.contains("silent_mode = true"));
    let decoded: Config = toml::from_str(&text).unwrap();
    assert_same_config(&decoded, &config);
    assert!(decoded.paused_until.is_none());
    assert!(decoded.recent_pastes.is_empty());
}

#[test]
fn missing_empty_and_partial_configs_use_complete_defaults() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("missing/subdir/config.toml");
    let defaults = Config::default();
    assert_same_config(&Config::load(&path).unwrap(), &defaults);
    assert!(!path.parent().unwrap().exists());
    assert_same_config(&toml::from_str::<Config>("").unwrap(), &defaults);
    for text in ["", "silent_mode = true", "# old configuration\n"] {
        let path = temp.path().join("config.toml");
        std::fs::write(&path, text).unwrap();
        assert_same_config(&Config::load(&path).unwrap(), &defaults);
    }
    let config: Config = toml::from_str("silent_mode = false").unwrap();
    assert!(!config.silent_mode);
    assert_eq!(config.app_rules, defaults.app_rules);
}

#[test]
fn all_formats_styles_and_fallbacks_roundtrip_through_disk() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("config.toml");
    for style in [
        NumberingStyle::Chinese,
        NumberingStyle::ChineseCompact,
        NumberingStyle::Decimal,
    ] {
        for format in formats() {
            for fallback in [Fallback::Popup, Fallback::Fixed(format)] {
                let mut config = Config {
                    silent_mode: false,
                    numbering_style: style,
                    fallback,
                    hotkey: "Alt+Shift+V".into(),
                    paused_until: Some(0),
                    app_rules: vec![],
                    ..Config::default()
                };
                config.remember(&identity(Some("example.editor"), None, None), format);
                config.record_paste(&AppIdentity::default(), None, 0);
                config.record_paste(&AppIdentity::default(), Some(format), 1);
                config.save(&path).unwrap();
                assert_same_config(&Config::load(&path).unwrap(), &config);
            }
        }
    }
}

#[test]
fn remembering_each_identifier_survives_restart_and_reselection() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("config.toml");
    for (app, probe) in [
        (
            identity(Some("vendor.editor"), Some("shared.exe"), Some("Editor")),
            identity(Some("VENDOR.EDITOR"), None, None),
        ),
        (
            identity(None, Some(r"C:\Apps\Editor.EXE"), Some("Editor")),
            identity(None, Some("/opt/editor.exe"), None),
        ),
        (
            identity(None, None, Some("EditorClass")),
            identity(None, None, Some("EDITORCLASS")),
        ),
    ] {
        let mut config = Config {
            app_rules: vec![],
            ..Config::default()
        };
        config.remember(&app, Format::PlainStripped);
        config.save(&path).unwrap();
        let mut loaded = Config::load(&path).unwrap();
        assert_eq!(loaded.format_for(&probe), Some(Format::PlainStripped));
        assert_eq!(loaded.app_rules.len(), 1);
        assert_eq!(loaded.app_rules[0].app.name, app.name);
        assert_eq!(
            [
                loaded.app_rules[0].app.bundle_id.is_some(),
                loaded.app_rules[0].app.exe.is_some(),
                loaded.app_rules[0].app.wm_class.is_some()
            ]
            .into_iter()
            .filter(|present| *present)
            .count(),
            1
        );
        loaded.remember(&probe, Format::HtmlSource);
        loaded.save(&path).unwrap();
        let reloaded = Config::load(&path).unwrap();
        assert_eq!(reloaded.app_rules.len(), 1);
        assert_eq!(reloaded.format_for(&app), Some(Format::HtmlSource));
    }
}

#[test]
fn remember_existing_rule_wins_over_earlier_multi_identifier_alias() {
    for (alias, specific, other) in [
        (
            identity(Some("vendor.editor"), Some("editor.exe"), None),
            identity(Some("vendor.editor"), None, None),
            identity(None, Some("editor.exe"), None),
        ),
        (
            identity(None, Some("editor.exe"), Some("EditorClass")),
            identity(None, Some("editor.exe"), None),
            identity(None, None, Some("EditorClass")),
        ),
        (
            identity(Some("vendor.editor"), None, Some("EditorClass")),
            identity(None, None, Some("EditorClass")),
            identity(Some("vendor.editor"), None, None),
        ),
    ] {
        let mut config = Config {
            app_rules: vec![
                AppRule {
                    app: alias,
                    format: Format::Markdown,
                },
                AppRule {
                    app: specific.clone(),
                    format: Format::ChineseNumbered,
                },
            ],
            ..Config::default()
        };
        config.remember(&specific, Format::HtmlSource);
        assert_eq!(config.app_rules.len(), 2);
        assert_eq!(config.format_for(&specific), Some(Format::HtmlSource));
        assert_eq!(config.format_for(&other), Some(Format::Markdown));
        config.remember(&specific, Format::PlainStripped);
        assert_eq!(config.app_rules.len(), 2);
        assert_eq!(config.format_for(&specific), Some(Format::PlainStripped));
    }
}

#[test]
fn match_precedence_is_bundle_then_executable_then_class_regardless_of_order() {
    let rules = [
        AppRule {
            app: identity(None, None, Some("Editor")),
            format: Format::PlainStripped,
        },
        AppRule {
            app: identity(None, Some("editor.exe"), None),
            format: Format::Markdown,
        },
        AppRule {
            app: identity(Some("vendor.editor"), None, None),
            format: Format::HtmlSource,
        },
    ];
    for order in [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ] {
        let config = Config {
            app_rules: order
                .into_iter()
                .map(|index| rules[index].clone())
                .collect(),
            fallback: Fallback::Fixed(Format::WordRichText),
            ..Config::default()
        };
        for (app, expected) in [
            (
                identity(Some("vendor.editor"), Some("editor.exe"), Some("Editor")),
                Format::HtmlSource,
            ),
            (
                identity(Some("unknown"), Some("editor.exe"), Some("Editor")),
                Format::Markdown,
            ),
            (
                identity(None, Some("other.exe"), Some("Editor")),
                Format::PlainStripped,
            ),
            (AppIdentity::default(), Format::WordRichText),
        ] {
            assert_eq!(
                config.format_for(&app),
                Some(expected),
                "order {order:?}, app {app:?}"
            );
        }
    }
}

#[test]
fn equal_rank_conflicts_use_first_rule_for_each_identifier_kind() {
    for app in [
        identity(Some("editor.id"), None, None),
        identity(None, Some("editor.exe"), None),
        identity(None, None, Some("Editor")),
    ] {
        let mut config = Config {
            app_rules: vec![
                AppRule {
                    app: app.clone(),
                    format: Format::Markdown,
                },
                AppRule {
                    app: app.clone(),
                    format: Format::PlainStripped,
                },
            ],
            ..Config::default()
        };
        assert_eq!(config.format_for(&app), Some(Format::Markdown));
        config.app_rules.reverse();
        assert_eq!(config.format_for(&app), Some(Format::PlainStripped));
    }
}

#[test]
fn every_builtin_alias_matches_individually_without_using_display_names() {
    let config = Config::default();
    for rule in &config.app_rules {
        let mut app = rule.app.clone();
        app.name = "unrelated display name".into();
        assert!(rule.matches(&app));
        assert_eq!(config.format_for(&app), Some(rule.format));
    }
}

#[test]
fn empty_whitespace_and_path_only_identifiers_never_match_or_get_remembered() {
    let mut config = Config {
        app_rules: vec![],
        ..Config::default()
    };
    for app in [
        AppIdentity::default(),
        identity(Some(" \t\n"), Some(" "), Some("\t")),
        identity(None, Some("/"), None),
        identity(None, Some(r"C:\Apps\"), None),
    ] {
        let rule = AppRule {
            app: app.clone(),
            format: Format::Markdown,
        };
        assert!(!rule.matches(&app));
        config.remember(&app, Format::Markdown);
        assert!(config.app_rules.is_empty());
    }
    config.remember(
        &identity(Some(" "), Some("/"), Some("Editor")),
        Format::Markdown,
    );
    assert_eq!(
        config.app_rules[0].app,
        identity(None, None, Some("Editor"))
    );
}

#[test]
fn identifiers_are_exact_and_executable_paths_are_cross_platform() {
    for exe in [
        "EDITOR.EXE",
        "/opt/apps/EDITOR.EXE",
        r"C:\Program Files\EDITOR.EXE",
        r"C:\Apps/mixed\EDITOR.EXE",
    ] {
        let rule = AppRule {
            app: identity(None, Some(exe), None),
            format: Format::Markdown,
        };
        assert!(rule.matches(&identity(None, Some("editor.exe"), None)));
        for other in [
            "editor",
            "editor.exe.bak",
            "my-editor.exe",
            " editor.exe",
            "editor.exe ",
            "/editor.exe/",
        ] {
            assert!(
                !rule.matches(&identity(None, Some(other), None)),
                "{exe:?} vs {other:?}"
            );
        }
    }
    for (left, right) in [
        ("vendor.editor", "vendor.editor.extra"),
        ("Editor", "Editor-helper"),
        ("Editor", " Editor"),
    ] {
        assert!(!AppRule {
            app: identity(Some(left), None, Some(left)),
            format: Format::Markdown
        }
        .matches(&identity(Some(right), None, Some(right))));
    }
}

#[test]
fn disabled_menu_order_and_all_disabled_state_survive_roundtrip() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("config.toml");
    let mut config = Config::default();
    config.secondary_items.reverse();
    for item in &mut config.secondary_items {
        item.enabled = false;
    }
    config.save(&path).unwrap();
    assert_same_config(&Config::load(&path).unwrap(), &config);
}

#[test]
fn menu_normalization_preserves_first_duplicate_and_appends_missing_defaults() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("config.toml");
    let config = Config {
        secondary_items: vec![
            SecondaryItem {
                format: Format::HtmlSource,
                enabled: true,
            },
            SecondaryItem {
                format: Format::Markdown,
                enabled: false,
            },
            SecondaryItem {
                format: Format::HtmlSource,
                enabled: false,
            },
        ],
        ..Config::default()
    };
    config.save(&path).unwrap();
    assert_eq!(
        config.secondary_items.len(),
        3,
        "save must not mutate its caller"
    );
    let loaded = Config::load(&path).unwrap();
    assert_eq!(
        loaded
            .secondary_items
            .iter()
            .map(|item| (item.format, item.enabled))
            .collect::<Vec<_>>(),
        vec![
            (Format::HtmlSource, true),
            (Format::Markdown, false),
            (Format::ChineseNumbered, true),
            (Format::WordRichText, true),
            (Format::PlainStripped, true),
        ]
    );
    loaded.save(&path).unwrap();
    assert_same_config(&Config::load(&path).unwrap(), &loaded);
}

#[test]
fn explicit_empty_rules_are_not_replaced_with_builtin_rules() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("config.toml");
    std::fs::write(&path, "app_rules = []\nsecondary_items = []").unwrap();
    let config = Config::load(&path).unwrap();
    assert!(config.app_rules.is_empty());
    assert_eq!(config.secondary_items, Config::default().secondary_items);
    config.save(&path).unwrap();
    assert_same_config(&Config::load(&path).unwrap(), &config);
}

#[test]
fn history_ties_and_original_paste_survive_load_save_without_mutating_caller() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("config.toml");
    let app = identity(Some("editor.id"), None, None);
    let config = Config {
        recent_pastes: vec![
            RecentPaste {
                app: app.clone(),
                format: None,
                timestamp: 9,
            },
            RecentPaste {
                app: app.clone(),
                format: Some(Format::Markdown),
                timestamp: 9,
            },
            RecentPaste {
                app,
                format: Some(Format::HtmlSource),
                timestamp: 1,
            },
        ],
        ..Config::default()
    };
    config.save(&path).unwrap();
    assert_eq!(config.recent_pastes.len(), 3);
    let loaded = Config::load(&path).unwrap();
    assert_eq!(loaded.recent_pastes, config.recent_pastes[..2]);
    assert!(loaded.recent_pastes[0].format.is_none());
}

#[test]
fn malformed_fields_are_decode_errors() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("config.toml");
    for text in [
        "silent_mode = 'true'",
        "silent_mode = true\nsilent_mode = false",
        "app_rules = false",
        "recent_pastes = false",
        "paused_until = -1",
        "numbering_style = 'unknown'",
        "fallback = 'unknown'",
        "[fallback]\nfixed = 'unknown'",
        "[[secondary_items]]\nformat = 'markdown'",
        "[[secondary_items]]\nformat = 'unknown'\nenabled = true",
        "[[app_rules]]\nformat = 'unknown'\n[app_rules.app]\nexe = 'editor'",
        "[[app_rules]]\n[app_rules.app]\nexe = 'editor'",
        "[[recent_pastes]]\napp = {}",
        "[[recent_pastes]]\napp = {}\ntimestamp = -1",
    ] {
        std::fs::write(&path, text).unwrap();
        let error = Config::load(&path).unwrap_err();
        assert!(matches!(error, ConfigError::Decode(_)), "{text}: {error}");
        assert!(error.source().is_some());
    }
}

#[test]
fn validation_rejects_bad_identity_fields_on_both_load_and_save() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("config.toml");
    for field in 0..4 {
        for bad in ["x".repeat(4097), "app\0name".into()] {
            let mut app = identity(Some("valid.id"), None, None);
            match field {
                0 => app.name = bad,
                1 => app.bundle_id = Some(bad),
                2 => app.exe = Some(bad),
                _ => app.wm_class = Some(bad),
            }
            for in_history in [false, true] {
                let mut config = Config::default();
                if in_history {
                    config.recent_pastes = vec![RecentPaste {
                        app: app.clone(),
                        format: None,
                        timestamp: 1,
                    }];
                } else {
                    config.app_rules = vec![AppRule {
                        app: app.clone(),
                        format: Format::Markdown,
                    }];
                }
                std::fs::write(&path, toml::to_string(&config).unwrap()).unwrap();
                assert!(matches!(Config::load(&path), Err(ConfigError::Invalid(_))));
                assert!(matches!(config.save(&path), Err(ConfigError::Invalid(_))));
            }
        }
    }
}

#[test]
fn rule_and_text_limits_accept_boundaries_and_reject_excess() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("config.toml");
    let mut config = Config {
        hotkey: "x".repeat(256),
        app_rules: vec![
            AppRule {
                app: identity(Some(&"x".repeat(4096)), None, None),
                format: Format::Markdown
            };
            1024
        ],
        ..Config::default()
    };
    config.save(&path).unwrap();
    assert_same_config(&Config::load(&path).unwrap(), &config);
    config.app_rules.push(config.app_rules[0].clone());
    assert!(matches!(config.save(&path), Err(ConfigError::Invalid(_))));
    std::fs::write(&path, toml::to_string(&config).unwrap()).unwrap();
    assert!(matches!(Config::load(&path), Err(ConfigError::Invalid(_))));
    config.app_rules.clear();
    for hotkey in ["".into(), " \t\n".into(), "x".repeat(257), "é".repeat(129)] {
        config.hotkey = hotkey;
        assert!(matches!(config.save(&path), Err(ConfigError::Invalid(_))));
        std::fs::write(&path, toml::to_string(&config).unwrap()).unwrap();
        assert!(matches!(Config::load(&path), Err(ConfigError::Invalid(_))));
    }
}

#[test]
fn identifierless_rules_are_invalid_but_original_history_can_be_anonymous() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("config.toml");
    for app in [
        identity(None, None, None),
        identity(Some(" "), Some("/"), Some("\t")),
    ] {
        let config = Config {
            app_rules: vec![AppRule {
                app,
                format: Format::Markdown,
            }],
            ..Config::default()
        };
        assert!(matches!(config.save(&path), Err(ConfigError::Invalid(_))));
        std::fs::write(&path, toml::to_string(&config).unwrap()).unwrap();
        assert!(matches!(Config::load(&path), Err(ConfigError::Invalid(_))));
    }
    let mut config = Config::default();
    config.record_paste(&AppIdentity::default(), None, 0);
    config.save(&path).unwrap();
    assert_same_config(&Config::load(&path).unwrap(), &config);
}

#[test]
fn save_failures_preserve_destination_and_clean_temporary_files() {
    let temp = tempfile::tempdir().unwrap();
    let destination = temp.path().join("destination");
    std::fs::create_dir(&destination).unwrap();
    std::fs::write(destination.join("keep"), "original").unwrap();
    assert!(matches!(
        Config::default().save(&destination),
        Err(ConfigError::Io(_))
    ));
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 1);
    assert_eq!(
        std::fs::read_to_string(destination.join("keep")).unwrap(),
        "original"
    );
    let blocker = temp.path().join("file-not-directory");
    std::fs::write(&blocker, "original").unwrap();
    assert!(matches!(
        Config::default().save(blocker.join("config.toml")),
        Err(ConfigError::Io(_))
    ));
    assert_eq!(std::fs::read_to_string(blocker).unwrap(), "original");
}

#[test]
fn invalid_utf8_is_an_io_error_and_error_messages_retain_context() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("config.toml");
    std::fs::write(&path, [0xff, 0xfe]).unwrap();
    let error = Config::load(&path).unwrap_err();
    assert!(matches!(error, ConfigError::Io(_)));
    assert!(error.to_string().contains("configuration I/O error"));
    assert!(error.source().is_some());
    let error = ConfigError::Invalid("test reason");
    assert_eq!(error.to_string(), "invalid configuration: test reason");
    assert!(error.source().is_none());
    let error = ConfigError::NoConfigDirectory;
    assert!(error
        .to_string()
        .contains("per-user configuration directory"));
    assert!(error.source().is_none());
}
