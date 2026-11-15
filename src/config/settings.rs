//! Active TOML-backed user settings for ff.
//!
//! New runtime configuration should be added here.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

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
    /// Folders left out of the folder index with everything below them, e.g.
    /// "~/Movies". Unlike `ignore_directories`, only these exact folders.
    pub ignore_paths: Vec<String>,
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
                "cdn".to_string(),
                ".git".to_string(),
                ".sauce".to_string(),
                ".husky".to_string(),
                ".vscode".to_string(),
                ".zed".to_string(),
                "cypress".to_string(),
                "dist-prod".to_string(),
            ],
            ignore_paths: Vec::new(),
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

/// Add `add` to and remove `remove` from `ignore_paths` in ff's settings.toml.
pub fn update_ignore_paths(add: &[PathBuf], remove: &[String]) -> AppResult<()> {
    update_ignore_paths_in(&get_config_dir()?.join("settings.toml"), add, remove)
}

/// Add the folders `add` to, and remove the entries `remove` from, `ignore_paths` in the
/// settings file at `path`. An entry is removed when it's written exactly as given or
/// names the same folder. Only that list in the file as it is now changes, so other
/// settings, comments, and entries written by hand or by another ff stay. New entries
/// under home are written "~/…". A symlinked file is written through to its target; a
/// read-only one is refused.
pub fn update_ignore_paths_in(path: &Path, add: &[PathBuf], remove: &[String]) -> AppResult<()> {
    let error = |message: String| AppError::Configuration { message };
    let target = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let metadata = fs::metadata(&target)
        .map_err(|e| error(format!("Can't read {}: {e}", target.display())))?;
    if metadata.permissions().readonly() {
        return Err(error(format!("{} is read-only", target.display())));
    }
    let text = fs::read_to_string(&target)
        .map_err(|e| error(format!("Can't read {}: {e}", target.display())))?;
    let crlf = text.contains("\r\n");
    let mut doc: toml_edit::DocumentMut = text
        .parse()
        .map_err(|e| error(format!("Can't parse {}: {e}", target.display())))?;

    if doc.get("ignore_paths").is_none() {
        doc["ignore_paths"] = toml_edit::value(toml_edit::Array::new());
    }
    // Edited in place, so comments inside the list stay.
    let array = doc["ignore_paths"]
        .as_array_mut()
        .ok_or_else(|| error("ignore_paths in settings.toml isn't a list".to_string()))?;
    let removed = |entry: &str| {
        remove
            .iter()
            .any(|gone| entry == gone || resolved(Path::new(entry)) == resolved(Path::new(gone)))
    };
    // A comment after an entry belongs to the next one's text, so a removed entry's
    // comments move on to the entry after it.
    let decor_text = |value: &toml_edit::Value| {
        value
            .decor()
            .prefix()
            .and_then(|prefix| prefix.as_str())
            .unwrap_or("")
            .to_string()
    };
    let mut carried = String::new();
    let mut i = 0;
    while i < array.len() {
        if array
            .get(i)
            .and_then(|value| value.as_str())
            .is_some_and(removed)
        {
            carried.push_str(decor_text(&array.remove(i)).trim_end());
            continue;
        }
        if !carried.is_empty() {
            let value = array.get_mut(i).expect("index below len");
            let prefix = std::mem::take(&mut carried) + &decor_text(value);
            value.decor_mut().set_prefix(prefix);
        }
        i += 1;
    }
    for path in add {
        let present = array.iter().any(|value| {
            value
                .as_str()
                .is_some_and(|entry| resolved(Path::new(entry)) == resolved(path))
        });
        if !present {
            // One entry a line, like the default ignore_directories.
            let mut value = toml_edit::Value::from(home_relative(path));
            let prefix = std::mem::take(&mut carried) + "\n    ";
            value.decor_mut().set_prefix(prefix);
            array.push_formatted(value);
        }
    }
    if !carried.is_empty() {
        let trailing = carried + array.trailing().as_str().unwrap_or("");
        array.set_trailing(trailing);
    }
    if array.is_empty() {
        array.set_trailing("");
        array.set_trailing_comma(false);
    } else {
        array.set_trailing_comma(true);
        if !array.trailing().as_str().unwrap_or("").contains('\n') {
            array.set_trailing("\n");
        }
    }

    // Replace the file in one step, keeping its permissions.
    let temp = target.with_file_name(format!(".settings.toml.{}.tmp", std::process::id()));
    // toml_edit writes "\n"; keep a file that used "\r\n" that way.
    let mut out = doc.to_string();
    if crlf {
        out = out.replace("\r\n", "\n").replace('\n', "\r\n");
    }
    let result = fs::write(&temp, out)
        .and_then(|()| fs::set_permissions(&temp, metadata.permissions()))
        .and_then(|()| fs::rename(&temp, &target));
    if let Err(e) = result {
        let _ = fs::remove_file(&temp);
        return Err(error(format!("Can't write {}: {e}", target.display())));
    }
    Ok(())
}

