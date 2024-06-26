use crate::directory_store::DirectoryStore;

extern crate copypasta;

#[derive(Debug, Clone)]
pub enum IDE {
    NVIM,
    VSCODE,
    ZED,
}

#[derive(Debug, Clone)]
pub enum InputMode {
    Normal,
    Editing,
    WatchDelete,
    WatchCreate,
}

#[derive(Debug, Clone)]
pub struct App {
    pub input: String,
    pub character_index: usize,
    pub input_mode: InputMode,
    pub message: Vec<String>,
    pub files: Vec<String>,
    pub read_only_files: Vec<String>,
    pub selected_id: Option<IDE>,
    pub render_popup: bool,
    // create and edit file name
    pub create_edit_file_name: String,
    pub char_index: usize,
    pub is_create_edit_error: bool,
    pub error_message: String,
}

impl App {
    pub fn new(files: Vec<String>) -> Self {
        let files_clone = files.clone();
        Self {
            input: String::new(),
            input_mode: InputMode::Normal,
            message: Vec::new(),
            files,
            read_only_files: files_clone,
            character_index: 0,
            selected_id: None,
            render_popup: false,
            create_edit_file_name: String::new(),
            char_index: 0,
            is_create_edit_error: false,
            error_message: String::new(),
        }
    }

    pub fn move_cursor_left(&mut self) {
        let cursor_moved_left = self.character_index.saturating_sub(1);
        self.character_index = self.clamp_cursor(cursor_moved_left);
    }

    pub fn move_cursor_right(&mut self) {
        let cursor_moved_right = self.character_index.saturating_add(1);
        self.character_index = self.clamp_cursor(cursor_moved_right);
    }

    pub fn enter_char(&mut self, new_char: char, store: DirectoryStore) {
        let index = self.byte_index();
        self.input.insert(index, new_char);
        self.filter_files(self.input.clone(), store);
        self.move_cursor_right();
    }

    pub fn filter_files(&mut self, input: String, store: DirectoryStore) {
        let mut new_files: Vec<String> = Vec::new();

        let r = store.search(&input);
        for file in self.read_only_files.iter() {
            if file.contains(&input) {
                new_files.push(file.clone());
            }
        }

        //self.files = new_files;
        self.files = r;
    }

    pub fn byte_index(&mut self) -> usize {
        self.input
            .char_indices()
            .map(|(i, _)| i)
            .nth(self.character_index)
            .unwrap_or(self.input.len())
    }

    pub fn delete_char(&mut self, store: DirectoryStore) {
        let is_not_cursor_leftmost = self.character_index != 0;
        if is_not_cursor_leftmost {
            let current_index = self.character_index;
            let from_left_to_current_index = current_index - 1;

            let before_char_to_delete = self.input.chars().take(from_left_to_current_index);

            let after_char_to_delete = self.input.chars().skip(current_index);

            self.input = before_char_to_delete.chain(after_char_to_delete).collect();
            self.filter_files(self.input.clone(), store);
            self.move_cursor_left();
        }
    }

    pub fn clamp_cursor(&mut self, new_cursor_pos: usize) -> usize {
        new_cursor_pos.clamp(0, self.input.chars().count())
    }

    pub fn reset_cursor(&mut self) {
        self.character_index = 0;
    }

    pub fn reset_create_edit_values(&mut self) {
        self.create_edit_file_name.clear();
        self.char_index = 0;

        // reset error vaules
        self.is_create_edit_error = false;
        self.error_message = String::new();
    }

    pub fn submit_message(&mut self) {
        self.message.push(self.input.clone());
        self.input.clear();
        self.reset_cursor();
    }

    pub fn validate_user_input(&self, input: &str) -> Option<IDE> {
        match input {
            "nvim" => Some(IDE::NVIM),
            "vscode" => Some(IDE::VSCODE),
            "zed" => Some(IDE::ZED),
            _ => None,
        }
    }

    // TODO: could we combine search, create, edit input field methods?
    // there is a lot of duplication here
    //
    //
    //
    //
    pub fn add_char(&mut self, new_char: char) {
        let index = self.byte_char_index();
        self.create_edit_file_name.insert(index, new_char);
        self.move_create_edit_cursor_right();
    }
    pub fn byte_char_index(&mut self) -> usize {
        self.create_edit_file_name
            .char_indices()
            .map(|(i, _)| i)
            .nth(self.char_index)
            .unwrap_or(self.create_edit_file_name.len())
    }

    pub fn delete_c(&mut self) {
        let is_not_cursor_leftmost = self.char_index != 0;
        if is_not_cursor_leftmost {
            let current_index = self.char_index;
            let from_left_to_current_index = current_index - 1;

            let before_char_to_delete = self
                .create_edit_file_name
                .chars()
                .take(from_left_to_current_index);

            let after_char_to_delete = self.create_edit_file_name.chars().skip(current_index);

            self.create_edit_file_name =
                before_char_to_delete.chain(after_char_to_delete).collect();
            self.move_create_edit_cursor_left();
        }
    }

    pub fn move_create_edit_cursor_left(&mut self) {
        let cursor_moved_left = self.char_index.saturating_sub(1);
        self.char_index = self.clamp_create_edit_cursor(cursor_moved_left);
    }

    pub fn move_create_edit_cursor_right(&mut self) {
        let cursor_moved_right = self.char_index.saturating_add(1);
        self.char_index = self.clamp_create_edit_cursor(cursor_moved_right);
    }

    pub fn clamp_create_edit_cursor(&mut self, new_cursor_pos: usize) -> usize {
        new_cursor_pos.clamp(0, self.create_edit_file_name.chars().count())
    }

    pub fn handle_arguments(&mut self, args: Vec<String>) {
        if args.len() > 1 {
            let ide = &args[1];

            let validated_ide = self.validate_user_input(ide);

            if let Some(selection) = validated_ide {
                //
                self.selected_id = Some(selection);
            } else {
                panic!(
                    "Invalid IDE selection, Please select from the following: nvim, vscode, zed"
                );
            }
        }
    }

    pub fn get_selected_ide(&self) -> Option<String> {
        if let Some(selection) = &self.selected_id {
            match selection {
                IDE::NVIM => Some("nvim".to_string()),
                IDE::VSCODE => Some("vscode".to_string()),
                IDE::ZED => Some("zed".to_string()),
            }
        } else {
            None
        }
    }
}
