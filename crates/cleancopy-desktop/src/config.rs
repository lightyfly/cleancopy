//! TOML persistence and exact, platform-independent application matching.

use cleancopy_core::NumberingStyle;
use serde::{Deserialize, Serialize};
use std::{
    fmt, fs, io,
    io::Write,
    path::{Path, PathBuf},
};

const MAX_RULES: usize = 1024;
const MAX_TEXT_BYTES: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Format {
    ChineseNumbered,
    WordRichText,
    Markdown,
    PlainStripped,
    HtmlSource,
}

/// Display names are informational and are never used as matching identifiers.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppIdentity {
    pub name: String,
    pub bundle_id: Option<String>,
    pub exe: Option<String>,
    pub wm_class: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppRule {
    pub app: AppIdentity,
    pub format: Format,
}

impl AppRule {
    pub fn matches(&self, app: &AppIdentity) -> bool {
        self.match_rank(app).is_some()
    }

    fn match_rank(&self, app: &AppIdentity) -> Option<u8> {
        if same_id(self.app.bundle_id.as_deref(), app.bundle_id.as_deref()) {
            Some(3)
        } else if same_exe(self.app.exe.as_deref(), app.exe.as_deref()) {
            Some(2)
        } else if same_id(self.app.wm_class.as_deref(), app.wm_class.as_deref()) {
            Some(1)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Fallback {
    #[default]
    Popup,
    Fixed(Format),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecondaryItem {
    pub format: Format,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecentPaste {
    pub app: AppIdentity,
    /// `None` means the original clipboard was pasted without conversion.
    pub format: Option<Format>,
    /// Unix timestamp in milliseconds.
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub silent_mode: bool,
    pub numbering_style: NumberingStyle,
    pub app_rules: Vec<AppRule>,
    pub fallback: Fallback,
    /// Order is significant; disabled entries retain their positions.
    pub secondary_items: Vec<SecondaryItem>,
    pub hotkey: String,
    /// Unix timestamp in milliseconds, or no timed pause.
    pub paused_until: Option<u64>,
    /// Newest first, capped at two entries when loaded, saved, or recorded.
    pub recent_pastes: Vec<RecentPaste>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            silent_mode: true,
            numbering_style: NumberingStyle::Chinese,
            app_rules: default_app_rules(),
            fallback: Fallback::Popup,
            secondary_items: default_secondary_items(),
            hotkey: if cfg!(target_os = "macos") {
                "Super+Shift+V"
            } else {
                "Ctrl+Shift+V"
            }
            .into(),
            paused_until: None,
            recent_pastes: Vec::new(),
        }
    }
}

impl Config {
    /// Prefer a matching bundle ID, then executable basename, then WM_CLASS.
    /// Equal-specificity conflicts use the first rule in the configuration.
    pub fn format_for(&self, app: &AppIdentity) -> Option<Format> {
        let mut best = None;
        for rule in &self.app_rules {
            if let Some(rank) = rule.match_rank(app) {
                if best.is_none_or(|(old_rank, _)| rank > old_rank) {
                    best = Some((rank, rule.format));
                }
            }
        }
        best.map(|(_, format)| format).or(match self.fallback {
            Fallback::Popup => None,
            Fallback::Fixed(format) => Some(format),
        })
    }

    /// Remember only the strongest available identifier, never a display name.
    /// A bundle-specific choice must not rewrite a broader executable rule.
    /// An identity without a nonempty identifier is deliberately ignored.
    pub fn remember(&mut self, app: &AppIdentity, format: Format) {
        let mut identity = AppIdentity {
            name: app.name.clone(),
            ..AppIdentity::default()
        };
        let rank = if let Some(id) = nonempty(app.bundle_id.as_deref()) {
            identity.bundle_id = Some(id.to_owned());
            3
        } else if let Some(exe) = nonempty(app.exe.as_deref()).and_then(exe_basename) {
            identity.exe = Some(exe.to_owned());
            2
        } else if let Some(id) = nonempty(app.wm_class.as_deref()) {
            identity.wm_class = Some(id.to_owned());
            1
        } else {
            return;
        };
        if let Some(index) = self.app_rules.iter().position(|rule| {
            let rule_rank = if nonempty(rule.app.bundle_id.as_deref()).is_some() {
                3
            } else if nonempty(rule.app.exe.as_deref())
                .and_then(exe_basename)
                .is_some()
            {
                2
            } else {
                1
            };
            let identifier_count = [
                rule.app.bundle_id.as_deref(),
                rule.app.exe.as_deref(),
                rule.app.wm_class.as_deref(),
            ]
            .into_iter()
            .filter(|id| nonempty(*id).is_some())
            .count();
            identifier_count == 1 && rule_rank == rank && rule.match_rank(&identity) == Some(rank)
        }) {
            // A previously remembered rule may sit behind a multi-identifier
            // alias with the same rank. The explicit choice must win next time.
            let mut rule = self.app_rules.remove(index);
            rule.format = format;
            self.app_rules.insert(0, rule);
        } else {
            // Insert before broader multi-identifier rules at equal specificity.
            self.app_rules.insert(
                0,
                AppRule {
                    app: identity,
                    format,
                },
            );
        }
    }

    pub fn record_paste(&mut self, app: &AppIdentity, format: Option<Format>, timestamp: u64) {
        self.recent_pastes.insert(
            0,
            RecentPaste {
                app: app.clone(),
                format,
                timestamp,
            },
        );
        self.normalize_history();
    }

    /// A missing file is a first launch; unreadable or malformed files are errors.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let contents = match fs::read_to_string(path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(error) => return Err(ConfigError::Io(error)),
        };
        let mut config: Self = toml::from_str(&contents).map_err(ConfigError::Decode)?;
        config.normalize();
        config.validate()?;
        Ok(config)
    }

    /// Write in the destination directory, sync, then atomically replace the file.
    /// `NamedTempFile::persist` supports replacement on Windows as well as Unix.
    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), ConfigError> {
        let path = path.as_ref();
        let mut config = self.clone();
        config.normalize();
        config.validate()?;
        let contents = toml::to_string_pretty(&config).map_err(ConfigError::Encode)?;
        let parent = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent).map_err(ConfigError::Io)?;
        let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(ConfigError::Io)?;
        temporary
            .write_all(contents.as_bytes())
            .map_err(ConfigError::Io)?;
        temporary.as_file().sync_all().map_err(ConfigError::Io)?;
        temporary
            .persist(path)
            .map_err(|error| ConfigError::Io(error.error))?;
        Ok(())
    }

    /// Merge only fields edited since `previous`, preserving changes made by the
    /// daemon or another settings window. All desktop writers share this lock.
    pub fn save_changes(&self, path: &Path, previous: &Self) -> Result<Self, ConfigError> {
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent).map_err(ConfigError::Io)?;
        let lock = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(path.with_extension("toml.lock"))
            .map_err(ConfigError::Io)?;
        lock.lock().map_err(ConfigError::Io)?;
        let mut merged = Self::load(path)?;
        let original = toml::to_string(&merged).map_err(ConfigError::Encode)?;
        macro_rules! merge {
            ($($field:ident),+ $(,)?) => { $(
                if self.$field != previous.$field {
                    merged.$field = self.$field.clone();
                }
            )+ };
        }
        merge!(
            silent_mode,
            numbering_style,
            app_rules,
            fallback,
            secondary_items,
            hotkey,
            paused_until,
            recent_pastes
        );
        if toml::to_string(&merged).map_err(ConfigError::Encode)? != original {
            merged.save(path)?;
        }
        Ok(merged)
    }

    fn normalize_history(&mut self) {
        self.recent_pastes
            .sort_by_key(|paste| std::cmp::Reverse(paste.timestamp));
        self.recent_pastes.truncate(2);
    }

    fn normalize(&mut self) {
        let mut formats = Vec::new();
        self.secondary_items.retain(|item| {
            if formats.contains(&item.format) {
                false
            } else {
                formats.push(item.format);
                true
            }
        });
        for item in default_secondary_items() {
            if !formats.contains(&item.format) {
                self.secondary_items.push(item);
            }
        }
        self.normalize_history();
    }

    fn validate(&self) -> Result<(), ConfigError> {
        if self.app_rules.len() > MAX_RULES {
            return Err(ConfigError::Invalid(
                "at most 1024 application rules are allowed",
            ));
        }
        if self.hotkey.trim().is_empty() || self.hotkey.len() > 256 {
            return Err(ConfigError::Invalid(
                "hotkey must contain 1–256 bytes of nonblank text",
            ));
        }
        for rule in &self.app_rules {
            validate_identity(&rule.app)?;
            if nonempty(rule.app.bundle_id.as_deref()).is_none()
                && nonempty(rule.app.exe.as_deref())
                    .and_then(exe_basename)
                    .is_none()
                && nonempty(rule.app.wm_class.as_deref()).is_none()
            {
                return Err(ConfigError::Invalid(
                    "application rules require an identifier, not only a name",
                ));
            }
        }
        for paste in &self.recent_pastes {
            validate_identity(&paste.app)?;
        }
        Ok(())
    }
}

