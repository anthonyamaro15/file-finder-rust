//! Nerd Font Icon System
//!
//! Provides file-type-aware icons with auto-detection for nerd font support.
//! Falls back to emoji icons when nerd fonts are not available.

use std::env;
use std::path::Path;

use ratatui::style::Color;

use crate::config::settings::NerdFontSetting;

/// Colored icon result - contains the icon string and its color
#[derive(Debug, Clone, Copy)]
pub struct ColoredIcon {
    pub icon: &'static str,
    pub color: Color,
}

impl ColoredIcon {
    pub const fn new(icon: &'static str, color: Color) -> Self {
        Self { icon, color }
    }
}

/// Icon set for a specific file type with color
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IconPair {
    pub nerd: &'static str,
    pub emoji: &'static str,
}

impl IconPair {
    const fn new(nerd: &'static str, emoji: &'static str) -> Self {
        Self { nerd, emoji }
    }
}

/// Colors for different file types (yazi-inspired)
pub mod colors {
    use ratatui::style::Color;

    // Directories - Blue
    pub const DIRECTORY: Color = Color::Rgb(97, 175, 239);     // #61AFEF - bright blue

    // Programming languages
    pub const RUST: Color = Color::Rgb(222, 165, 132);         // #DEA584 - rust orange
    pub const JAVASCRIPT: Color = Color::Rgb(241, 224, 90);    // #F1E05A - JS yellow
    pub const TYPESCRIPT: Color = Color::Rgb(49, 120, 198);    // #3178C6 - TS blue
    pub const PYTHON: Color = Color::Rgb(53, 114, 165);        // #3572A5 - Python blue
    pub const GO: Color = Color::Rgb(0, 173, 216);             // #00ADD8 - Go cyan
    pub const JAVA: Color = Color::Rgb(176, 114, 25);          // #B07219 - Java orange
    pub const C: Color = Color::Rgb(85, 85, 85);               // #555555 - C gray
    pub const CPP: Color = Color::Rgb(243, 75, 125);           // #F34B7D - C++ pink
    pub const RUBY: Color = Color::Rgb(112, 21, 22);           // #701516 - Ruby red
    pub const PHP: Color = Color::Rgb(79, 93, 149);            // #4F5D95 - PHP purple
    pub const SWIFT: Color = Color::Rgb(240, 81, 56);          // #F05138 - Swift orange
    pub const KOTLIN: Color = Color::Rgb(169, 123, 255);       // #A97BFF - Kotlin purple
    pub const LUA: Color = Color::Rgb(0, 0, 128);              // #000080 - Lua blue
    pub const SHELL: Color = Color::Rgb(137, 224, 81);         // #89E051 - Shell green

    // Web
    pub const HTML: Color = Color::Rgb(227, 76, 38);           // #E34C26 - HTML orange
    pub const CSS: Color = Color::Rgb(86, 61, 124);            // #563D7C - CSS purple
    pub const VUE: Color = Color::Rgb(65, 184, 131);           // #41B883 - Vue green
    pub const REACT: Color = Color::Rgb(97, 218, 251);         // #61DAFB - React cyan
    pub const SVELTE: Color = Color::Rgb(255, 62, 0);          // #FF3E00 - Svelte orange

    // Data/Config
    pub const JSON: Color = Color::Rgb(203, 203, 65);          // #CBCB41 - JSON yellow
    pub const YAML: Color = Color::Rgb(203, 23, 30);           // #CB171E - YAML red
    pub const TOML: Color = Color::Rgb(156, 66, 33);           // #9C4221 - TOML brown
    pub const XML: Color = Color::Rgb(0, 96, 172);             // #0060AC - XML blue
    pub const MARKDOWN: Color = Color::Rgb(8, 51, 68);         // #083344 - Markdown teal
    pub const CONFIG: Color = Color::Rgb(109, 117, 126);       // #6D757E - Config gray

    // Media
    pub const IMAGE: Color = Color::Rgb(168, 100, 253);        // #A864FD - Image purple
    pub const VIDEO: Color = Color::Rgb(253, 100, 100);        // #FD6464 - Video red
    pub const AUDIO: Color = Color::Rgb(253, 183, 100);        // #FDB764 - Audio orange
    pub const PDF: Color = Color::Rgb(236, 47, 41);            // #EC2F29 - PDF red

    // Archives
    pub const ARCHIVE: Color = Color::Rgb(175, 180, 43);       // #AFB42B - Archive olive

