use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

use crate::config::get_config_dir;
use crate::errors::{AppError, AppResult};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Settings {
    pub start_path: String,
    pub root_dir: String,
    pub cache_directory: String,
    pub settings_path: String,
    pub ignore_directories: Vec<String>,
    pub theme: String,
}

impl Default for Settings {
    fn default() -> Self {
        let home_dir = dirs::home_dir()
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")));
        
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
                "src".to_string(),
                "cdn".to_string(),
                ".git".to_string(),
                ".sauce".to_string(),
                ".husky".to_string(),
                ".vscode".to_string(),
                ".zed".to_string(),
                "cypress".to_string(),
                "dist-prod".to_string(),
            ],
            theme: "onedark".to_string(),
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
        let content = fs::read_to_string(&path)
            .map_err(|e| AppError::Configuration {
                message: format!("Failed to read settings file '{}': {}", path.as_ref().display(), e)
            })?;

        let settings: Self = toml::from_str(&content)
            .map_err(|e| AppError::Configuration {
                message: format!("Failed to parse TOML settings file '{}': {}", path.as_ref().display(), e)
            })?;

        Ok(settings)
    }

    /// Save current settings to TOML file
    pub fn save(&self) -> AppResult<()> {
        let config_dir = get_config_dir()?;
        let settings_path = config_dir.join("settings.toml");
        
        self.save_to_file(&settings_path)
    }

    /// Save settings to a specific TOML file path
    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> AppResult<()> {
        let toml_content = toml::to_string_pretty(self)
            .map_err(|e| AppError::Configuration {
                message: format!("Failed to serialize settings to TOML: {}", e)
            })?;

        fs::write(&path, toml_content)
            .map_err(|e| AppError::Configuration {
                message: format!("Failed to write settings to file '{}': {}", path.as_ref().display(), e)
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
            
            println!("Created default settings file at: {}", settings_path.display());
        }
        
        Ok(())
    }

    /// Update a specific setting and save to file
    pub fn update_setting<F>(&mut self, updater: F) -> AppResult<()>
    where
        F: FnOnce(&mut Self),
    {
        updater(self);
        self.save()
    }
}

// Backward compatibility: alias Configuration to Settings
pub type Configuration = Settings;

impl Configuration {
    pub fn new() -> Self {
        Settings::default()
    }

    pub fn handle_settings_configuration(&mut self) -> AppResult<()> {
        match Settings::load() {
            Ok(loaded_settings) => {
                *self = loaded_settings;
                Ok(())
            }
            Err(e) => {
                eprintln!("Warning: Failed to load settings, using defaults: {}", e);
                Settings::create_default_if_missing()?;
                Ok(())
            }
        }
    }

    pub fn write_settings_to_file(&self) -> AppResult<()> {
        self.save()
    }

    pub fn load_settings_from_file(&self, path: &str) -> AppResult<Configuration> {
        Settings::load_from_file(path)
    }
}