/// Resolve the per-user configuration path without creating directories.
pub fn config_path() -> Result<PathBuf, ConfigError> {
    dirs::config_dir()
        .map(|dir| dir.join("cleancopy").join("config.toml"))
        .ok_or(ConfigError::NoConfigDirectory)
}

#[derive(Debug)]
pub enum ConfigError {
    Io(io::Error),
    Decode(toml::de::Error),
    Encode(toml::ser::Error),
    Invalid(&'static str),
    NoConfigDirectory,
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "configuration I/O error: {error}"),
            Self::Decode(error) => write!(f, "invalid configuration TOML: {error}"),
            Self::Encode(error) => write!(f, "cannot serialize configuration: {error}"),
            Self::Invalid(message) => write!(f, "invalid configuration: {message}"),
            Self::NoConfigDirectory => {
                f.write_str("no per-user configuration directory is available")
            }
        }
    }
}

impl std::error::Error for ConfigError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Decode(error) => Some(error),
            Self::Encode(error) => Some(error),
            Self::Invalid(_) | Self::NoConfigDirectory => None,
        }
    }
}

fn nonempty(value: Option<&str>) -> Option<&str> {
    value.filter(|value| !value.trim().is_empty())
}

fn same_id(left: Option<&str>, right: Option<&str>) -> bool {
    match (nonempty(left), nonempty(right)) {
        (Some(left), Some(right)) => left.to_lowercase() == right.to_lowercase(),
        _ => false,
    }
}

