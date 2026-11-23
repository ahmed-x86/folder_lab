//! Persistent user settings.
//!
//! Settings live in `~/.config/folder-lab/settings.conf` (or under
//! `$XDG_CONFIG_HOME` when it is set). The file is a tiny, human-editable
//! `key = value` list:
//!
//! ```text
//! # Folder Lab settings
//! theme = dark
//! ```
//!
//! Reading is forgiving on purpose: a missing, unreadable or half-edited file
//! never stops the app from starting, it just falls back to the defaults.
//! Writing is atomic (temp file, then rename), so a crash can't leave a
//! truncated settings file behind.

use std::env;
use std::fmt;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::str::FromStr;

/// Folder created inside the user's config directory.
const APP_DIR: &str = "folder-lab";
const FILE_NAME: &str = "settings.conf";

/// Which color theme the user asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemeChoice {
    /// Follow the desktop's light/dark setting.
    #[default]
    System,
    Light,
    Dark,
    Latte,
    Frappe,
    Macchiato,
    Mocha,
    WhiteDark,
    BlackLight,
    BlueAccent,
    ButterCream,
    MintCream,
    OliveGrove,
    VioletGrid,
    VioletNight,
    Android13,
    Nord,
    Dracula,
    Gruvbox,
    TokyoNight,
    Rose,
    AmoledBlack,
    OceanTeal,
    Sunset,
    Forest,
    Sakura,
    Custom,
}

impl ThemeChoice {
    /// The spelling used in the settings file.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Light => "light",
            Self::Dark => "dark",
            Self::Latte => "latte",
            Self::Frappe => "frappe",
            Self::Macchiato => "macchiato",
            Self::Mocha => "mocha",
            Self::WhiteDark => "white-dark",
            Self::BlackLight => "black-light",
            Self::BlueAccent => "blue-accent",
            Self::ButterCream => "butter-cream",
            Self::MintCream => "mint-cream",
            Self::OliveGrove => "olive-grove",
            Self::VioletGrid => "violet-grid",
            Self::VioletNight => "violet-night",
            Self::Android13 => "android13",
            Self::Nord => "nord",
            Self::Dracula => "dracula",
            Self::Gruvbox => "gruvbox",
            Self::TokyoNight => "tokyo-night",
            Self::Rose => "rose",
            Self::AmoledBlack => "amoled-black",
            Self::OceanTeal => "ocean-teal",
            Self::Sunset => "sunset",
            Self::Forest => "forest",
            Self::Sakura => "sakura",
            Self::Custom => "custom",
        }
    }
}

/// Returned when a settings value is not a known theme name.
#[derive(Debug, PartialEq, Eq)]
pub struct UnknownTheme;

impl fmt::Display for UnknownTheme {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("unknown theme name")
    }
}

impl std::error::Error for UnknownTheme {}

impl FromStr for ThemeChoice {
    type Err = UnknownTheme;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "system" => Ok(Self::System),
            "light" => Ok(Self::Light),
            "dark" => Ok(Self::Dark),
            "latte" => Ok(Self::Latte),
            "frappe" | "frappé" => Ok(Self::Frappe),
            "macchiato" => Ok(Self::Macchiato),
            "mocha" => Ok(Self::Mocha),
            "white-dark" => Ok(Self::WhiteDark),
            "black-light" => Ok(Self::BlackLight),
            "blue-accent" => Ok(Self::BlueAccent),
            "butter-cream" => Ok(Self::ButterCream),
            "mint-cream" => Ok(Self::MintCream),
            "olive-grove" => Ok(Self::OliveGrove),
            "violet-grid" => Ok(Self::VioletGrid),
            "violet-night" => Ok(Self::VioletNight),
            "android13" => Ok(Self::Android13),
            "nord" => Ok(Self::Nord),
            "dracula" => Ok(Self::Dracula),
            "gruvbox" => Ok(Self::Gruvbox),
            "tokyo-night" => Ok(Self::TokyoNight),
            "rose" => Ok(Self::Rose),
            "amoled-black" => Ok(Self::AmoledBlack),
            "ocean-teal" => Ok(Self::OceanTeal),
            "sunset" => Ok(Self::Sunset),
            "forest" => Ok(Self::Forest),
            "sakura" => Ok(Self::Sakura),
            "custom" => Ok(Self::Custom),
            _ => Err(UnknownTheme),
        }
    }
}

/// The colours of the "Custom colours" preset, stored as hex text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomColors {
    pub bg: String,
    pub sidebar: String,
    pub ribbon: String,
    pub field: String,
    pub accent: String,
    pub text: String,
    pub text_secondary: String,
    pub border: String,
    pub icon_tint: String,
}

impl Default for CustomColors {
    fn default() -> Self {
        Self {
            bg: "#1e1e2e".into(),
            sidebar: "#181825".into(),
            ribbon: "#11111b".into(),
            field: "#313244".into(),
            accent: "#cba6f7".into(),
            text: "#cdd6f4".into(),
            text_secondary: "#bac2de".into(),
            border: "#585b70".into(),
            icon_tint: "#cdd6f4".into(),
        }
    }
}

