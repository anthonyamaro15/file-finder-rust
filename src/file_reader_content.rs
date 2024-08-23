use std::{fs, iter::zip, path::Path};

#[derive(Debug, Clone)]
pub enum FileType {
    FILE,
    CSV,
    ZIP,
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
    pub curr_zip_content: Vec<String>,
}

impl FileContent {
    pub fn new() -> FileContent {
        FileContent {
            file_type: FileType::NotAvailable,
            is_error: false,
            error_message: String::from(""),
            curr_asset_path: String::from(""),
            curr_zip_content: Vec::new(),
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
                    "zip" => FileType::ZIP,
                    _ => FileType::NotAvailable,
                }
            }
            None => FileType::NotAvailable,
        }
    }

    pub fn read_zip_content(&mut self, path: String) -> i32 {
        let filename = std::path::Path::new(&path);
        let file = fs::File::open(filename).unwrap();

        let mut archive = zip::ZipArchive::new(file).unwrap();

        let mut list: Vec<String> = Vec::new();

        for i in 0..archive.len() {
            let file = archive.by_index(i).unwrap();

            let outpath = match file.enclosed_name() {
                Some(fil_path) => fil_path,
                None => continue,
            };

            let name = outpath.display().to_string();

            list.push(name);
        }

        self.curr_zip_content = list;
        0
    }
}
