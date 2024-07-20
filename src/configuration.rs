use std::{
    fs::{self, File},
    io::{BufReader, BufWriter},
    path::Path,
};

use dirs::home_dir;
use serde::{Deserialize, Serialize};

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
        let home_dir = home_dir().unwrap();
        let append_config_to_cache =
            format!("{}/.config/ff/cache_directory.json", home_dir.display());
        let append_config_to_settings = format!("{}/.config/ff/settings.json", home_dir.display());
        let append_to_start_path = format!("{}/Desktop", home_dir.display());
        //let append_to_start_path = format!("{}/Desktop", home_dir.display());

        config.start_path = ".".to_string();
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
    pub fn handle_settings_configuration(&mut self) {
        let append_config_dir = format!("{}/.config/ff", self.root_dir);
        let find_dir = Path::new(&append_config_dir).try_exists();

        let get_result = match find_dir {
            Ok(res) => {
                if res {
                    true
                } else {
                    false
                }
            }
            Err(error) => {
                println!("error:  {:?}", error);
                false
            }
        };

        if get_result {
            // read config content, and cache content,
            match self.load_settings_from_file(&self.settings_path.to_owned()) {
                Ok(get_config) => {
                    self.start_path = get_config.start_path;
                    self.ignore_directories = get_config.ignore_directories;
                    self.root_dir = get_config.root_dir;
                    self.cache_directory = get_config.cache_directory;
                    self.settings_path = get_config.settings_path;
                }
                Err(err) => {
                    println!("error {:?}", err);
                }
            }
        } else {
            // create directory and files
            self.create_files();
        }
    }

    fn create_files(&self) {
        let config_root_dir = format!("{}/.config/ff", self.root_dir);
        match fs::create_dir_all(config_root_dir) {
            Ok(_) => {}
            Err(error) => println!("error {:?}", error),
        };

        // only create file if it does not exist
        if !Path::new(&self.settings_path).exists() {
            match self.write_settings_to_file() {
                Ok(_) => {}
                Err(error) => {
                    println!("error {:?}", error);
                }
            }
        }
    }

    pub fn write_settings_to_file(&self) -> anyhow::Result<()> {
        match File::create(self.settings_path.to_owned()) {
            Ok(file) => {
                let writer = BufWriter::new(file);

                serde_json::to_writer(writer, self).unwrap();
            }
            Err(error) => {
                println!("error {:?}", error);
            }
        }

        Ok(())
    }

    pub fn load_settings_from_file(&self, path: &str) -> anyhow::Result<Configuration> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);

        let settings = serde_json::from_reader(reader)?;
        Ok(settings)
    }
}
