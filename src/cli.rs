use crate::config::Settings;
use clap::{Parser, ValueEnum};
use log::{debug, warn};
use std::{env, io, path::PathBuf};

/// File Finder - Terminal-based file navigation and search tool
#[derive(Parser)]
#[command(name = "ff")]
#[command(version = "0.1.0")]
#[command(about = "A fast, interactive file finder with editor integration")]
#[command(
    long_about = "File Finder (ff) is a terminal-based file navigation and search tool built in Rust. 
It provides an interactive file browser with editor integration, search capabilities, and file preview functionality."
)]
pub struct CliArgs {
    /// Starting directory path
    #[arg(long, short = 's', value_name = "PATH")]
    pub start: Option<PathBuf>,

    /// Theme name or path to theme file
    #[arg(long, short = 't', value_name = "THEME")]
    pub theme: Option<String>,

    /// Editor to use for opening files
    #[arg(long, short = 'e', value_enum)]
    pub editor: Option<Editor>,

    /// Reset configuration to defaults
    #[arg(long)]
    pub reset_config: bool,

    /// Rebuild directory cache
    #[arg(long)]
    pub rebuild_cache: bool,

    /// Optional positional path argument (if --start also provided, --start takes precedence)
    #[arg(value_name = "PATH")]
    pub path: Option<PathBuf>,
}

/// Supported editors
#[derive(Debug, Clone, ValueEnum)]
pub enum Editor {
    Nvim,
    Vscode,
    Zed,
}

impl std::fmt::Display for Editor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Editor::Nvim => write!(f, "nvim"),
            Editor::Vscode => write!(f, "vscode"),
            Editor::Zed => write!(f, "zed"),
        }
    }
}

/// Effective configuration after applying precedence rules
/// Precedence: CLI > ENV (future) > settings.toml > built-in defaults
#[derive(Debug, Clone)]
pub struct EffectiveConfig {
    pub start_path: PathBuf,
    pub theme: Option<String>,
    pub editor: Option<Editor>,
}

impl CliArgs {
    /// Get the effective start path with proper precedence
    /// If both positional [path] and --start are provided, prefer --start with warning
    pub fn get_effective_start_path(&self) -> Option<PathBuf> {
        match (&self.start, &self.path) {
            (Some(start_path), Some(pos_path)) => {
                warn!(
                    "Both --start '{}' and positional path '{}' provided. Using --start.",
                    start_path.display(),
                    pos_path.display()
                );
                Some(start_path.clone())
            }
            (Some(start_path), None) => Some(start_path.clone()),
            (None, Some(pos_path)) => Some(pos_path.clone()),
            (None, None) => None,
        }
    }

    /// Parse command line arguments
    pub fn parse_args() -> Self {
        Self::parse()
    }
}

/// Path normalization utilities
mod path_utils {
    use super::*;
    use std::io;

    /// Normalize a path by:
    /// 1. Expanding ~ to home directory
    /// 2. Converting "." to current directory
    /// 3. Canonicalizing the path if it exists
    pub fn normalize_path(path: &PathBuf) -> io::Result<PathBuf> {
        let expanded = expand_home(path)?;
        let absolute = if expanded == PathBuf::from(".") {
            env::current_dir()?
        } else if expanded.is_relative() {
            env::current_dir()?.join(expanded)
        } else {
            expanded
        };

        // Try to canonicalize, but fall back to the absolute path if it fails
        // (e.g., if the path doesn't exist yet)
        absolute.canonicalize().or_else(|_| Ok(absolute))
    }

    /// Expand ~ to the home directory
    fn expand_home(path: &PathBuf) -> io::Result<PathBuf> {
        if let Some(path_str) = path.to_str() {
            if path_str.starts_with("~/") {
                if let Some(home) = dirs::home_dir() {
                    return Ok(home.join(&path_str[2..]));
                }
            } else if path_str == "~" {
                if let Some(home) = dirs::home_dir() {
                    return Ok(home);
                }
            }
        }
        Ok(path.clone())
    }
}

/// Compute effective configuration by applying precedence rules
/// Precedence: CLI > ENV (future) > settings.toml > built-in defaults
pub fn compute_effective_config(
    cli_args: &CliArgs,
    settings: &Settings,
) -> io::Result<EffectiveConfig> {
    // Determine effective start path
    let start_path = if let Some(cli_path) = cli_args.get_effective_start_path() {
        debug!("Using CLI-provided start path: {}", cli_path.display());
        path_utils::normalize_path(&cli_path)?
    } else {
        debug!("Using settings start path: {}", settings.start_path);
        path_utils::normalize_path(&PathBuf::from(&settings.start_path))?
    };

    // Determine effective theme (CLI > settings > default)
    let theme = cli_args
        .theme
        .clone()
        .or_else(|| Some("onedark".to_string())); // Default theme if none specified

    // Determine effective editor (CLI > settings > none)
    let editor = cli_args.editor.clone();

    debug!(
        "Effective config - Start: {}, Theme: {:?}, Editor: {:?}",
        start_path.display(),
        theme,
        editor
    );

    Ok(EffectiveConfig {
        start_path,
        theme,
        editor,
    })
}
