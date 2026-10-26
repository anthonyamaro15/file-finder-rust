//! Active TOML-backed user settings for ff.
//!
//! New runtime configuration should be added here.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

use crate::config::get_config_dir;
use crate::config::keymap::KeymapSettings;
use crate::errors::{AppError, AppResult};

/// Layout style for the main UI
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum LayoutStyle {
    /// Classic: Heavy borders, emoji icons, multi-section status bar
    Classic,
    /// Modern: Minimal borders, nerd font icons, clean status bar (default)
    #[default]
    Modern,
    /// Miller: Three-pane miller columns layout
    Miller,
}

/// Nerd font usage setting
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum NerdFontSetting {
    /// Always use nerd font icons
    Always,
    /// Never use nerd font icons (use emoji fallback)
    Never,
    /// Auto-detect based on terminal (default)
    #[default]
    Auto,
}

/// Status bar style
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum StatusBarStyle {
    /// Classic: Multiple bordered sections
    Classic,
    /// Minimal: Single clean line (default)
    #[default]
    Minimal,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct FinderSettings {
    #[serde(default = "default_finder_max_results")]
    pub max_results: usize,
    #[serde(default = "default_global_search_debounce_ms")]
    pub global_search_debounce_ms: u64,
}

fn default_finder_max_results() -> usize {
    500
}

fn default_global_search_debounce_ms() -> u64 {
    80
}

impl Default for FinderSettings {
    fn default() -> Self {
        Self {
            max_results: default_finder_max_results(),
            global_search_debounce_ms: default_global_search_debounce_ms(),
        }
    }
}

/// Any field left out of settings.toml takes its value from `Settings::default()`,
/// so a partial file loads and new settings don't break older files.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Settings {
    pub start_path: String,
    pub root_dir: String,
    pub cache_directory: String,
    pub settings_path: String,
    pub ignore_directories: Vec<String>,
    pub theme: String,

    /// Show file sizes in file list
    pub show_size_bars: bool,

    /// Syntax highlighting theme for file previews
    /// Available themes: "base16-ocean.dark", "base16-eighties.dark", "base16-mocha.dark", "base16-ocean.light", "InspiredGitHub", "Solarized (dark)", "Solarized (light)"
    pub syntax_theme: String,

    // === UI Modernization Settings ===
    /// Layout style: "classic", "modern", or "miller"
    pub layout_style: LayoutStyle,

    /// Nerd font icons: "always", "never", or "auto"
    pub use_nerd_fonts: NerdFontSetting,

    /// Show panel borders
    pub show_borders: bool,

    /// Status bar style: "classic" or "minimal"
    pub status_bar_style: StatusBarStyle,

    /// Show the bottom row of keys for the current mode
    pub show_hints: bool,

    /// Mode-aware keybindings.
    pub keymap: KeymapSettings,

    /// Finder/search behavior.
    pub finder: FinderSettings,
}

fn default_show_file_sizes() -> bool {
    true
}

fn default_syntax_theme() -> String {
    "base16-ocean.dark".to_string()
}

fn default_show_borders() -> bool {
    false // Modern default: minimal borders
}

impl Default for Settings {
    fn default() -> Self {
        let home_dir = dirs::home_dir().unwrap_or_else(|| {
            std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
        });

        let config_dir = home_dir.join(".config").join("ff");

        Self {
            start_path: format!("{}/Desktop", home_dir.display()),
            root_dir: home_dir.display().to_string(),
            cache_directory: format!("{}/cache_directory.json", config_dir.display()),
            settings_path: format!("{}/settings.toml", config_dir.display()),
            ignore_directories: vec![
                "node_modules".to_string(),
                "bower_components".to_string(),
                "obj".to_string(),
                "maui".to_string(),
                "target".to_string(),
                ".venv".to_string(),
                "src".to_string(),
                "cdn".to_string(),
                ".git".to_string(),
                ".sauce".to_string(),
                ".husky".to_string(),
                ".vscode".to_string(),
                ".zed".to_string(),
                "cypress".to_string(),
                "dist-prod".to_string(),
            ],
            theme: "onedark".to_string(),
            show_size_bars: default_show_file_sizes(),
            syntax_theme: default_syntax_theme(),
            // UI Modernization defaults (modern is the new default)
            layout_style: LayoutStyle::default(),
            use_nerd_fonts: NerdFontSetting::default(),
            show_borders: default_show_borders(),
            status_bar_style: StatusBarStyle::default(),
            show_hints: true,
            keymap: KeymapSettings::default(),
            finder: FinderSettings::default(),
        }
    }
}

