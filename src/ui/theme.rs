//! One Dark Darker Theme
//!
//! Centralized theme colors based on navarasu/onedark.nvim "darker" style.
//! All UI colors should reference this module for consistency.

use ratatui::style::{Color, Modifier, Style};

/// One Dark Darker color palette
pub mod palette {
    use ratatui::style::Color;

    // Core colors
    pub const RED: Color = Color::Rgb(229, 85, 97);        // #e55561
    pub const GREEN: Color = Color::Rgb(142, 189, 107);    // #8ebd6b
    pub const YELLOW: Color = Color::Rgb(226, 184, 107);   // #e2b86b
    pub const BLUE: Color = Color::Rgb(79, 166, 237);      // #4fa6ed
    pub const PURPLE: Color = Color::Rgb(191, 104, 217);   // #bf68d9
    pub const CYAN: Color = Color::Rgb(72, 176, 189);      // #48b0bd
    pub const ORANGE: Color = Color::Rgb(204, 144, 87);    // #cc9057

    // Grayscale
    pub const GRAY: Color = Color::Rgb(83, 89, 101);       // #535965
    pub const LIGHT_GRAY: Color = Color::Rgb(122, 129, 142); // #7a818e
    pub const FG: Color = Color::Rgb(160, 168, 183);       // #a0a8b7
    pub const BG: Color = Color::Rgb(31, 35, 41);          // #1f2329
    pub const BG_DARK: Color = Color::Rgb(24, 27, 32);     // #181b20
    pub const BG_LIGHTER: Color = Color::Rgb(40, 44, 52);  // #282c34
    pub const BG_HIGHLIGHT: Color = Color::Rgb(48, 54, 63); // #30363f
    pub const SELECTION: Color = Color::Rgb(50, 54, 65);   // #323641
    pub const BLACK: Color = Color::Rgb(14, 16, 19);       // #0e1013
}

/// UI element styles
pub struct OneDarkTheme;

impl OneDarkTheme {
    // ─────────────────────────────────────────────────────────────
    // Border styles
    // ─────────────────────────────────────────────────────────────

    pub fn active_border() -> Style {
        Style::default()
            .fg(palette::BLUE)
            .add_modifier(Modifier::BOLD)
    }

    pub fn inactive_border() -> Style {
        Style::default().fg(palette::GRAY)
    }

    // ─────────────────────────────────────────────────────────────
    // Search styles
    // ─────────────────────────────────────────────────────────────

    pub fn global_search() -> Style {
        Style::default()
            .fg(palette::YELLOW)
            .add_modifier(Modifier::BOLD)
    }

    pub fn local_search() -> Style {
        Style::default()
            .fg(palette::GREEN)
            .add_modifier(Modifier::BOLD)
    }

    pub fn search_highlight() -> Style {
        Style::default()
            .fg(palette::BLACK)
            .bg(palette::YELLOW)
            .add_modifier(Modifier::BOLD)
    }

    // ─────────────────────────────────────────────────────────────
    // Selection and list styles
    // ─────────────────────────────────────────────────────────────

    pub fn selected() -> Style {
        Style::default()
            .fg(palette::BLACK)
            .bg(palette::BLUE)
            .add_modifier(Modifier::BOLD)
    }

    pub fn normal() -> Style {
        Style::default().fg(palette::FG)
    }

    pub fn disabled() -> Style {
        Style::default().fg(palette::GRAY)
    }

    // ─────────────────────────────────────────────────────────────
    // Status and info styles
    // ─────────────────────────────────────────────────────────────

    pub fn info() -> Style {
        Style::default().fg(palette::CYAN)
    }

    pub fn success() -> Style {
        Style::default().fg(palette::GREEN)
    }

    pub fn warning() -> Style {
        Style::default().fg(palette::YELLOW)
    }

    pub fn error() -> Style {
        Style::default().fg(palette::RED)
    }

    // ─────────────────────────────────────────────────────────────
    // File list styles
    // ─────────────────────────────────────────────────────────────

    /// Directory name color (also used for directory icons)
    pub fn directory() -> Style {
        Style::default().fg(palette::BLUE)
    }

    /// Regular file name color
    pub fn file() -> Style {
        Style::default().fg(palette::FG)
    }

    /// Muted text (file sizes, metadata, secondary info)
    pub fn muted() -> Style {
        Style::default().fg(palette::GRAY)
    }

    /// Dotfile style (dimmed for hidden files starting with '.')
    pub fn dotfile() -> Style {
        Style::default().fg(palette::GRAY)
    }

