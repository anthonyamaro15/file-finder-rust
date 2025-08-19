use std::{
    fs::{self, File},
    io::{BufReader, BufWriter},
    path::Path,
};

use dirs::home_dir;
use serde::{Deserialize, Serialize};

use crate::errors::{AppResult, AppError};

#[derive(Serialize, Deserialize, Default, Clone)]

pub struct Configuration {
    pub start_path: String,
    pub ignore_directories: Vec<String>,
    pub root_dir: String,
    pub cache_directory: String,
    pub settings_path: String,
}

impl Configuration {
    pub fn new() -> Self {
        let mut config = Configuration {
            start_path: "".to_string(),
            ignore_directories: Vec::new(),
            root_dir: String::from("."),
            cache_directory: String::from(""),
            settings_path: String::from(""),
        };

        config.set_default_ignore_directories();
        
        // Use a fallback if home_dir() returns None
        let home_dir = match home_dir() {
            Some(dir) => dir,
            None => {
                eprintln!("Warning: Could not determine home directory, using current directory");
                std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
            }
        };
        
        let append_config_to_cache =
            format!("{}/.config/ff/cache_directory.json", home_dir.display());
        let append_config_to_settings = format!("{}/.config/ff/settings.json", home_dir.display());
        let append_to_start_path = format!("{}/Desktop", home_dir.display());

        config.start_path = append_to_start_path;
        config.root_dir = home_dir.display().to_string();
        config.cache_directory = append_config_to_cache;
        config.settings_path = append_config_to_settings;
        config
    }

    // TODO: should we cache all directories when first loading the app? or is there a better way to do this?
    fn set_default_ignore_directories(&mut self) {
        let default_ignore_dirs = vec![
            "node_modules",
            "bower_components",
            "obj",
            "maui",
            "target",
            ".venv",
            "src",
            "cdn",
            ".git",
            ".sauce",
            ".husky",
            ".vscode",
            ".zed",
            "cypress",
            "dist-prod",
        ];

        self.ignore_directories = default_ignore_dirs.iter().map(|s| s.to_string()).collect();
    }
    pub fn handle_settings_configuration(&mut self) -> AppResult<()> {
        let append_config_dir = format!("{}/.config/ff", self.root_dir);
        let config_dir_exists = Path::new(&append_config_dir).try_exists()
            .map_err(|e| AppError::Configuration {
                message: format!("Failed to check if config directory exists '{}': {}", append_config_dir, e)
            })?;

        if config_dir_exists {
            // Try to load existing settings, but handle errors gracefully
            match self.load_settings_from_file(&self.settings_path) {
                Ok(get_config) => {
                    // Apply loaded config
                    self.start_path = get_config.start_path;
                    self.ignore_directories = get_config.ignore_directories;
                    self.root_dir = get_config.root_dir;
                    self.cache_directory = get_config.cache_directory;
                    self.settings_path = get_config.settings_path;
                }
                Err(err) => {
                    // Log error but continue with default settings
                    eprintln!("Warning: Failed to load settings, using defaults: {}", err);
                    // Create new settings file with current defaults
                    self.create_files()?;
                }
            }
        } else {
            // Create directory and files since they don't exist
            self.create_files()?;
        }
        
        Ok(())
    }

    fn create_files(&self) -> AppResult<()> {
        let config_root_dir = format!("{}/.config/ff", self.root_dir);
        fs::create_dir_all(&config_root_dir)
            .map_err(|e| AppError::Configuration {
                message: format!("Failed to create config directory '{}': {}", config_root_dir, e)
            })?;

        // only create file if it does not exist
        if !Path::new(&self.settings_path).exists() {
            self.write_settings_to_file()?;
        }
        
        Ok(())
    }

    pub fn write_settings_to_file(&self) -> AppResult<()> {
        let file = File::create(self.settings_path.to_owned())
            .map_err(|e| AppError::Configuration {
                message: format!("Failed to create settings file: {}", e)
            })?;
        
        let writer = BufWriter::new(file);
        serde_json::to_writer(writer, self)
            .map_err(|e| AppError::Configuration {
                message: format!("Failed to write settings to file: {}", e)
            })?;
        
        Ok(())
    }

    pub fn load_settings_from_file(&self, path: &str) -> AppResult<Configuration> {
        let file = File::open(path)
            .map_err(|e| AppError::Configuration {
                message: format!("Failed to open settings file '{}': {}", path, e)
            })?;
        
        let reader = BufReader::new(file);
        let settings = serde_json::from_reader(reader)
            .map_err(|e| AppError::Configuration {
                message: format!("Failed to parse settings file '{}': {}", path, e)
            })?;
        
        Ok(settings)
    }
}