fn exe_basename(exe: &str) -> Option<&str> {
    // Path::file_name alone cannot interpret Windows paths on Unix.
    exe.rsplit(['/', '\\'])
        .next()
        .filter(|name| !name.trim().is_empty())
}

fn same_exe(left: Option<&str>, right: Option<&str>) -> bool {
    same_id(left.and_then(exe_basename), right.and_then(exe_basename))
}

fn validate_identity(app: &AppIdentity) -> Result<(), ConfigError> {
    for value in std::iter::once(app.name.as_str()).chain(
        [
            app.bundle_id.as_deref(),
            app.exe.as_deref(),
            app.wm_class.as_deref(),
        ]
        .into_iter()
        .flatten(),
    ) {
        if value.len() > MAX_TEXT_BYTES || value.contains('\0') {
            return Err(ConfigError::Invalid(
                "application identity fields must be at most 4096 bytes and contain no NUL",
            ));
        }
    }
    Ok(())
}

fn default_secondary_items() -> Vec<SecondaryItem> {
    [
        Format::ChineseNumbered,
        Format::WordRichText,
        Format::Markdown,
        Format::PlainStripped,
        Format::HtmlSource,
    ]
    .into_iter()
    .map(|format| SecondaryItem {
        format,
        enabled: format != Format::HtmlSource,
    })
    .collect()
}

fn default_app_rules() -> Vec<AppRule> {
    // Separate exact aliases avoid broad matches such as "word" in a window title.
    type AppAliases<'a> = (Format, &'a str, &'a [&'a str], &'a [&'a str], &'a [&'a str]);
    let groups: &[AppAliases<'_>] = &[
        (
            Format::ChineseNumbered,
            "TextEdit / Notepad / gedit",
            &["com.apple.TextEdit"],
            &["TextEdit", "notepad.exe", "gedit", "gnome-text-editor"],
            &[
                "gedit",
                "org.gnome.gedit",
                "org.gnome.TextEditor",
                "notepad",
            ],
        ),
        (
            Format::WordRichText,
            "Microsoft Word",
            &["com.microsoft.Word"],
            &["WINWORD.EXE"],
            &["winword.exe", "Microsoft Word"],
        ),
        (
            Format::WordRichText,
            "WPS Writer",
            &[
                "com.kingsoft.wpsoffice.mac",
                "com.kingsoft.wpsoffice.mac.writer",
            ],
            &["wps.exe", "wps", "wpsoffice"],
            &["wps", "Wps", "kingsoft-wps"],
        ),
        (
            Format::Markdown,
            "Notion",
            &["notion.id"],
            &["Notion.exe", "notion"],
            &["notion"],
        ),
        (
            Format::Markdown,
            "Obsidian",
            &["md.obsidian"],
            &["Obsidian.exe", "obsidian"],
            &["obsidian"],
        ),
    ];
    let mut rules = Vec::new();
    for (format, name, bundles, executables, classes) in groups {
        for (kind, identifiers) in [(3, *bundles), (2, *executables), (1, *classes)] {
            for identifier in identifiers {
                let mut app = AppIdentity {
                    name: (*name).into(),
                    ..AppIdentity::default()
                };
                match kind {
                    3 => app.bundle_id = Some((*identifier).into()),
                    2 => app.exe = Some((*identifier).into()),
                    _ => app.wm_class = Some((*identifier).into()),
                }
                rules.push(AppRule {
                    app,
                    format: *format,
                });
            }
        }
    }
    rules
}