    // Git
    pub const GIT: Color = Color::Rgb(241, 80, 47);            // #F1502F - Git orange

    // Security
    pub const KEY: Color = Color::Rgb(255, 213, 79);           // #FFD54F - Key yellow
    pub const LOCK: Color = Color::Rgb(255, 193, 7);           // #FFC107 - Lock amber

    // Database
    pub const DATABASE: Color = Color::Rgb(0, 188, 212);       // #00BCD4 - Database cyan

    // Docker
    pub const DOCKER: Color = Color::Rgb(13, 183, 237);        // #0DB7ED - Docker blue

    // License/Readme
    pub const LICENSE: Color = Color::Rgb(214, 157, 133);      // #D69D85 - License tan
    pub const README: Color = Color::Rgb(255, 200, 100);       // #FFC864 - Readme gold

    // Binary
    pub const BINARY: Color = Color::Rgb(144, 164, 174);       // #90A4AE - Binary gray

    // Font
    pub const FONT: Color = Color::Rgb(244, 67, 54);           // #F44336 - Font red

    // Default file
    pub const DEFAULT: Color = Color::Rgb(171, 178, 191);      // #ABB2BF - Default gray
}

/// Common icons used throughout the UI
pub mod icons {
    use super::IconPair;

    // File types
    pub const DIRECTORY: IconPair = IconPair::new("\u{f07b}", "\u{1f4c1}"); //  vs 📁
    pub const DIRECTORY_OPEN: IconPair = IconPair::new("\u{f07c}", "\u{1f4c2}"); //  vs 📂
    pub const FILE_DEFAULT: IconPair = IconPair::new("\u{f15b}", "\u{1f4c4}"); //  vs 📄
    pub const FILE_TEXT: IconPair = IconPair::new("\u{f15c}", "\u{1f4c4}"); //  vs 📄
    pub const FILE_CODE: IconPair = IconPair::new("\u{f1c9}", "\u{1f4c4}"); //  vs 📄

    // Programming languages
    pub const RUST: IconPair = IconPair::new("\u{e7a8}", "\u{1f980}"); //  vs 🦀
    pub const JAVASCRIPT: IconPair = IconPair::new("\u{e74e}", "\u{1f4c4}"); //  vs 📄
    pub const TYPESCRIPT: IconPair = IconPair::new("\u{e628}", "\u{1f4c4}"); //  vs 📄
    pub const PYTHON: IconPair = IconPair::new("\u{e73c}", "\u{1f40d}"); //  vs 🐍
    pub const GO: IconPair = IconPair::new("\u{e627}", "\u{1f4c4}"); //  vs 📄
    pub const JAVA: IconPair = IconPair::new("\u{e738}", "\u{2615}"); //  vs ☕
    pub const C: IconPair = IconPair::new("\u{e61e}", "\u{1f4c4}"); //  vs 📄
    pub const CPP: IconPair = IconPair::new("\u{e61d}", "\u{1f4c4}"); //  vs 📄
    pub const RUBY: IconPair = IconPair::new("\u{e739}", "\u{1f48e}"); //  vs 💎
    pub const PHP: IconPair = IconPair::new("\u{e73d}", "\u{1f4c4}"); //  vs 📄
    pub const SWIFT: IconPair = IconPair::new("\u{e755}", "\u{1f4c4}"); //  vs 📄
    pub const KOTLIN: IconPair = IconPair::new("\u{e634}", "\u{1f4c4}"); //  vs 📄
    pub const LUA: IconPair = IconPair::new("\u{e620}", "\u{1f319}"); //  vs 🌙
    pub const SHELL: IconPair = IconPair::new("\u{f489}", "\u{1f4c4}"); //  vs 📄
    pub const HTML: IconPair = IconPair::new("\u{e736}", "\u{1f310}"); //  vs 🌐
    pub const CSS: IconPair = IconPair::new("\u{e749}", "\u{1f3a8}"); //  vs 🎨
    pub const SCSS: IconPair = IconPair::new("\u{e74b}", "\u{1f3a8}"); //  vs 🎨
    pub const VUE: IconPair = IconPair::new("\u{e6a0}", "\u{1f4c4}"); //  vs 📄
    pub const REACT: IconPair = IconPair::new("\u{e7ba}", "\u{269b}"); //  vs ⚛
    pub const SVELTE: IconPair = IconPair::new("\u{e697}", "\u{1f4c4}"); //  vs 📄

