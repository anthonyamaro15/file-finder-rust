pub mod settings;
pub mod theme;

pub use settings::{Configuration, Settings};
pub use theme::{Theme, ThemeColors};

use crate::errors::{AppError, AppResult};
use std::fs;
use std::path::Path;

/// Initialize configuration directories and files on first run
pub fn ensure_config_directories() -> AppResult<()> {
    let config_dir = get_config_dir()?;
    let themes_dir = config_dir.join("themes");

    // Create ~/.config/ff/ if it doesn't exist
    if !config_dir.exists() {
        fs::create_dir_all(&config_dir).map_err(|e| AppError::Configuration {
            message: format!(
                "Failed to create config directory '{}': {}",
                config_dir.display(),
                e
            ),
        })?;
    }

    // Create ~/.config/ff/themes/ if it doesn't exist
    if !themes_dir.exists() {
        fs::create_dir_all(&themes_dir).map_err(|e| AppError::Configuration {
            message: format!(
                "Failed to create themes directory '{}': {}",
                themes_dir.display(),
                e
            ),
        })?;
    }

    Ok(())
}

/// Get the configuration directory path (~/.config/ff/)
pub fn get_config_dir() -> AppResult<std::path::PathBuf> {
    let home_dir = dirs::home_dir().ok_or_else(|| AppError::Configuration {
        message: "Could not determine home directory".to_string(),
    })?;

    Ok(home_dir.join(".config").join("ff"))
}

/// Get the themes directory path (~/.config/ff/themes/)
pub fn get_themes_dir() -> AppResult<std::path::PathBuf> {
    Ok(get_config_dir()?.join("themes"))
}

/// Reset configuration by removing all config files and regenerating defaults
pub fn reset_configuration() -> AppResult<()> {
    let config_dir = get_config_dir()?;

    if config_dir.exists() {
        fs::remove_dir_all(&config_dir).map_err(|e| AppError::Configuration {
            message: format!(
                "Failed to remove config directory '{}': {}",
                config_dir.display(),
                e
            ),
        })?;
    }

    // Recreate directories and default files
    ensure_config_directories()?;
    Settings::create_default_if_missing()?;
    Theme::create_default_if_missing("onedark")?;

    Ok(())
}