impl Settings {
    /// Load settings from TOML file, creating default if it doesn't exist
    pub fn load() -> AppResult<Self> {
        Self::create_default_if_missing()?;

        let config_dir = get_config_dir()?;
        let settings_path = config_dir.join("settings.toml");

        Self::load_from_file(&settings_path)
    }

    /// Load settings from a specific TOML file path
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> AppResult<Self> {
        let content = fs::read_to_string(&path).map_err(|e| AppError::Configuration {
            message: format!(
                "Failed to read settings file '{}': {}",
                path.as_ref().display(),
                e
            ),
        })?;

        toml::from_str(&content).map_err(|e| AppError::Configuration {
            message: format!(
                "Failed to parse TOML settings file '{}': {}",
                path.as_ref().display(),
                e
            ),
        })
    }

    /// Save settings to a specific TOML file path
    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> AppResult<()> {
        let toml_content = toml::to_string_pretty(self).map_err(|e| AppError::Configuration {
            message: format!("Failed to serialize settings to TOML: {}", e),
        })?;

        fs::write(&path, toml_content).map_err(|e| AppError::Configuration {
            message: format!(
                "Failed to write settings to file '{}': {}",
                path.as_ref().display(),
                e
            ),
        })?;

        Ok(())
    }

    /// Create default settings file if it doesn't exist
    pub fn create_default_if_missing() -> AppResult<()> {
        crate::config::ensure_config_directories()?;

        let config_dir = get_config_dir()?;
        let settings_path = config_dir.join("settings.toml");

        if !settings_path.exists() {
            let default_settings = Self::default();
            default_settings.save_to_file(&settings_path)?;

            println!(
                "Created default settings file at: {}",
                settings_path.display()
            );
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use tempfile::NamedTempFile;

    use super::*;

    #[test]
    fn a_keymap_only_file_loads_and_merges_with_the_defaults() {
        use crate::config::{Action, KeySequence};

        let mut file = NamedTempFile::new().unwrap();
        write!(
            file,
            r#"
show_borders = true

[keymap.normal]
"/" = "search.root"
"x" = "file.trsh"
"<C-q" = "quit"
"#
        )
        .unwrap();

        // Every setting left out takes its default, and bad keymap entries never
        // stop ff from starting.
        let settings = Settings::load_from_file(file.path()).unwrap();
        let defaults = Settings::default();
        assert!(settings.show_borders);
        assert_eq!(settings.start_path, defaults.start_path);
        assert_eq!(settings.ignore_directories, defaults.ignore_directories);
        assert_eq!(settings.theme, defaults.theme);
        assert_eq!(settings.finder, defaults.finder);

        let bindings = settings.keymap.resolve();
        let bound = |key: &str| {
            let sequence = KeySequence::parse(key).unwrap();
            bindings
                .normal
                .iter()
                .find(|(bound, _)| *bound == sequence)
                .map(|(_, action)| *action)
        };

        assert_eq!(bound("/"), Some(Action::SearchRoot));
        assert_eq!(bound("<leader>/"), Some(Action::SearchRoot));
        assert_eq!(bound("i"), Some(Action::SearchCurrent));
        assert_eq!(bound("q"), Some(Action::Quit));
        assert_eq!(bindings.ignored.len(), 2);
    }

    #[test]
    fn first_run_settings_list_the_default_keymap_sorted() {
        let toml = toml::to_string_pretty(&Settings::default()).unwrap();
        let keymap = toml.split("[keymap.normal]\n").nth(1).unwrap();
        let keys: Vec<&str> = keymap
            .lines()
            .take_while(|line| !line.is_empty() && !line.starts_with('['))
            .map(|line| line.split(" = ").next().unwrap().trim_matches('"'))
            .collect();

        assert_eq!(keys.len(), 34);
        assert!(keys.windows(2).all(|pair| pair[0] < pair[1]), "{keys:?}");
        assert!(
            keymap.contains(r#""<leader>/" = "search.root""#),
            "{keymap}"
        );
        assert!(keymap.contains(r#"gg = "nav.top""#), "{keymap}");
        assert!(keymap.contains(r#"":" = "palette""#), "{keymap}");
    }
}