    // Data and config
    pub const JSON: IconPair = IconPair::new("\u{e60b}", "\u{1f4c4}"); //  vs 📄
    pub const YAML: IconPair = IconPair::new("\u{e6a8}", "\u{1f4c4}"); //  vs 📄
    pub const TOML: IconPair = IconPair::new("\u{e6b2}", "\u{2699}"); //  vs ⚙
    pub const XML: IconPair = IconPair::new("\u{e619}", "\u{1f4c4}"); //  vs 📄
    pub const MARKDOWN: IconPair = IconPair::new("\u{e73e}", "\u{1f4dd}"); //  vs 📝
    pub const CONFIG: IconPair = IconPair::new("\u{e615}", "\u{2699}"); //  vs ⚙

    // Media
    pub const IMAGE: IconPair = IconPair::new("\u{f1c5}", "\u{1f5bc}"); //  vs 🖼
    pub const VIDEO: IconPair = IconPair::new("\u{f1c8}", "\u{1f3ac}"); //  vs 🎬
    pub const AUDIO: IconPair = IconPair::new("\u{f1c7}", "\u{1f3b5}"); //  vs 🎵
    pub const PDF: IconPair = IconPair::new("\u{f1c1}", "\u{1f4d5}"); //  vs 📕
    pub const ARCHIVE: IconPair = IconPair::new("\u{f1c6}", "\u{1f4e6}"); //  vs 📦

    // Version control
    pub const GIT: IconPair = IconPair::new("\u{f1d3}", "\u{1f4c4}"); //  vs 📄
    pub const GITIGNORE: IconPair = IconPair::new("\u{e65d}", "\u{1f4c4}"); //  vs 📄

    // Special
    pub const LOCK: IconPair = IconPair::new("\u{f023}", "\u{1f512}"); //  vs 🔒
    pub const KEY: IconPair = IconPair::new("\u{f084}", "\u{1f511}"); //  vs 🔑
    pub const DATABASE: IconPair = IconPair::new("\u{f1c0}", "\u{1f5c4}"); //  vs 🗄
    pub const DOCKER: IconPair = IconPair::new("\u{f308}", "\u{1f433}"); //  vs 🐳
    pub const LICENSE: IconPair = IconPair::new("\u{f2c2}", "\u{1f4dc}"); //  vs 📜
    pub const README: IconPair = IconPair::new("\u{f48a}", "\u{1f4d6}"); //  vs 📖
    pub const BINARY: IconPair = IconPair::new("\u{f471}", "\u{1f4c4}"); //  vs 📄
    pub const FONT: IconPair = IconPair::new("\u{f031}", "\u{1f520}"); //  vs 🔠
    pub const MAKEFILE: IconPair = IconPair::new("\u{e673}", "\u{1f6e0}"); //  vs 🛠
    pub const DOCKERFILE: IconPair = IconPair::new("\u{f308}", "\u{1f433}"); //  vs 🐳

    // UI indicators
    pub const SYMLINK: IconPair = IconPair::new("\u{f0c1}", "\u{1f517}"); //  vs 🔗
    pub const HIDDEN: IconPair = IconPair::new("\u{f070}", "\u{1f441}"); //  vs 👁
    pub const EXECUTABLE: IconPair = IconPair::new("\u{f489}", "\u{2699}"); //  vs ⚙
}

/// Icon provider that handles nerd font detection and fallback
#[derive(Debug, Clone)]
pub struct IconProvider {
    use_nerd_fonts: bool,
}

impl IconProvider {
    /// Create a new icon provider based on the setting
    pub fn new(setting: &NerdFontSetting) -> Self {
        let use_nerd_fonts = match setting {
            NerdFontSetting::Always => true,
            NerdFontSetting::Never => false,
            NerdFontSetting::Auto => Self::detect_nerd_font_support(),
        };

        Self { use_nerd_fonts }
    }

