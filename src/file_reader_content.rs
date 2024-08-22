use std::{fs, path::Path};

#[derive(Debug, Clone)]
pub enum FileType {
    FILE,
    CSV,
    SHAPE,
    PNG,
    NotAvailable,
    DEFAULT,
    IMG,
}

pub struct FileContent {
    pub file_type: FileType,
    pub is_error: bool,
    pub error_message: String,
    pub curr_asset_path: String,
}

impl FileContent {
    pub fn new() -> FileContent {
        FileContent {
            file_type: FileType::NotAvailable,
            is_error: false,
            error_message: String::from(""),
            curr_asset_path: String::from(""),
        }
    }
    pub fn is_curr_path_file(path: String) -> bool {
        match fs::metadata(path) {
            Ok(file) => {
                let file_t = file.file_type();
                if file_t.is_file() {
                    true
                } else {
                    false
                }
            }
            Err(_) => false,
        }
    }

    pub fn read_file_content(&mut self, path: String) -> String {
        let content = match fs::read_to_string(path) {
            Ok(file_content) => file_content,
            Err(err) => {
                let err_kind = err.kind().to_string();
                let format_error = format!("Encounter Error: '{}'", err_kind);
                self.is_error = true;
                self.error_message = String::from("");
                return format_error;
            }
        };
        content
    }

    pub fn get_file_extension(&mut self, path: String) -> FileType {
        let file_extension = Path::new(&path).extension();

        match file_extension {
            Some(extention) => {
                let convert_to_str = extention.to_str().unwrap();

                match convert_to_str {
                    "js" | "ts" | "html" | "yml" | "json" | "css" => FileType::FILE,
                    "png" => FileType::IMG,
                    _ => FileType::NotAvailable,
                }
            }
            None => FileType::NotAvailable,
        }
    }
}
