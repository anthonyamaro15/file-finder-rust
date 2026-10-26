//! Active TOML-backed theme configuration for ff.
//!
//! Every color on screen is loaded here and converted into `ThemeColors`.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

use crate::config::get_themes_dir;
use crate::errors::{AppError, AppResult};
use ratatui::style::{Color, Modifier, Style};

/// Theme configuration loaded from TOML. Every table and color is optional: a
/// missing one takes the OneDark default, so new colors don't break older files.
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct Theme {
    pub palette: Palette,
    pub ui: UiTheme,
    pub syntax: SyntaxTheme,
    pub icons: IconTheme,
    pub markdown: MarkdownTheme,
    pub statusbar: StatusBarTheme,
}

/// Core color palette
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Palette {
    pub red: String,
    pub green: String,
    pub yellow: String,
    pub blue: String,
    pub purple: String,
    pub cyan: String,
    pub orange: String,
    pub gray: String,
    pub light_gray: String,
    pub fg: String,
    pub bg: String,
    pub bg_dark: String,
    pub bg_lighter: String,
    pub bg_highlight: String,
    pub selection: String,
    pub black: String,
    /// The cursor's name.
    pub bright: String,
    /// Sizes, dates, locations and quiet titles.
    pub muted: String,
    /// Separators between panes.
    pub dim: String,
}

/// UI element colors. Older files' `selected_*` and `search_match_*` are ignored: the
/// cursor and search matches take the mode colors.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct UiTheme {
    pub active_border: String,
    pub inactive_border: String,
}

/// Syntax highlighting colors (for code preview)
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct SyntaxTheme {
    pub keyword: String,
    pub function: String,
    pub r#type: String,
    pub string: String,
    pub number: String,
    pub comment: String,
    pub operator: String,
    pub variable: String,
}

/// Icon colors by file type
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct IconTheme {
    pub directory: String,
    pub rust: String,
    pub javascript: String,
    pub typescript: String,
    pub python: String,
    pub go: String,
    pub java: String,
    pub c: String,
    pub cpp: String,
    pub ruby: String,
    pub php: String,
    pub swift: String,
    pub kotlin: String,
    pub lua: String,
    pub shell: String,
    pub html: String,
    pub css: String,
    pub vue: String,
    pub react: String,
    pub svelte: String,
    pub json: String,
    pub yaml: String,
    pub toml: String,
    pub xml: String,
    pub markdown: String,
    pub config: String,
    pub image: String,
    pub video: String,
    pub audio: String,
    pub pdf: String,
    pub archive: String,
    pub git: String,
    pub key: String,
    pub lock: String,
    pub database: String,
    pub docker: String,
    pub license: String,
    pub readme: String,
    pub binary: String,
    pub font: String,
    pub default: String,
}

/// Markdown rendering colors
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct MarkdownTheme {
    pub header_1: String,
    pub header_2: String,
    pub header_3: String,
    pub header_4: String,
    pub bold: String,
    pub italic: String,
    pub code: String,
    pub code_bg: String,
    pub link: String,
    pub link_url: String,
    pub blockquote: String,
    pub list_marker: String,
    pub table_border: String,
    pub horizontal_rule: String,
}

/// Status bar colors, and one color per mode for its badge, cursor mark, prompt and
/// popup. Older files' `mode_insert` and `mode_command` are ignored.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct StatusBarTheme {
    pub background: String,
    pub foreground: String,
    pub mode_normal: String,
    pub mode_search: String,
    /// Searching from the root.
    pub mode_global: String,
    pub mode_visual: String,
    /// Trash and permanent delete.
    pub mode_delete: String,
    pub mode_create: String,
    pub mode_rename: String,
    pub mode_sort: String,
}

// Default implementations

