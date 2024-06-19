use std::{
    fs::{self, File},
    path::Path,
};

use dirs::home_dir;

#[derive(Debug, Clone)]
pub struct Configuration {
    pub start_path: Option<String>,
    pub ignore_directories: Vec<String>,
    pub root_dir: String,
    pub cache_directory: String,
}

impl Configuration {
    pub fn new() -> Self {
        let config = Configuration {
            start_path: None,
            ignore_directories: Vec::new(),
            root_dir: String::from("../../"),
            cache_directory: String::from("./config/ff/directory_cache.json"),
            //cache_directory: String::from("directory_cache.json"),
        };

        let home_dir = home_dir().unwrap();
        let append_config_to_cache =
            format!("{}/.config/ff/cache_directory.json", home_dir.display());

        //config.root_dir = home_dir.display().to_string();
        //config.cache_directory = append_config_to_cache;
        config
    }

    pub fn handle_settings_configuration(&self) {
        let append_config_dir = "/.config/ff";
        //let append_config_dir = format!("{}/.config/ff", self.root_dir);
        let find_dir = Path::new(&append_config_dir).try_exists();

        let get_result = match find_dir {
            Ok(res) => {
                if res {
                    true
                } else {
                    false
                }
            }
            Err(_) => false,
        };

        if get_result {
            // read config content, and cache content,
            println!("we have a result {:?}", get_result);
        } else {
            self.create_files(&append_config_dir);
            // create directory and files
            println!("we dont ahve a result");
        }
    }

    fn create_files(&self, root_dir: &str) {
        //let cache_directory = "~/.config/ff/directory_cache.json";
        let append_root_dir = "./config/ff/settings.json";
        //let append_root_dir = format!("{}/settings.json", root_dir);
        //let settings_directory = "~/.config/ff/settings.json";

        fs::create_dir_all("./config/ff").unwrap();
        // only create file if it does not exist

        if !Path::new(&append_root_dir).exists() {
            File::create_new(append_root_dir).unwrap();
        }
    }
}
