use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

use crate::config::get_themes_dir;
use crate::errors::{AppError, AppResult};
use ratatui::style::{Color, Modifier, Style};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ThemeColor {
    pub color: String,
    pub modifiers: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Theme {
    pub theme: ThemeData,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ThemeData {
    pub default_fg: String,
    pub active_border: ThemeColor,
    pub inactive_border: String,
    pub options_text: String,
    pub searching_active_box: ThemeColor,
    pub selected_line_bg: ThemeColor,
    pub selected_range_bg: String,

    // Merge conflict states
    pub merging_conflict_bg: String,
    pub merging_conflict_fg: String,
    pub merging_unresolved_bg: String,
    pub merging_unresolved_fg: String,
    pub merging_resolved_bg: String,
    pub merging_resolved_fg: String,

    // Commit/status accents
    pub cherry_picked_commit_bg: String,
    pub cherry_picked_commit_fg: String,

    // Alert styles
    pub alert_header_bg: String,
    pub alert_header_text: String,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            theme: ThemeData {
                default_fg: "#ABB2BF".to_string(),
                active_border: ThemeColor {
                    color: "#61AFEF".to_string(),
                    modifiers: Some(vec!["bold".to_string()]),
                },
                inactive_border: "#3E4451".to_string(),
                options_text: "#56B6C2".to_string(),
                searching_active_box: ThemeColor {
                    color: "#E5C07B".to_string(),
                    modifiers: Some(vec!["bold".to_string()]),
                },
                selected_line_bg: ThemeColor {
                    color: "#39414f".to_string(),
                    modifiers: Some(vec!["bold".to_string()]),
                },
                selected_range_bg: "#39414f".to_string(),

                merging_conflict_bg: "#5c2a2a".to_string(),
                merging_conflict_fg: "#E06C75".to_string(),
                merging_unresolved_bg: "#4b2f2f".to_string(),
                merging_unresolved_fg: "#E06C75".to_string(),
                merging_resolved_bg: "#2a4b2a".to_string(),
                merging_resolved_fg: "#98C379".to_string(),

                cherry_picked_commit_bg: "#2b3a2b".to_string(),
                cherry_picked_commit_fg: "#98C379".to_string(),

                alert_header_bg: "#3E4451".to_string(),
                alert_header_text: "#E06C75".to_string(),
            },
        }
    }
}

/// Processed theme colors ready for UI consumption
#[derive(Debug, Clone)]
pub struct ThemeColors {
    pub default_fg: Style,
    pub active_border: Style,
    pub inactive_border: Style,
    pub options_text: Style,
    pub searching_active_box: Style,
    pub selected_line_bg: Style,
    pub selected_range_bg: Style,

    // Merge conflict states
    pub merging_conflict_bg: Style,
    pub merging_conflict_fg: Style,
    pub merging_unresolved_bg: Style,
    pub merging_unresolved_fg: Style,
    pub merging_resolved_bg: Style,
    pub merging_resolved_fg: Style,

    // Commit/status accents
    pub cherry_picked_commit_bg: Style,
    pub cherry_picked_commit_fg: Style,

    // Alert styles
    pub alert_header_bg: Style,
    pub alert_header_text: Style,

    // Additional commonly used styles derived from theme
    pub normal: Style,
    pub selected: Style,
    pub search_active: Style,
    pub search_inactive: Style,
    pub success: Style,
    pub error: Style,
    pub warning: Style,
    pub info: Style,
    pub directory: Style,
    pub file: Style,
    pub highlight: Style,
    pub disabled: Style,
    pub loading: Style,
    pub global_search: Style,
    pub local_search: Style,
    pub search_highlight: Style,

    // Modern UI muted colors
    pub muted_text: Style,     // For file sizes, dates, metadata
    pub separator: Style,      // For subtle dividers
    pub panel_bg: Style,       // Panel background for modern mode
}