    /// Active pane title style (for borderless mode)
    pub fn active_title() -> Style {
        Style::default()
            .fg(palette::BLUE)
            .add_modifier(Modifier::BOLD)
    }

    /// Inactive pane title style (for borderless mode)
    pub fn inactive_title() -> Style {
        Style::default().fg(palette::GRAY)
    }

    /// Score/rank display
    pub fn score() -> Style {
        Style::default().fg(palette::LIGHT_GRAY)
    }
}

/// Markdown rendering colors
pub mod markdown {
    use super::palette;
    use ratatui::style::Color;

    pub const HEADER_1: Color = palette::RED;
    pub const HEADER_2: Color = palette::YELLOW;
    pub const HEADER_3: Color = palette::GREEN;
    pub const HEADER_4: Color = palette::BLUE;
    pub const BOLD: Color = palette::ORANGE;
    pub const ITALIC: Color = palette::PURPLE;
    pub const CODE: Color = palette::GREEN;
    pub const CODE_BG: Color = palette::BG_LIGHTER;
    pub const LINK: Color = palette::BLUE;
    pub const LINK_URL: Color = palette::GRAY;
    pub const BLOCKQUOTE: Color = palette::LIGHT_GRAY;
    pub const LIST_MARKER: Color = palette::PURPLE;
    pub const TABLE_BORDER: Color = palette::GRAY;
    pub const HORIZONTAL_RULE: Color = palette::GRAY;
}

/// Icon colors for file types
pub mod icons {
    use super::palette;
    use ratatui::style::Color;

    // Directories
    pub const DIRECTORY: Color = palette::BLUE;

    // Programming languages - use language brand colors where appropriate
    pub const RUST: Color = Color::Rgb(222, 165, 132);      // Rust orange
    pub const JAVASCRIPT: Color = palette::YELLOW;
    pub const TYPESCRIPT: Color = palette::BLUE;
    pub const PYTHON: Color = palette::BLUE;
    pub const GO: Color = palette::CYAN;
    pub const JAVA: Color = palette::ORANGE;
    pub const C: Color = palette::LIGHT_GRAY;
    pub const CPP: Color = palette::PURPLE;
    pub const RUBY: Color = palette::RED;
    pub const PHP: Color = palette::PURPLE;
    pub const SWIFT: Color = palette::ORANGE;
    pub const KOTLIN: Color = palette::PURPLE;
    pub const LUA: Color = palette::BLUE;
    pub const SHELL: Color = palette::GREEN;

    // Web
    pub const HTML: Color = palette::ORANGE;
    pub const CSS: Color = palette::PURPLE;
    pub const VUE: Color = palette::GREEN;
    pub const REACT: Color = palette::CYAN;
    pub const SVELTE: Color = palette::ORANGE;

    // Data/Config
    pub const JSON: Color = palette::YELLOW;
    pub const YAML: Color = palette::RED;
    pub const TOML: Color = palette::ORANGE;
    pub const XML: Color = palette::BLUE;
    pub const MARKDOWN: Color = palette::CYAN;
    pub const CONFIG: Color = palette::LIGHT_GRAY;

    // Media
    pub const IMAGE: Color = palette::PURPLE;
    pub const VIDEO: Color = palette::RED;
    pub const AUDIO: Color = palette::ORANGE;
    pub const PDF: Color = palette::RED;

    // Archives
    pub const ARCHIVE: Color = palette::YELLOW;

    // Git
    pub const GIT: Color = palette::ORANGE;

    // Security
    pub const KEY: Color = palette::YELLOW;
    pub const LOCK: Color = palette::YELLOW;

    // Database
    pub const DATABASE: Color = palette::CYAN;

    // Docker
    pub const DOCKER: Color = palette::CYAN;

    // License/Readme
    pub const LICENSE: Color = palette::YELLOW;
    pub const README: Color = palette::YELLOW;

    // Binary
    pub const BINARY: Color = palette::LIGHT_GRAY;

    // Font
    pub const FONT: Color = palette::RED;

    // Default file
    pub const DEFAULT: Color = palette::FG;
}

/// Status bar colors
pub mod statusbar {
    use super::palette;
    use ratatui::style::Color;

    pub const BACKGROUND: Color = palette::BG_DARK;
    pub const FOREGROUND: Color = palette::FG;
    pub const MODE_NORMAL: Color = palette::BLUE;
    pub const MODE_INSERT: Color = palette::GREEN;
    pub const MODE_VISUAL: Color = palette::PURPLE;
    pub const MODE_COMMAND: Color = palette::YELLOW;
}