/// `path` with `~` expanded and links resolved when it exists, for comparing entries.
fn resolved(path: &Path) -> PathBuf {
    let expanded = crate::cli::path_utils::expand_home(path).unwrap_or_else(|_| path.to_path_buf());
    fs::canonicalize(&expanded).unwrap_or(expanded)
}

/// "~/Movies" for a folder under home, otherwise the path as given.
fn home_relative(path: &Path) -> String {
    dirs::home_dir()
        .and_then(|home| {
            let home = fs::canonicalize(&home).unwrap_or(home);
            let path = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
            path.strip_prefix(&home)
                .ok()
                .map(|rest| format!("~/{}", rest.display()))
        })
        .unwrap_or_else(|| path.display().to_string())
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

        assert_eq!(keys.len(), 35);
        assert!(keys.windows(2).all(|pair| pair[0] < pair[1]), "{keys:?}");
        assert!(
            keymap.contains(r#""<leader>/" = "search.root""#),
            "{keymap}"
        );
        assert!(keymap.contains(r#"gg = "nav.top""#), "{keymap}");
        assert!(keymap.contains(r#"":" = "palette""#), "{keymap}");
    }

    #[test]
    fn update_ignore_paths_edits_only_that_key() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("settings.toml");
        let original = "# my settings\nstart_path = \"/w\"  # where ff opens\n\n[keymap.normal]\nx = \"quit\"\n";
        fs::write(&file, original).unwrap();
        let home = dirs::home_dir().unwrap();
        let movies = home.join("ff-no-such-folder/Movies");

        update_ignore_paths_in(&file, &[movies.clone(), movies.clone()], &[]).unwrap();
        let text = fs::read_to_string(&file).unwrap();
        assert!(
            text.starts_with("# my settings\nstart_path = \"/w\"  # where ff opens\n"),
            "{text}"
        );
        assert!(text.contains("x = \"quit\""), "{text}");
        let settings: Settings = toml::from_str(&text).unwrap();
        // Top-level, not inside [keymap.normal]; written "~/…"; added once.
        assert_eq!(settings.ignore_paths, ["~/ff-no-such-folder/Movies"]);
        assert_eq!(settings.start_path, "/w");
        assert_eq!(
            settings.keymap.normal.get("x").map(String::as_str),
            Some("quit")
        );

        // Entries written by hand stay, and a full path removes its "~/…" spelling.
        let text = text.replace("ignore_paths = [", "ignore_paths = [\n    \"/mine\",");
        fs::write(&file, text).unwrap();
        update_ignore_paths_in(&file, &[], &[movies.to_string_lossy().into_owned()]).unwrap();
        let settings: Settings = toml::from_str(&fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(settings.ignore_paths, ["/mine"]);
    }

    #[cfg(unix)]
    #[test]
    fn update_ignore_paths_writes_through_a_link_and_refuses_a_read_only_file() {
        use std::os::unix::fs::{symlink, PermissionsExt};
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("dotfiles-settings.toml");
        let link = dir.path().join("settings.toml");
        fs::write(&target, "theme = \"onedark\"\n").unwrap();
        symlink(&target, &link).unwrap();

        update_ignore_paths_in(&link, &[PathBuf::from("/data/raw")], &[]).unwrap();
        assert!(fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink());
        assert!(fs::read_to_string(&target)
            .unwrap()
            .contains("\"/data/raw\""));

        fs::set_permissions(&target, fs::Permissions::from_mode(0o444)).unwrap();
        let before = fs::read_to_string(&target).unwrap();
        assert!(update_ignore_paths_in(&link, &[PathBuf::from("/data/more")], &[]).is_err());
        assert_eq!(fs::read_to_string(&target).unwrap(), before);
    }

    #[test]
    fn update_ignore_paths_keeps_comments_in_the_list_and_removes_entries_as_written() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("settings.toml");
        let original = "ignore_paths = [\r\n    \"/keep\", # my big drive\r\n    # \"/old\",\r\n    \"Movies\",\r\n    \"/w/Case\",\r\n] # trailing note\r\ntheme = \"onedark\"\r\n";
        fs::write(&file, original).unwrap();

        // Entries spelled relative or in another case go when named exactly as written.
        update_ignore_paths_in(
            &file,
            &[PathBuf::from("/data/new")],
            &["Movies".to_string(), "/w/Case".to_string()],
        )
        .unwrap();
        let text = fs::read_to_string(&file).unwrap();
        for kept in [
            "# my big drive",
            "# \"/old\",",
            "# trailing note",
            "\"/keep\"",
        ] {
            assert!(text.contains(kept), "{kept} in {text}");
        }
        assert!(!text.contains("Movies") && !text.contains("Case"), "{text}");
        assert!(
            !text.replace("\r\n", "").contains('\n'),
            "line endings: {text:?}"
        );
        let settings: Settings = toml::from_str(&text).unwrap();
        assert_eq!(settings.ignore_paths, ["/keep", "/data/new"]);

        // Emptying the list leaves `[]`.
        update_ignore_paths_in(&file, &[], &["/keep".to_string(), "/data/new".to_string()])
            .unwrap();
        let settings: Settings = toml::from_str(&fs::read_to_string(&file).unwrap()).unwrap();
        assert!(settings.ignore_paths.is_empty());
    }
}