impl Default for Palette {
    fn default() -> Self {
        Self {
            red: "#e55561".to_string(),
            green: "#8ebd6b".to_string(),
            yellow: "#e2b86b".to_string(),
            blue: "#4fa6ed".to_string(),
            purple: "#bf68d9".to_string(),
            cyan: "#48b0bd".to_string(),
            orange: "#cc9057".to_string(),
            gray: "#535965".to_string(),
            light_gray: "#7a818e".to_string(),
            fg: "#a0a8b7".to_string(),
            bg: "#1f2329".to_string(),
            bg_dark: "#181b20".to_string(),
            bg_lighter: "#282c34".to_string(),
            bg_highlight: "#30363f".to_string(),
            selection: "#323641".to_string(),
            black: "#0e1013".to_string(),
            bright: "#e3e7ee".to_string(),
            muted: "#5c6370".to_string(),
            dim: "#3e4451".to_string(),
        }
    }
}

impl Default for UiTheme {
    fn default() -> Self {
        Self {
            active_border: "#4fa6ed".to_string(),   // blue
            inactive_border: "#535965".to_string(), // gray
        }
    }
}

impl Default for SyntaxTheme {
    fn default() -> Self {
        Self {
            keyword: "#bf68d9".to_string(),  // purple
            function: "#4fa6ed".to_string(), // blue
            r#type: "#e2b86b".to_string(),   // yellow
            string: "#8ebd6b".to_string(),   // green
            number: "#cc9057".to_string(),   // orange
            comment: "#535965".to_string(),  // gray
            operator: "#48b0bd".to_string(), // cyan
            variable: "#e55561".to_string(), // red
        }
    }
}

impl Default for IconTheme {
    fn default() -> Self {
        Self {
            directory: "#4fa6ed".to_string(),  // blue
            rust: "#dea584".to_string(),       // rust orange
            javascript: "#e2b86b".to_string(), // yellow
            typescript: "#4fa6ed".to_string(), // blue
            python: "#4fa6ed".to_string(),     // blue
            go: "#48b0bd".to_string(),         // cyan
            java: "#cc9057".to_string(),       // orange
            c: "#7a818e".to_string(),          // light_gray
            cpp: "#bf68d9".to_string(),        // purple
            ruby: "#e55561".to_string(),       // red
            php: "#bf68d9".to_string(),        // purple
            swift: "#cc9057".to_string(),      // orange
            kotlin: "#bf68d9".to_string(),     // purple
            lua: "#4fa6ed".to_string(),        // blue
            shell: "#8ebd6b".to_string(),      // green
            html: "#cc9057".to_string(),       // orange
            css: "#bf68d9".to_string(),        // purple
            vue: "#8ebd6b".to_string(),        // green
            react: "#48b0bd".to_string(),      // cyan
            svelte: "#cc9057".to_string(),     // orange
            json: "#e2b86b".to_string(),       // yellow
            yaml: "#e55561".to_string(),       // red
            toml: "#cc9057".to_string(),       // orange
            xml: "#4fa6ed".to_string(),        // blue
            markdown: "#48b0bd".to_string(),   // cyan
            config: "#7a818e".to_string(),     // light_gray
            image: "#bf68d9".to_string(),      // purple
            video: "#e55561".to_string(),      // red
            audio: "#cc9057".to_string(),      // orange
            pdf: "#e55561".to_string(),        // red
            archive: "#e2b86b".to_string(),    // yellow
            git: "#cc9057".to_string(),        // orange
            key: "#e2b86b".to_string(),        // yellow
            lock: "#e2b86b".to_string(),       // yellow
            database: "#48b0bd".to_string(),   // cyan
            docker: "#48b0bd".to_string(),     // cyan
            license: "#e2b86b".to_string(),    // yellow
            readme: "#e2b86b".to_string(),     // yellow
            binary: "#7a818e".to_string(),     // light_gray
            font: "#e55561".to_string(),       // red
            default: "#a0a8b7".to_string(),    // fg
        }
    }
}

impl Default for MarkdownTheme {
    fn default() -> Self {
        Self {
            header_1: "#e55561".to_string(),        // red
            header_2: "#e2b86b".to_string(),        // yellow
            header_3: "#8ebd6b".to_string(),        // green
            header_4: "#4fa6ed".to_string(),        // blue
            bold: "#cc9057".to_string(),            // orange
            italic: "#bf68d9".to_string(),          // purple
            code: "#8ebd6b".to_string(),            // green
            code_bg: "#282c34".to_string(),         // bg_lighter
            link: "#4fa6ed".to_string(),            // blue
            link_url: "#535965".to_string(),        // gray
            blockquote: "#7a818e".to_string(),      // light_gray
            list_marker: "#bf68d9".to_string(),     // purple
            table_border: "#535965".to_string(),    // gray
            horizontal_rule: "#535965".to_string(), // gray
        }
    }
}

