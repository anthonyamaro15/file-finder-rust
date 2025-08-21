use clap::{Parser, ValueEnum};
use std::path::PathBuf;

/// File Finder - Terminal-based file navigation and search tool
#[derive(Parser)]
#[command(name = "ff")]
#[command(version = "0.1.0")]
#[command(about = "A fast, interactive file finder with editor integration")]
#[command(long_about = "File Finder (ff) is a terminal-based file navigation and search tool built in Rust. 
It provides an interactive file browser with editor integration, search capabilities, and file preview functionality.")]
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

    /// Optional positional path argument (overrides start path if provided)
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

impl CliArgs {
    /// Get the effective start path, prioritizing positional path over --start flag
    pub fn get_effective_start_path(&self) -> Option<PathBuf> {
        self.path.clone().or(self.start.clone())
    }

    /// Parse command line arguments
    pub fn parse_args() -> Self {
        Self::parse()
    }
}
