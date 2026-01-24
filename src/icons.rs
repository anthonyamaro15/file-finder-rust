//! Nerd Font Icon System
//!
//! Provides file-type-aware icons with auto-detection for nerd font support.
//! Falls back to emoji icons when nerd fonts are not available.

use std::env;
use std::path::Path;

use crate::config::settings::NerdFontSetting;

/// Icon set for a specific file type
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

    /// Get icon for a file based on its path
    pub fn get_for_path(&self, path: &Path, is_directory: bool) -> &'static str {
        if is_directory {
            return self.get(icons::DIRECTORY);
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
            let icon = match name.as_str() {
                "dockerfile" | "dockerfile.dev" | "dockerfile.prod" => icons::DOCKERFILE,
                "docker-compose.yml" | "docker-compose.yaml" | "compose.yml" | "compose.yaml" => {
                    icons::DOCKER
                }
                "makefile" | "gnumakefile" => icons::MAKEFILE,
                "cargo.toml" | "cargo.lock" => icons::RUST,
                "package.json" | "package-lock.json" => icons::JAVASCRIPT,
                "tsconfig.json" => icons::TYPESCRIPT,
                "readme" | "readme.md" | "readme.txt" | "readme.rst" => icons::README,
                "license" | "license.md" | "license.txt" | "copying" => icons::LICENSE,
                ".gitignore" | ".gitattributes" | ".gitmodules" => icons::GITIGNORE,
                ".env" | ".env.local" | ".env.development" | ".env.production" => icons::CONFIG,
                "requirements.txt" | "pyproject.toml" | "setup.py" => icons::PYTHON,
                "gemfile" | "gemfile.lock" => icons::RUBY,
                _ => icons::FILE_DEFAULT,
            };

            if icon != icons::FILE_DEFAULT {
                return self.get(icon);
            }
        }

        // Check by extension
        let icon = match extension.as_deref() {
            // Rust
            Some("rs") => icons::RUST,

            // JavaScript/TypeScript
            Some("js" | "mjs" | "cjs") => icons::JAVASCRIPT,
            Some("ts" | "mts" | "cts") => icons::TYPESCRIPT,
            Some("jsx") => icons::REACT,
            Some("tsx") => icons::REACT,

            // Web
            Some("html" | "htm") => icons::HTML,
            Some("css") => icons::CSS,
            Some("scss" | "sass") => icons::SCSS,
            Some("vue") => icons::VUE,
            Some("svelte") => icons::SVELTE,

            // Python
            Some("py" | "pyw" | "pyi") => icons::PYTHON,

            // Go
            Some("go") => icons::GO,

            // Java/Kotlin
            Some("java") => icons::JAVA,
            Some("kt" | "kts") => icons::KOTLIN,

            // C/C++
            Some("c" | "h") => icons::C,
            Some("cpp" | "cc" | "cxx" | "hpp" | "hxx") => icons::CPP,

            // Ruby
            Some("rb" | "rake" | "gemspec") => icons::RUBY,

            // PHP
            Some("php") => icons::PHP,

            // Swift
            Some("swift") => icons::SWIFT,

            // Lua
            Some("lua") => icons::LUA,

            // Shell
            Some("sh" | "bash" | "zsh" | "fish") => icons::SHELL,

            // Data/Config
            Some("json" | "jsonc" | "json5") => icons::JSON,
            Some("yaml" | "yml") => icons::YAML,
            Some("toml") => icons::TOML,
            Some("xml" | "plist") => icons::XML,
            Some("md" | "markdown" | "rst") => icons::MARKDOWN,
            Some("ini" | "cfg" | "conf" | "config") => icons::CONFIG,

            // Media
            Some("png" | "jpg" | "jpeg" | "gif" | "bmp" | "svg" | "ico" | "webp") => icons::IMAGE,
            Some("mp4" | "avi" | "mkv" | "mov" | "wmv" | "flv" | "webm") => icons::VIDEO,
            Some("mp3" | "wav" | "ogg" | "flac" | "aac" | "m4a") => icons::AUDIO,
            Some("pdf") => icons::PDF,

            // Archives
            Some("zip" | "tar" | "gz" | "bz2" | "xz" | "7z" | "rar") => icons::ARCHIVE,

            // Git
            Some("git") => icons::GIT,

            // Security
            Some("pem" | "crt" | "cer" | "key" | "pub") => icons::KEY,

            // Database
            Some("sql" | "sqlite" | "db") => icons::DATABASE,

            // Fonts
            Some("ttf" | "otf" | "woff" | "woff2") => icons::FONT,

            // Binary/Executable
            Some("exe" | "dll" | "so" | "dylib" | "bin" | "o" | "a") => icons::BINARY,

            // Lock files
            Some("lock") => icons::LOCK,

            // Default
            _ => icons::FILE_DEFAULT,
        };

        self.get(icon)
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