impl Default for StatusBarTheme {
    fn default() -> Self {
        Self {
            background: "#1e2128".to_string(),
            foreground: "#a0a8b7".to_string(),  // fg
            mode_normal: "#4fa6ed".to_string(), // blue
            mode_search: "#8ebd6b".to_string(), // green
            mode_global: "#e2b86b".to_string(), // yellow
            mode_visual: "#bf68d9".to_string(), // purple
            mode_delete: "#e55561".to_string(), // red
            mode_create: "#48b0bd".to_string(), // cyan
            mode_rename: "#cc9057".to_string(), // orange
            mode_sort: "#a0a8b7".to_string(),   // fg
        }
    }
}

/// Processed theme colors ready for UI consumption
#[derive(Debug, Clone)]
pub struct ThemeColors {
    // Palette colors
    pub red: Color,
    pub green: Color,
    pub yellow: Color,
    pub blue: Color,
    pub purple: Color,
    pub cyan: Color,
    pub orange: Color,
    pub gray: Color,
    pub light_gray: Color,
    pub fg: Color,
    pub bg: Color,
    pub bg_dark: Color,
    pub bg_lighter: Color,
    pub bg_highlight: Color,
    pub selection: Color,
    pub black: Color,
    pub bright: Color,
    pub muted: Color,
    pub dim: Color,

    // UI styles
    pub active_border: Style,
    pub inactive_border: Style,
    pub normal: Style,
    pub disabled: Style,
    pub info: Style,
    pub success: Style,
    pub warning: Style,
    pub error: Style,
    /// Names starting with '.'.
    pub dotfile: Style,

    // Row bands, tinted from the background
    /// The cursor's, in the pane keys go to.
    pub cursor_bg: Color,
    /// The cursor's in the other pane.
    pub inactive_cursor_bg: Color,
    /// The visual selection's.
    pub visual_bg: Color,
    /// The cursor's inside the visual selection.
    pub visual_cursor_bg: Color,
    /// The top bar's while a search of the folder is typed.
    pub search_bg: Color,
    /// The top bar's while a search from the root is typed.
    pub global_bg: Color,
    /// A message's row: an error's, a warning's and a notice's.
    pub error_bg: Color,
    pub warning_bg: Color,
    pub success_bg: Color,

    // Icon colors
    pub icon_directory: Color,
    pub icon_rust: Color,
    pub icon_javascript: Color,
    pub icon_typescript: Color,
    pub icon_python: Color,
    pub icon_go: Color,
    pub icon_java: Color,
    pub icon_c: Color,
    pub icon_cpp: Color,
    pub icon_ruby: Color,
    pub icon_php: Color,
    pub icon_swift: Color,
    pub icon_kotlin: Color,
    pub icon_lua: Color,
    pub icon_shell: Color,
    pub icon_html: Color,
    pub icon_css: Color,
    pub icon_vue: Color,
    pub icon_react: Color,
    pub icon_svelte: Color,
    pub icon_json: Color,
    pub icon_yaml: Color,
    pub icon_toml: Color,
    pub icon_xml: Color,
    pub icon_markdown: Color,
    pub icon_config: Color,
    pub icon_image: Color,
    pub icon_video: Color,
    pub icon_audio: Color,
    pub icon_pdf: Color,
    pub icon_archive: Color,
    pub icon_git: Color,
    pub icon_key: Color,
    pub icon_lock: Color,
    pub icon_database: Color,
    pub icon_docker: Color,
    pub icon_license: Color,
    pub icon_readme: Color,
    pub icon_binary: Color,
    pub icon_font: Color,
    pub icon_default: Color,

    // Markdown colors
    pub md_header_1: Color,
    pub md_header_2: Color,
    pub md_header_3: Color,
    pub md_header_4: Color,
    pub md_bold: Color,
    pub md_italic: Color,
    pub md_code: Color,
    pub md_code_bg: Color,
    pub md_link: Color,
    pub md_link_url: Color,
    pub md_blockquote: Color,
    pub md_list_marker: Color,
    pub md_table_border: Color,
    pub md_horizontal_rule: Color,