    /// Detect if the terminal likely supports nerd fonts
    fn detect_nerd_font_support() -> bool {
        // Check TERM_PROGRAM for known nerd-font-friendly terminals
        if let Ok(term_program) = env::var("TERM_PROGRAM") {
            let known_terminals = [
                "iTerm.app",
                "Alacritty",
                "kitty",
                "WezTerm",
                "Hyper",
                "vscode",
                "Tabby",
                "Warp",
            ];
            if known_terminals.iter().any(|t| term_program.contains(t)) {
                return true;
            }
        }

        // Check for kitty terminal
        if env::var("KITTY_WINDOW_ID").is_ok() {
            return true;
        }

        // Check for WezTerm
        if env::var("WEZTERM_PANE").is_ok() {
            return true;
        }

        // Check for Alacritty
        if env::var("ALACRITTY_SOCKET").is_ok() {
            return true;
        }

        // Check LC_TERMINAL for additional detection
        if let Ok(lc_terminal) = env::var("LC_TERMINAL") {
            if lc_terminal.contains("iTerm") {
                return true;
            }
        }

        // Default to true for modern terminals - most users who install CLI tools
        // likely have nerd fonts. Can be overridden with settings.
        true
    }

    /// Get the appropriate icon string for an IconPair
    pub fn get(&self, icon: IconPair) -> &'static str {
        if self.use_nerd_fonts {
            icon.nerd
        } else {
            icon.emoji
        }
    }

    /// Get icon for a file based on its path (returns just the icon string)
    pub fn get_for_path(&self, path: &Path, is_directory: bool) -> &'static str {
        self.get_colored_for_path(path, is_directory).icon
    }

    /// Get colored icon for a file based on its path
    pub fn get_colored_for_path(&self, path: &Path, is_directory: bool) -> ColoredIcon {
        if is_directory {
            return ColoredIcon::new(self.get(icons::DIRECTORY), colors::DIRECTORY);
        }

        // Get extension
        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase());

        // Get filename for special files
        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.to_lowercase());

        // Check for special filenames first
        if let Some(ref name) = filename {
            let result = match name.as_str() {
                "dockerfile" | "dockerfile.dev" | "dockerfile.prod" => {
                    Some((icons::DOCKERFILE, colors::DOCKER))
                }
                "docker-compose.yml" | "docker-compose.yaml" | "compose.yml" | "compose.yaml" => {
                    Some((icons::DOCKER, colors::DOCKER))
                }
                "makefile" | "gnumakefile" => Some((icons::MAKEFILE, colors::CONFIG)),
                "cargo.toml" => Some((icons::RUST, colors::RUST)),
                "cargo.lock" => Some((icons::LOCK, colors::LOCK)),
                "package.json" | "package-lock.json" => Some((icons::JAVASCRIPT, colors::JAVASCRIPT)),
                "tsconfig.json" => Some((icons::TYPESCRIPT, colors::TYPESCRIPT)),
                "readme" | "readme.md" | "readme.txt" | "readme.rst" => {
                    Some((icons::README, colors::README))
                }
                "license" | "license.md" | "license.txt" | "copying" => {
                    Some((icons::LICENSE, colors::LICENSE))
                }
                ".gitignore" | ".gitattributes" | ".gitmodules" => Some((icons::GITIGNORE, colors::GIT)),
                ".env" | ".env.local" | ".env.development" | ".env.production" => {
                    Some((icons::CONFIG, colors::CONFIG))
                }
                "requirements.txt" | "pyproject.toml" | "setup.py" => {
                    Some((icons::PYTHON, colors::PYTHON))
                }
                "gemfile" | "gemfile.lock" => Some((icons::RUBY, colors::RUBY)),
                _ => None,
            };

            if let Some((icon, color)) = result {
                return ColoredIcon::new(self.get(icon), color);
            }
        }

        // Check by extension
        let (icon, color) = match extension.as_deref() {
            // Rust
            Some("rs") => (icons::RUST, colors::RUST),

            // JavaScript/TypeScript
            Some("js" | "mjs" | "cjs") => (icons::JAVASCRIPT, colors::JAVASCRIPT),
            Some("ts" | "mts" | "cts") => (icons::TYPESCRIPT, colors::TYPESCRIPT),
            Some("jsx") => (icons::REACT, colors::REACT),
            Some("tsx") => (icons::REACT, colors::REACT),

            // Web
            Some("html" | "htm") => (icons::HTML, colors::HTML),
            Some("css") => (icons::CSS, colors::CSS),
            Some("scss" | "sass") => (icons::SCSS, colors::CSS),
            Some("vue") => (icons::VUE, colors::VUE),
            Some("svelte") => (icons::SVELTE, colors::SVELTE),

            // Python
            Some("py" | "pyw" | "pyi") => (icons::PYTHON, colors::PYTHON),

            // Go
            Some("go") => (icons::GO, colors::GO),

            // Java/Kotlin
            Some("java") => (icons::JAVA, colors::JAVA),
            Some("kt" | "kts") => (icons::KOTLIN, colors::KOTLIN),

            // C/C++
            Some("c" | "h") => (icons::C, colors::C),
            Some("cpp" | "cc" | "cxx" | "hpp" | "hxx") => (icons::CPP, colors::CPP),

            // Ruby
            Some("rb" | "rake" | "gemspec") => (icons::RUBY, colors::RUBY),

            // PHP
            Some("php") => (icons::PHP, colors::PHP),

            // Swift
            Some("swift") => (icons::SWIFT, colors::SWIFT),

            // Lua
            Some("lua") => (icons::LUA, colors::LUA),

            // Shell
            Some("sh" | "bash" | "zsh" | "fish") => (icons::SHELL, colors::SHELL),

            // Data/Config
            Some("json" | "jsonc" | "json5") => (icons::JSON, colors::JSON),
            Some("yaml" | "yml") => (icons::YAML, colors::YAML),
            Some("toml") => (icons::TOML, colors::TOML),
            Some("xml" | "plist") => (icons::XML, colors::XML),
            Some("md" | "markdown" | "rst") => (icons::MARKDOWN, colors::MARKDOWN),
            Some("ini" | "cfg" | "conf" | "config") => (icons::CONFIG, colors::CONFIG),

            // Media
            Some("png" | "jpg" | "jpeg" | "gif" | "bmp" | "svg" | "ico" | "webp") => {
                (icons::IMAGE, colors::IMAGE)
            }
            Some("mp4" | "avi" | "mkv" | "mov" | "wmv" | "flv" | "webm") => {
                (icons::VIDEO, colors::VIDEO)
            }
            Some("mp3" | "wav" | "ogg" | "flac" | "aac" | "m4a") => (icons::AUDIO, colors::AUDIO),
            Some("pdf") => (icons::PDF, colors::PDF),

            // Archives
            Some("zip" | "tar" | "gz" | "bz2" | "xz" | "7z" | "rar") => {
                (icons::ARCHIVE, colors::ARCHIVE)
            }

            // Git
            Some("git") => (icons::GIT, colors::GIT),

            // Security
            Some("pem" | "crt" | "cer" | "key" | "pub") => (icons::KEY, colors::KEY),

            // Database
            Some("sql" | "sqlite" | "db") => (icons::DATABASE, colors::DATABASE),

            // Fonts
            Some("ttf" | "otf" | "woff" | "woff2") => (icons::FONT, colors::FONT),

            // Binary/Executable
            Some("exe" | "dll" | "so" | "dylib" | "bin" | "o" | "a") => {
                (icons::BINARY, colors::BINARY)
            }

            // Lock files
            Some("lock") => (icons::LOCK, colors::LOCK),

            // Default
            _ => (icons::FILE_DEFAULT, colors::DEFAULT),
        };

        ColoredIcon::new(self.get(icon), color)
    }

    /// Check if using nerd fonts
    pub fn uses_nerd_fonts(&self) -> bool {
        self.use_nerd_fonts
    }
}