impl CustomColors {
    /// `settings.conf` key for each colour, in a stable order.
    pub fn pairs(&self) -> [(&'static str, &str); 9] {
        [
            ("custom-bg", &self.bg),
            ("custom-sidebar", &self.sidebar),
            ("custom-ribbon", &self.ribbon),
            ("custom-field", &self.field),
            ("custom-accent", &self.accent),
            ("custom-text", &self.text),
            ("custom-text-secondary", &self.text_secondary),
            ("custom-border", &self.border),
            ("custom-icon", &self.icon_tint),
        ]
    }

    fn set(&mut self, key: &str, value: &str) -> bool {
        let value = value.trim();
        let slot = match key {
            "custom-bg" => &mut self.bg,
            "custom-sidebar" => &mut self.sidebar,
            "custom-ribbon" => &mut self.ribbon,
            "custom-field" => &mut self.field,
            "custom-accent" => &mut self.accent,
            "custom-text" => &mut self.text,
            "custom-text-secondary" => &mut self.text_secondary,
            "custom-border" => &mut self.border,
            "custom-icon" => &mut self.icon_tint,
            _ => return false,
        };
        *slot = value.to_string();
        true
    }
}

/// Everything the app remembers between runs.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Config {
    pub theme: ThemeChoice,
    pub custom: CustomColors,
}

impl Config {
    /// Loads the settings, falling back to the defaults when the file is
    /// missing or unreadable.
    pub fn load() -> Self {
        let Some(path) = settings_path() else {
            return Self::default();
        };

        match fs::read_to_string(&path) {
            Ok(text) => Self::parse(&text),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Self::default(),
            Err(err) => {
                eprintln!("folder-lab: cannot read {}: {err}", path.display());
                Self::default()
            }
        }
    }

    /// Writes the settings, creating `~/.config/folder-lab/` if needed.
    pub fn save(&self) -> io::Result<()> {
        let path = settings_path().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "could not find a config directory (HOME is not set)",
            )
        })?;

        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }

        let tmp = path.with_extension("conf.tmp");
        fs::write(&tmp, self.render())?;
        fs::rename(&tmp, &path)
    }

    /// Reads `key = value` lines. Blank lines, `#` comments, unknown keys and
    /// invalid values are ignored.
    fn parse(text: &str) -> Self {
        let mut config = Self::default();

        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };

            let key = key.trim();
            if key == "theme" {
                if let Ok(theme) = value.parse() {
                    config.theme = theme;
                }
            } else {
                config.custom.set(key, value);
            }
        }

        config
    }

    fn render(&self) -> String {
        let mut text = format!("# Folder Lab settings\ntheme = {}\n", self.theme.as_str());
        for (key, value) in self.custom.pairs() {
            text.push_str(&format!("{key} = {value}\n"));
        }
        text
    }
}

/// `$XDG_CONFIG_HOME/folder-lab/` or `~/.config/folder-lab/`.
fn config_dir() -> Option<PathBuf> {
    let base = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;

    Some(base.join(APP_DIR))
}

fn settings_path() -> Option<PathBuf> {
    config_dir().map(|dir| dir.join(FILE_NAME))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_should_read_the_theme_value() {
        assert_eq!(Config::parse("theme = dark\n").theme, ThemeChoice::Dark);
    }

    #[test]
    fn parse_should_ignore_comments_blank_lines_and_spacing() {
        let text = "# comment\n\n   theme=light   \n";
        assert_eq!(Config::parse(text).theme, ThemeChoice::Light);
    }

    #[test]
    fn parse_should_default_to_system_when_the_value_is_invalid() {
        assert_eq!(Config::parse("theme = purple\n").theme, ThemeChoice::System);
    }

    #[test]
    fn parse_should_ignore_unknown_keys() {
        assert_eq!(Config::parse("font = big\n").theme, ThemeChoice::System);
    }

    #[test]
    fn render_should_round_trip_through_parse() {
        for theme in [
            ThemeChoice::System,
            ThemeChoice::Light,
            ThemeChoice::Dark,
            ThemeChoice::Latte,
            ThemeChoice::Frappe,
            ThemeChoice::Macchiato,
            ThemeChoice::Mocha,
            ThemeChoice::WhiteDark,
            ThemeChoice::BlackLight,
            ThemeChoice::BlueAccent,
            ThemeChoice::ButterCream,
            ThemeChoice::MintCream,
            ThemeChoice::OliveGrove,
            ThemeChoice::VioletGrid,
            ThemeChoice::VioletNight,
            ThemeChoice::Android13,
            ThemeChoice::Nord,
            ThemeChoice::Dracula,
            ThemeChoice::Gruvbox,
            ThemeChoice::TokyoNight,
            ThemeChoice::Rose,
            ThemeChoice::AmoledBlack,
            ThemeChoice::OceanTeal,
            ThemeChoice::Sunset,
            ThemeChoice::Forest,
            ThemeChoice::Sakura,
            ThemeChoice::Custom,
        ] {
            let config = Config {
                theme,
                ..Default::default()
            };
            assert_eq!(Config::parse(&config.render()), config);
        }
    }

    #[test]
    fn render_should_round_trip_custom_colours() {
        let config = Config {
            theme: ThemeChoice::Custom,
            custom: CustomColors {
                bg: "#101010".into(),
                accent: "#ff00aa".into(),
                ..Default::default()
            },
        };
        assert_eq!(Config::parse(&config.render()), config);
    }
}