    // Status bar colors
    pub statusbar_bg: Color,
    pub statusbar_fg: Color,

    // Mode colors
    pub mode_normal: Color,
    pub mode_search: Color,
    pub mode_global: Color,
    pub mode_visual: Color,
    pub mode_delete: Color,
    pub mode_create: Color,
    pub mode_rename: Color,
    pub mode_sort: Color,
}

impl Theme {
    /// Load theme from TOML file by name (e.g., "onedark-darker")
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
        }

        Ok(())
    }

    /// Convert theme data to processed colors ready for UI
    pub fn to_colors(&self) -> AppResult<ThemeColors> {
        let p = &self.palette;
        let ui = &self.ui;
        let icons = &self.icons;
        let md = &self.markdown;
        let sb = &self.statusbar;

        // Parse palette colors
        let red = Self::parse_color(&p.red)?;
        let green = Self::parse_color(&p.green)?;
        let yellow = Self::parse_color(&p.yellow)?;
        let blue = Self::parse_color(&p.blue)?;
        let purple = Self::parse_color(&p.purple)?;
        let cyan = Self::parse_color(&p.cyan)?;
        let orange = Self::parse_color(&p.orange)?;
        let gray = Self::parse_color(&p.gray)?;
        let light_gray = Self::parse_color(&p.light_gray)?;
        let fg = Self::parse_color(&p.fg)?;
        let bg = Self::parse_color(&p.bg)?;
        let bg_dark = Self::parse_color(&p.bg_dark)?;
        let bg_lighter = Self::parse_color(&p.bg_lighter)?;
        let bg_highlight = Self::parse_color(&p.bg_highlight)?;
        let selection = Self::parse_color(&p.selection)?;
        let black = Self::parse_color(&p.black)?;
        let mode_visual = Self::parse_color(&sb.mode_visual)?;
        let mode_search = Self::parse_color(&sb.mode_search)?;
        let mode_global = Self::parse_color(&sb.mode_global)?;

        // Parse UI colors
        let active_border_color = Self::parse_color(&ui.active_border)?;
        let inactive_border_color = Self::parse_color(&ui.inactive_border)?;

        Ok(ThemeColors {
            // Palette
            red,
            green,
            yellow,
            blue,
            purple,
            cyan,
            orange,
            gray,
            light_gray,
            fg,
            bg,
            bg_dark,
            bg_lighter,
            bg_highlight,
            selection,
            black,
            bright: Self::parse_color(&p.bright)?,
            muted: Self::parse_color(&p.muted)?,
            dim: Self::parse_color(&p.dim)?,

            // UI styles
            active_border: Style::default()
                .fg(active_border_color)
                .add_modifier(Modifier::BOLD),
            inactive_border: Style::default().fg(inactive_border_color),
            normal: Style::default().fg(fg),
            disabled: Style::default().fg(gray),
            info: Style::default().fg(cyan),
            success: Style::default().fg(green),
            warning: Style::default().fg(yellow),
            error: Style::default().fg(red),
            dotfile: Style::default().fg(gray),

            // Row bands and the search and message tints: a little of the foreground or a
            // color over the background, so a theme changes them with those colors.
            cursor_bg: blend(bg, fg, 0.1),
            inactive_cursor_bg: blend(bg, fg, 0.04),
            visual_bg: blend(bg, mode_visual, 0.1),
            visual_cursor_bg: blend(bg, mode_visual, 0.22),
            search_bg: blend(bg, mode_search, 0.1),
            global_bg: blend(bg, mode_global, 0.1),
            error_bg: blend(bg, red, 0.15),
            warning_bg: blend(bg, yellow, 0.15),
            success_bg: blend(bg, green, 0.15),

            // Icon colors
            icon_directory: Self::parse_color(&icons.directory)?,
            icon_rust: Self::parse_color(&icons.rust)?,
            icon_javascript: Self::parse_color(&icons.javascript)?,
            icon_typescript: Self::parse_color(&icons.typescript)?,
            icon_python: Self::parse_color(&icons.python)?,
            icon_go: Self::parse_color(&icons.go)?,
            icon_java: Self::parse_color(&icons.java)?,
            icon_c: Self::parse_color(&icons.c)?,
            icon_cpp: Self::parse_color(&icons.cpp)?,
            icon_ruby: Self::parse_color(&icons.ruby)?,
            icon_php: Self::parse_color(&icons.php)?,
            icon_swift: Self::parse_color(&icons.swift)?,
            icon_kotlin: Self::parse_color(&icons.kotlin)?,
            icon_lua: Self::parse_color(&icons.lua)?,
            icon_shell: Self::parse_color(&icons.shell)?,
            icon_html: Self::parse_color(&icons.html)?,
            icon_css: Self::parse_color(&icons.css)?,
            icon_vue: Self::parse_color(&icons.vue)?,
            icon_react: Self::parse_color(&icons.react)?,
            icon_svelte: Self::parse_color(&icons.svelte)?,
            icon_json: Self::parse_color(&icons.json)?,
            icon_yaml: Self::parse_color(&icons.yaml)?,
            icon_toml: Self::parse_color(&icons.toml)?,
            icon_xml: Self::parse_color(&icons.xml)?,
            icon_markdown: Self::parse_color(&icons.markdown)?,
            icon_config: Self::parse_color(&icons.config)?,
            icon_image: Self::parse_color(&icons.image)?,
            icon_video: Self::parse_color(&icons.video)?,
            icon_audio: Self::parse_color(&icons.audio)?,
            icon_pdf: Self::parse_color(&icons.pdf)?,
            icon_archive: Self::parse_color(&icons.archive)?,
            icon_git: Self::parse_color(&icons.git)?,
            icon_key: Self::parse_color(&icons.key)?,
            icon_lock: Self::parse_color(&icons.lock)?,
            icon_database: Self::parse_color(&icons.database)?,
            icon_docker: Self::parse_color(&icons.docker)?,
            icon_license: Self::parse_color(&icons.license)?,
            icon_readme: Self::parse_color(&icons.readme)?,
            icon_binary: Self::parse_color(&icons.binary)?,
            icon_font: Self::parse_color(&icons.font)?,
            icon_default: Self::parse_color(&icons.default)?,

            // Markdown colors
            md_header_1: Self::parse_color(&md.header_1)?,
            md_header_2: Self::parse_color(&md.header_2)?,
            md_header_3: Self::parse_color(&md.header_3)?,
            md_header_4: Self::parse_color(&md.header_4)?,
            md_bold: Self::parse_color(&md.bold)?,
            md_italic: Self::parse_color(&md.italic)?,
            md_code: Self::parse_color(&md.code)?,
            md_code_bg: Self::parse_color(&md.code_bg)?,
            md_link: Self::parse_color(&md.link)?,
            md_link_url: Self::parse_color(&md.link_url)?,
            md_blockquote: Self::parse_color(&md.blockquote)?,
            md_list_marker: Self::parse_color(&md.list_marker)?,
            md_table_border: Self::parse_color(&md.table_border)?,
            md_horizontal_rule: Self::parse_color(&md.horizontal_rule)?,

            // Status bar colors
            statusbar_bg: Self::parse_color(&sb.background)?,
            statusbar_fg: Self::parse_color(&sb.foreground)?,

            // Mode colors
            mode_normal: Self::parse_color(&sb.mode_normal)?,
            mode_search,
            mode_global,
            mode_visual,
            mode_delete: Self::parse_color(&sb.mode_delete)?,
            mode_create: Self::parse_color(&sb.mode_create)?,
            mode_rename: Self::parse_color(&sb.mode_rename)?,
            mode_sort: Self::parse_color(&sb.mode_sort)?,
        })
    }

    /// Parse a hex color string into a Color
    pub fn parse_color(color_str: &str) -> AppResult<Color> {
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

/// `over` mixed into `base` by `amount`, from 0 (all `base`) to 1 (all `over`).
fn blend(base: Color, over: Color, amount: f32) -> Color {
    let (Color::Rgb(r, g, b), Color::Rgb(r2, g2, b2)) = (base, over) else {
        return base;
    };
    let mix = |from: u8, to: u8| {
        (f32::from(from) + (f32::from(to) - f32::from(from)) * amount).round() as u8
    };
    Color::Rgb(mix(r, r2), mix(g, g2), mix(b, b2))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_partial_theme_file_loads_with_the_default_colors() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mine.toml");
        fs::write(
            &path,
            "[palette]\nred = \"#ff0000\"\n\n[icons]\nrust = \"#00ff00\"\n",
        )
        .unwrap();

        let theme = Theme::load_from_file(&path).unwrap();
        let defaults = Theme::default();
        assert_eq!(theme.palette.red, "#ff0000");
        assert_eq!(theme.palette.blue, defaults.palette.blue);
        assert_eq!(theme.icons.rust, "#00ff00");
        assert_eq!(theme.icons.directory, defaults.icons.directory);
        assert_eq!(theme.ui.active_border, defaults.ui.active_border);
        assert!(theme.to_colors().is_ok());
    }

    #[test]
    fn each_mode_defaults_to_its_palette_color() {
        let colors = Theme::default().to_colors().unwrap();
        let modes = [
            (colors.mode_normal, colors.blue),
            (colors.mode_search, colors.green),
            (colors.mode_global, colors.yellow),
            (colors.mode_visual, colors.purple),
            (colors.mode_delete, colors.red),
            (colors.mode_create, colors.cyan),
            (colors.mode_rename, colors.orange),
            (colors.mode_sort, colors.fg),
        ];
        for (i, (mode, color)) in modes.into_iter().enumerate() {
            assert_eq!(mode, color, "mode {i}");
        }
    }

    #[test]
    fn a_theme_file_overriding_one_mode_changes_only_that_mode() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mine.toml");
        fs::write(&path, "[statusbar]\nmode_rename = \"#ff00ff\"\n").unwrap();

        let colors = Theme::load_from_file(&path).unwrap().to_colors().unwrap();
        let defaults = Theme::default().to_colors().unwrap();
        assert_eq!(colors.mode_rename, Color::Rgb(255, 0, 255));
        let others = |c: &ThemeColors| {
            [
                c.mode_normal,
                c.mode_search,
                c.mode_global,
                c.mode_visual,
                c.mode_delete,
                c.mode_create,
                c.mode_sort,
                c.visual_bg,
                c.cursor_bg,
            ]
        };
        assert_eq!(others(&colors), others(&defaults));
    }

    #[test]
    fn older_theme_files_with_retired_keys_still_load() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("old.toml");
        fs::write(
            &path,
            "[ui]\nselected_bg = \"#4fa6ed\"\nselected_fg = \"#0e1013\"\n\
             search_match_bg = \"#e2b86b\"\nsearch_match_fg = \"#0e1013\"\n\n\
             [statusbar]\nmode_normal = \"#4fa6ed\"\nmode_insert = \"#8ebd6b\"\n\
             mode_visual = \"#bf68d9\"\nmode_command = \"#e2b86b\"\n",
        )
        .unwrap();

        let colors = Theme::load_from_file(&path).unwrap().to_colors().unwrap();
        let defaults = Theme::default().to_colors().unwrap();
        assert_eq!(colors.mode_search, defaults.mode_search);
        assert_eq!(colors.mode_global, defaults.mode_global);
        assert_eq!(colors.cursor_bg, defaults.cursor_bg);
    }

    #[test]
    fn bands_are_tinted_from_the_background() {
        let colors = Theme::default().to_colors().unwrap();
        // Near the design's #2a2f39, #20242b, #2b2232, #3f2d4c, #1c2619 and #2b2516.
        assert_eq!(colors.cursor_bg, Color::Rgb(0x2c, 0x30, 0x37));
        assert_eq!(colors.inactive_cursor_bg, Color::Rgb(0x24, 0x28, 0x2f));
        assert_eq!(colors.visual_bg, Color::Rgb(0x2f, 0x2a, 0x3b));
        assert_eq!(colors.visual_cursor_bg, Color::Rgb(0x42, 0x32, 0x50));
        assert_eq!(colors.search_bg, Color::Rgb(0x2a, 0x32, 0x30));
        assert_eq!(colors.global_bg, Color::Rgb(0x33, 0x32, 0x30));
    }
}
