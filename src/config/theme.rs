use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

use crate::config::get_themes_dir;
use crate::errors::{AppError, AppResult};
use ratatui::style::{Color, Modifier, Style};

/// Theme configuration loaded from TOML
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Theme {
    pub palette: Palette,
    #[serde(default)]
    pub ui: UiTheme,
    #[serde(default)]
    pub syntax: SyntaxTheme,
    #[serde(default)]
    pub icons: IconTheme,
    #[serde(default)]
    pub markdown: MarkdownTheme,
    #[serde(default)]
    pub statusbar: StatusBarTheme,
}

/// Core color palette
#[derive(Debug, Serialize, Deserialize, Clone)]
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
}

/// UI element colors
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UiTheme {
    pub active_border: String,
    pub inactive_border: String,
    pub selected_bg: String,
    pub selected_fg: String,
    pub search_match_bg: String,
    pub search_match_fg: String,
}

/// Syntax highlighting colors (for code preview)
#[derive(Debug, Serialize, Deserialize, Clone)]
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

/// Status bar colors
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct StatusBarTheme {
    pub background: String,
    pub foreground: String,
    pub mode_normal: String,
    pub mode_insert: String,
    pub mode_visual: String,
    pub mode_command: String,
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
        }
    }
}

impl Default for UiTheme {
    fn default() -> Self {
        Self {
            active_border: "#4fa6ed".to_string(),   // blue
            inactive_border: "#535965".to_string(), // gray
            selected_bg: "#4fa6ed".to_string(),     // blue
            selected_fg: "#0e1013".to_string(),     // black
            search_match_bg: "#e2b86b".to_string(), // yellow
            search_match_fg: "#0e1013".to_string(), // black
        }
    }
}

impl Default for SyntaxTheme {
    fn default() -> Self {
        Self {
            keyword: "#bf68d9".to_string(),   // purple
            function: "#4fa6ed".to_string(),  // blue
            r#type: "#e2b86b".to_string(),    // yellow
            string: "#8ebd6b".to_string(),    // green
            number: "#cc9057".to_string(),    // orange
            comment: "#535965".to_string(),   // gray
            operator: "#48b0bd".to_string(),  // cyan
            variable: "#e55561".to_string(),  // red
        }
    }
}

impl Default for IconTheme {
    fn default() -> Self {
        Self {
            directory: "#4fa6ed".to_string(),   // blue
            rust: "#dea584".to_string(),        // rust orange
            javascript: "#e2b86b".to_string(),  // yellow
            typescript: "#4fa6ed".to_string(),  // blue
            python: "#4fa6ed".to_string(),      // blue
            go: "#48b0bd".to_string(),          // cyan
            java: "#cc9057".to_string(),        // orange
            c: "#7a818e".to_string(),           // light_gray
            cpp: "#bf68d9".to_string(),         // purple
            ruby: "#e55561".to_string(),        // red
            php: "#bf68d9".to_string(),         // purple
            swift: "#cc9057".to_string(),       // orange
            kotlin: "#bf68d9".to_string(),      // purple
            lua: "#4fa6ed".to_string(),         // blue
            shell: "#8ebd6b".to_string(),       // green
            html: "#cc9057".to_string(),        // orange
            css: "#bf68d9".to_string(),         // purple
            vue: "#8ebd6b".to_string(),         // green
            react: "#48b0bd".to_string(),       // cyan
            svelte: "#cc9057".to_string(),      // orange
            json: "#e2b86b".to_string(),        // yellow
            yaml: "#e55561".to_string(),        // red
            toml: "#cc9057".to_string(),        // orange
            xml: "#4fa6ed".to_string(),         // blue
            markdown: "#48b0bd".to_string(),    // cyan
            config: "#7a818e".to_string(),      // light_gray
            image: "#bf68d9".to_string(),       // purple
            video: "#e55561".to_string(),       // red
            audio: "#cc9057".to_string(),       // orange
            pdf: "#e55561".to_string(),         // red
            archive: "#e2b86b".to_string(),     // yellow
            git: "#cc9057".to_string(),         // orange
            key: "#e2b86b".to_string(),         // yellow
            lock: "#e2b86b".to_string(),        // yellow
            database: "#48b0bd".to_string(),    // cyan
            docker: "#48b0bd".to_string(),      // cyan
            license: "#e2b86b".to_string(),     // yellow
            readme: "#e2b86b".to_string(),      // yellow
            binary: "#7a818e".to_string(),      // light_gray
            font: "#e55561".to_string(),        // red
            default: "#a0a8b7".to_string(),     // fg
        }
    }
}

