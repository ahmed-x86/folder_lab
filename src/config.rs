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
}

impl ThemeChoice {
    /// The spelling used in the settings file.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }
}

/// Returned when a settings value is not a known theme name.
#[derive(Debug, PartialEq, Eq)]
pub struct UnknownTheme;

impl fmt::Display for UnknownTheme {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("unknown theme (expected system, light or dark)")
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
            _ => Err(UnknownTheme),
        }
    }
}

/// Everything the app remembers between runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Config {
    pub theme: ThemeChoice,
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

            if key.trim() == "theme" {
                if let Ok(theme) = value.parse() {
                    config.theme = theme;
                }
            }
        }

        config
    }

    fn render(&self) -> String {
        format!("# Folder Lab settings\ntheme = {}\n", self.theme.as_str())
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
        for theme in [ThemeChoice::System, ThemeChoice::Light, ThemeChoice::Dark] {
            let config = Config { theme };
            assert_eq!(Config::parse(&config.render()), config);
        }
    }
}
