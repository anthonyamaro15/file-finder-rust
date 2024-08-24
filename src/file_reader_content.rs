use std::{fs, io, iter::zip, path::Path};

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
    pub curr_selected_path: String,
    pub curr_csv_content: Vec<String>,
}

impl FileContent {
    pub fn new() -> FileContent {
        FileContent {
            file_type: FileType::NotAvailable,
            is_error: false,
            error_message: String::from(""),
            curr_asset_path: String::from(""),
            curr_zip_content: Vec::new(),
            curr_selected_path: String::from(""),
            curr_csv_content: Vec::new(),
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
                    "csv" => FileType::CSV,
                    _ => FileType::NotAvailable,
                }
            }
            None => FileType::NotAvailable,
        }
    }

    pub fn read_csv_content(&mut self) {
        let file = fs::File::open(self.curr_selected_path.clone()).unwrap();

        let mut rdr = csv::Reader::from_reader(file);

        let mut file_content: Vec<String> = Vec::new();
        for result in rdr.records() {
            let record = result.unwrap();

            for val in record.iter() {
                file_content.push(val.to_string());
                //println!("result: {:?}", val);
            }
        }

        self.curr_csv_content = file_content;

        //  for result in rdr.recors() {}
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

    pub fn extract_zip_content(&mut self) -> String {
        let fname = std::path::Path::new(&self.curr_selected_path);
        let file = fs::File::open(fname).unwrap();

        let mut archive = zip::ZipArchive::new(file).unwrap();

        for i in 0..archive.len() {
            let mut file = archive.by_index(i).unwrap();

            let outpath = match file.enclosed_name() {
                Some(f_path) => f_path,
                None => continue,
            };

            {
                let comment = file.comment();
                if !comment.is_empty() {
                    println!("File {i} comment: {comment}");
                }
            }

            if file.is_dir() {
                fs::create_dir_all(&outpath).unwrap();
            } else {
                if let Some(p) = outpath.parent() {
                    if !p.exists() {
                        fs::create_dir_all(p).unwrap();
                    }
                }

                let mut outfile = fs::File::create(&outpath).unwrap();
                io::copy(&mut file, &mut outfile).unwrap();
            }

            {
                use std::os::unix::fs::PermissionsExt;

                if let Some(mode) = file.unix_mode() {
                    let _ = fs::set_permissions(&outpath, fs::Permissions::from_mode(mode));
                }
            }
        }
        self.curr_selected_path.clone()
    }
}