impl Default for MarkdownTheme {
    fn default() -> Self {
        Self {
            header_1: "#e55561".to_string(),    // red
            header_2: "#e2b86b".to_string(),    // yellow
            header_3: "#8ebd6b".to_string(),    // green
            header_4: "#4fa6ed".to_string(),    // blue
            bold: "#cc9057".to_string(),        // orange
            italic: "#bf68d9".to_string(),      // purple
            code: "#8ebd6b".to_string(),        // green
            code_bg: "#282c34".to_string(),     // bg_lighter
            link: "#4fa6ed".to_string(),        // blue
            link_url: "#535965".to_string(),    // gray
            blockquote: "#7a818e".to_string(),  // light_gray
            list_marker: "#bf68d9".to_string(), // purple
            table_border: "#535965".to_string(),// gray
            horizontal_rule: "#535965".to_string(), // gray
        }
    }
}

impl Default for StatusBarTheme {
    fn default() -> Self {
        Self {
            background: "#181b20".to_string(),  // bg_dark
            foreground: "#a0a8b7".to_string(),  // fg
            mode_normal: "#4fa6ed".to_string(), // blue
            mode_insert: "#8ebd6b".to_string(), // green
            mode_visual: "#bf68d9".to_string(), // purple
            mode_command: "#e2b86b".to_string(),// yellow
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            palette: Palette::default(),
            ui: UiTheme::default(),
            syntax: SyntaxTheme::default(),
            icons: IconTheme::default(),
            markdown: MarkdownTheme::default(),
            statusbar: StatusBarTheme::default(),
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

    // UI styles
    pub active_border: Style,
    pub inactive_border: Style,
    pub selected: Style,
    pub normal: Style,
    pub disabled: Style,
    pub search_highlight: Style,
    pub info: Style,
    pub success: Style,
    pub warning: Style,
    pub error: Style,
    pub muted: Style,
    pub global_search: Style,
    pub local_search: Style,

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
    pub statusbar_mode_normal: Color,
    pub statusbar_mode_insert: Color,
    pub statusbar_mode_visual: Color,
    pub statusbar_mode_command: Color,
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

        // Parse UI colors
        let active_border_color = Self::parse_color(&ui.active_border)?;
        let inactive_border_color = Self::parse_color(&ui.inactive_border)?;
        let selected_bg = Self::parse_color(&ui.selected_bg)?;
        let selected_fg = Self::parse_color(&ui.selected_fg)?;
        let search_match_bg = Self::parse_color(&ui.search_match_bg)?;
        let search_match_fg = Self::parse_color(&ui.search_match_fg)?;

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

            // UI styles
            active_border: Style::default().fg(active_border_color).add_modifier(Modifier::BOLD),
            inactive_border: Style::default().fg(inactive_border_color),
            selected: Style::default().fg(selected_fg).bg(selected_bg).add_modifier(Modifier::BOLD),
            normal: Style::default().fg(fg),
            disabled: Style::default().fg(gray),
            search_highlight: Style::default().fg(search_match_fg).bg(search_match_bg).add_modifier(Modifier::BOLD),
            info: Style::default().fg(cyan),
            success: Style::default().fg(green),
            warning: Style::default().fg(yellow),
            error: Style::default().fg(red),
            muted: Style::default().fg(gray),
            global_search: Style::default().fg(yellow).add_modifier(Modifier::BOLD),
            local_search: Style::default().fg(green).add_modifier(Modifier::BOLD),

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
            statusbar_mode_normal: Self::parse_color(&sb.mode_normal)?,
            statusbar_mode_insert: Self::parse_color(&sb.mode_insert)?,
            statusbar_mode_visual: Self::parse_color(&sb.mode_visual)?,
            statusbar_mode_command: Self::parse_color(&sb.mode_command)?,
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