impl Theme {
    /// Load theme from TOML file by name (e.g., "onedark")
    pub fn load(theme_name: &str) -> AppResult<Self> {
        Self::create_default_if_missing(theme_name)?;

        let themes_dir = get_themes_dir()?;
        let theme_path = themes_dir.join(format!("{}.toml", theme_name));

        Self::load_from_file(&theme_path)
    }

    /// Load theme from a specific TOML file path
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> AppResult<Self> {
        let content = fs::read_to_string(&path).map_err(|e| AppError::Configuration {
            message: format!(
                "Failed to read theme file '{}': {}",
                path.as_ref().display(),
                e
            ),
        })?;

        let theme: Self = toml::from_str(&content).map_err(|e| AppError::Configuration {
            message: format!(
                "Failed to parse TOML theme file '{}': {}",
                path.as_ref().display(),
                e
            ),
        })?;

        Ok(theme)
    }

    /// Save theme to TOML file by name
    pub fn save(&self, theme_name: &str) -> AppResult<()> {
        let themes_dir = get_themes_dir()?;
        let theme_path = themes_dir.join(format!("{}.toml", theme_name));

        self.save_to_file(&theme_path)
    }

    /// Save theme to a specific TOML file path
    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> AppResult<()> {
        let toml_content = toml::to_string_pretty(self).map_err(|e| AppError::Configuration {
            message: format!("Failed to serialize theme to TOML: {}", e),
        })?;

        fs::write(&path, toml_content).map_err(|e| AppError::Configuration {
            message: format!(
                "Failed to write theme to file '{}': {}",
                path.as_ref().display(),
                e
            ),
        })?;

        Ok(())
    }

    /// Create default theme file if it doesn't exist
    pub fn create_default_if_missing(theme_name: &str) -> AppResult<()> {
        crate::config::ensure_config_directories()?;

        let themes_dir = get_themes_dir()?;
        let theme_path = themes_dir.join(format!("{}.toml", theme_name));

        if !theme_path.exists() {
            let default_theme = Self::default();
            default_theme.save_to_file(&theme_path)?;

            println!("Created default theme file at: {}", theme_path.display());
        }

        Ok(())
    }

    /// Convert theme data to processed colors ready for UI
    pub fn to_colors(&self) -> AppResult<ThemeColors> {
        let theme = &self.theme;

        let default_fg = Self::parse_style(&theme.default_fg, None)?;
        let active_border = Self::parse_themed_color(&theme.active_border)?;
        let inactive_border = Self::parse_style(&theme.inactive_border, None)?;
        let options_text = Self::parse_style(&theme.options_text, None)?;
        let searching_active_box = Self::parse_themed_color(&theme.searching_active_box)?;
        let selected_line_bg = Self::parse_themed_color(&theme.selected_line_bg)?;
        let selected_range_bg = Self::parse_style(&theme.selected_range_bg, None)?;

        Ok(ThemeColors {
            default_fg: default_fg.clone(),
            active_border: active_border.clone(),
            inactive_border: inactive_border.clone(),
            options_text: options_text.clone(),
            searching_active_box: searching_active_box.clone(),
            selected_line_bg: selected_line_bg.clone(),
            selected_range_bg: selected_range_bg.clone(),

            merging_conflict_bg: Self::parse_style(&theme.merging_conflict_bg, None)?,
            merging_conflict_fg: Self::parse_style(&theme.merging_conflict_fg, None)?,
            merging_unresolved_bg: Self::parse_style(&theme.merging_unresolved_bg, None)?,
            merging_unresolved_fg: Self::parse_style(&theme.merging_unresolved_fg, None)?,
            merging_resolved_bg: Self::parse_style(&theme.merging_resolved_bg, None)?,
            merging_resolved_fg: Self::parse_style(&theme.merging_resolved_fg, None)?,

            cherry_picked_commit_bg: Self::parse_style(&theme.cherry_picked_commit_bg, None)?,
            cherry_picked_commit_fg: Self::parse_style(&theme.cherry_picked_commit_fg, None)?,

            alert_header_bg: Self::parse_style(&theme.alert_header_bg, None)?,
            alert_header_text: Self::parse_style(&theme.alert_header_text, None)?,

            // Derived styles
            normal: default_fg.clone(),
            selected: selected_line_bg.clone(),
            search_active: searching_active_box.clone(),
            search_inactive: inactive_border.clone(),
            success: Self::parse_style("#98C379", None)?, // Green
            error: Self::parse_style("#E06C75", None)?,   // Red
            warning: Self::parse_style("#E5C07B", None)?, // Yellow
            info: options_text.clone(),
            directory: Self::parse_style("#61AFEF", None)?, // Blue
            file: default_fg.clone(),
            highlight: Self::parse_style("#E5C07B", Some(&["bold"]))?, // Yellow + Bold
            disabled: inactive_border.clone(),
            loading: Self::parse_style("#E5C07B", None)?, // Yellow
            global_search: Self::parse_style("#C678DD", Some(&["bold"]))?, // Purple + Bold
            local_search: Self::parse_style("#56B6C2", Some(&["bold"]))?, // Cyan + Bold
            search_highlight: Style::default()
                .fg(Color::Black)
                .bg(Self::parse_color("#E5C07B")?) // Black text on yellow background
                .add_modifier(Modifier::BOLD),

            // Modern UI muted colors
            muted_text: Self::parse_style("#5C6370", None)?,    // Gray for metadata
            separator: Self::parse_style("#3E4451", None)?,     // Subtle dividers
            panel_bg: Style::default().bg(Self::parse_color("#21252B")?), // Dark panel bg
        })
    }