impl Default for IconProvider {
    fn default() -> Self {
        Self::new(&NerdFontSetting::Auto)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_icon_provider_always() {
        let provider = IconProvider::new(&NerdFontSetting::Always);
        assert!(provider.uses_nerd_fonts());
    }

    #[test]
    fn test_icon_provider_never() {
        let provider = IconProvider::new(&NerdFontSetting::Never);
        assert!(!provider.uses_nerd_fonts());
    }

    #[test]
    fn test_directory_icon() {
        let provider = IconProvider::new(&NerdFontSetting::Always);
        let icon = provider.get_for_path(Path::new("/some/dir"), true);
        assert_eq!(icon, icons::DIRECTORY.nerd);

        let provider = IconProvider::new(&NerdFontSetting::Never);
        let icon = provider.get_for_path(Path::new("/some/dir"), true);
        assert_eq!(icon, icons::DIRECTORY.emoji);
    }

    #[test]
    fn test_rust_file_icon() {
        let provider = IconProvider::new(&NerdFontSetting::Always);
        let icon = provider.get_for_path(Path::new("main.rs"), false);
        assert_eq!(icon, icons::RUST.nerd);
    }

    #[test]
    fn test_special_filename_icon() {
        let provider = IconProvider::new(&NerdFontSetting::Always);
        let icon = provider.get_for_path(Path::new("Dockerfile"), false);
        assert_eq!(icon, icons::DOCKERFILE.nerd);
    }
}