    /// Parse a ThemeColor (with potential modifiers) into a Style
    fn parse_themed_color(theme_color: &ThemeColor) -> AppResult<Style> {
        let modifiers = theme_color
            .modifiers
            .as_ref()
            .map(|m| m.iter().map(|s| s.as_str()).collect::<Vec<_>>())
            .unwrap_or_default();

        Self::parse_style(&theme_color.color, Some(&modifiers))
    }

    /// Parse a color string and optional modifiers into a Style
    fn parse_style(color_str: &str, modifiers: Option<&[&str]>) -> AppResult<Style> {
        let color = Self::parse_color(color_str)?;
        let mut style = Style::default().fg(color);

        if let Some(mods) = modifiers {
            for modifier in mods {
                match *modifier {
                    "bold" => style = style.add_modifier(Modifier::BOLD),
                    "italic" => style = style.add_modifier(Modifier::ITALIC),
                    "underlined" => style = style.add_modifier(Modifier::UNDERLINED),
                    "dim" => style = style.add_modifier(Modifier::DIM),
                    "crossed_out" => style = style.add_modifier(Modifier::CROSSED_OUT),
                    _ => {} // Ignore unknown modifiers
                }
            }
        }

        Ok(style)
    }

    /// Parse a hex color string into a Color
    fn parse_color(color_str: &str) -> AppResult<Color> {
        let color_str = color_str.trim_start_matches('#');

        if color_str.len() != 6 {
            return Err(AppError::Configuration {
                message: format!("Invalid color format '{}', expected #RRGGBB", color_str),
            });
        }

        let r = u8::from_str_radix(&color_str[0..2], 16).map_err(|e| AppError::Configuration {
            message: format!("Invalid red component in color '{}': {}", color_str, e),
        })?;

        let g = u8::from_str_radix(&color_str[2..4], 16).map_err(|e| AppError::Configuration {
            message: format!("Invalid green component in color '{}': {}", color_str, e),
        })?;

        let b = u8::from_str_radix(&color_str[4..6], 16).map_err(|e| AppError::Configuration {
            message: format!("Invalid blue component in color '{}': {}", color_str, e),
        })?;

        Ok(Color::Rgb(r, g, b))
    }
}
