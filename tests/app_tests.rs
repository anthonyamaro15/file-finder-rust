mod common;

use file_finder::app::{App, InputMode, IDE};
use file_finder::directory_store::DirectoryStore;

#[cfg(test)]
mod app_creation_tests {
    use super::*;

    #[test]
    fn test_app_new_with_files() {
        let files = vec![
            "/path/to/file1.txt".to_string(),
            "/path/to/file2.rs".to_string(),
            "/path/to/directory".to_string(),
        ];

        let app = App::new(files.clone());

        assert_eq!(app.files, files);
        assert_eq!(app.read_only_files, files);
        assert_eq!(app.input_mode, InputMode::Normal);
        assert_eq!(app.input, "");
        assert_eq!(app.character_index, 0);
        assert_eq!(app.selected_id, None);
        assert!(!app.render_popup);
        assert!(!app.show_hidden_files);
        assert_eq!(app.curr_index, Some(0));
        assert_eq!(app.filtered_indexes.len(), files.len());
    }

    #[test]
    fn test_app_new_empty_files() {
        let files = vec![];
        let app = App::new(files);

        assert!(app.files.is_empty());
        assert!(app.read_only_files.is_empty());
        assert!(app.filtered_indexes.is_empty());
        assert!(app.file_read_only_label_list.is_empty());
    }

    #[test]
    fn test_app_new_creates_correct_labels() {
        let files = vec![
            "/home/user/documents/test.txt".to_string(),
            "/home/user/pictures/image.png".to_string(),
        ];

        let app = App::new(files);

        assert_eq!(app.file_read_only_label_list.len(), 2);
        assert_eq!(app.file_read_only_label_list[0], "test.txt");
        assert_eq!(app.file_read_only_label_list[1], "image.png");
    }
}

#[cfg(test)]
mod cursor_movement_tests {
    use super::*;

    #[test]
    fn test_move_cursor_left() {
        let mut app = App::new(vec![]);
        app.input = "hello".to_string();
        app.character_index = 3;

        app.move_cursor_left();
        assert_eq!(app.character_index, 2);
    }

    #[test]
    fn test_move_cursor_left_at_start() {
        let mut app = App::new(vec![]);
        app.input = "hello".to_string();
        app.character_index = 0;

        app.move_cursor_left();
        assert_eq!(app.character_index, 0); // Should not go below 0
    }

    #[test]
    fn test_move_cursor_right() {
        let mut app = App::new(vec![]);
        app.input = "hello".to_string();
        app.character_index = 2;

        app.move_cursor_right();
        assert_eq!(app.character_index, 3);
    }

    #[test]
    fn test_move_cursor_right_at_end() {
        let mut app = App::new(vec![]);
        app.input = "hello".to_string();
        app.character_index = 5; // At end

        app.move_cursor_right();
        assert_eq!(app.character_index, 5); // Should not go beyond string length
    }

    #[test]
    fn test_reset_cursor() {
        let mut app = App::new(vec![]);
        app.character_index = 10;

        app.reset_cursor();
        assert_eq!(app.character_index, 0);
    }
}

#[cfg(test)]
mod input_handling_tests {
    use super::*;

    #[test]
    fn test_enter_char_at_end() {
        let files = vec!["/path/test.txt".to_string(), "/path/example.rs".to_string()];
        let mut app = App::new(files);
        let store = DirectoryStore::new();

        app.input = "test".to_string();
        app.character_index = 4;

        app.enter_char('s', store);

        assert_eq!(app.input, "tests");
        assert_eq!(app.character_index, 5);
    }

    #[test]
    fn test_enter_char_in_middle() {
        let files = vec!["/path/test.txt".to_string()];
        let mut app = App::new(files);
        let store = DirectoryStore::new();

        app.input = "tst".to_string();
        app.character_index = 1;

        app.enter_char('e', store);

        assert_eq!(app.input, "test");
        assert_eq!(app.character_index, 2);
    }

    #[test]
    fn test_delete_char() {
        let files = vec!["/path/test.txt".to_string()];
        let mut app = App::new(files);
        let store = DirectoryStore::new();

        app.input = "test".to_string();
        app.character_index = 2;

        app.delete_char(store);

        assert_eq!(app.input, "tst");
        assert_eq!(app.character_index, 1);
    }

    #[test]
    fn test_delete_char_at_start() {
        let files = vec!["/path/test.txt".to_string()];
        let mut app = App::new(files);
        let store = DirectoryStore::new();

        app.input = "test".to_string();
        app.character_index = 0;

        app.delete_char(store);

        assert_eq!(app.input, "test"); // Should not delete when at start
        assert_eq!(app.character_index, 0);
    }
}

#[cfg(test)]
mod filtering_tests {
    use super::*;

    #[test]
    fn test_filter_files_basic() {
        let files = vec![
            "/path/test.txt".to_string(),
            "/path/example.rs".to_string(),
            "/path/another_test.py".to_string(),
        ];
        let mut app = App::new(files);
        let store = DirectoryStore::new();

        app.filter_files("test".to_string(), store);

        // Should find indexes 0 and 2 (both contain "test")
        assert_eq!(app.filtered_indexes.len(), 2);
        assert!(app.filtered_indexes.contains(&0));
        assert!(app.filtered_indexes.contains(&2));
    }

    #[test]
    fn test_filter_files_no_matches() {
        let files = vec![
            "/path/example.rs".to_string(),
            "/path/document.txt".to_string(),
        ];
        let mut app = App::new(files);
        let store = DirectoryStore::new();

        app.filter_files("nonexistent".to_string(), store);

        assert!(app.filtered_indexes.is_empty());
    }

    #[test]
    fn test_filter_files_empty_input() {
        let files = vec!["/path/test.txt".to_string(), "/path/example.rs".to_string()];
        let mut app = App::new(files);
        let store = DirectoryStore::new();

        app.filter_files("".to_string(), store);

        // Empty input should match all files
        assert_eq!(app.filtered_indexes.len(), 2);
    }

    #[test]
    fn test_filter_files_case_sensitive() {
        let files = vec![
            "/path/Test.txt".to_string(),
            "/path/test.txt".to_string(),
            "/path/example.rs".to_string(),
        ];
        let mut app = App::new(files);
        let store = DirectoryStore::new();

        app.filter_files("Test".to_string(), store);

        // Should only match the exact case
        assert_eq!(app.filtered_indexes.len(), 1);
        assert_eq!(app.filtered_indexes[0], 0);
    }
}

#[cfg(test)]
mod validation_tests {
    use super::*;

    #[test]
    fn test_validate_user_input_nvim() {
        let app = App::new(vec![]);
        let result = app.validate_user_input("nvim");
        assert_eq!(result, Some(IDE::NVIM));
    }

    #[test]
    fn test_validate_user_input_vscode() {
        let app = App::new(vec![]);
        let result = app.validate_user_input("vscode");
        assert_eq!(result, Some(IDE::VSCODE));
    }

    #[test]
    fn test_validate_user_input_zed() {
        let app = App::new(vec![]);
        let result = app.validate_user_input("zed");
        assert_eq!(result, Some(IDE::ZED));
    }

    #[test]
    fn test_validate_user_input_invalid() {
        let app = App::new(vec![]);
        let result = app.validate_user_input("invalid");
        assert_eq!(result, None);
    }

    #[test]
    fn test_validate_user_input_empty() {
        let app = App::new(vec![]);
        let result = app.validate_user_input("");
        assert_eq!(result, None);
    }
}

#[cfg(test)]
mod state_management_tests {
    use super::*;

    #[test]
    fn test_reset_create_edit_values() {
        let mut app = App::new(vec![]);
        app.create_edit_file_name = "test.txt".to_string();
        app.char_index = 5;
        app.is_create_edit_error = true;
        app.error_message = "Error occurred".to_string();

        app.reset_create_edit_values();

        assert_eq!(app.create_edit_file_name, "");
        assert_eq!(app.char_index, 0);
        assert!(!app.is_create_edit_error);
        assert_eq!(app.error_message, "");
    }

    #[test]
    fn test_submit_message() {
        let mut app = App::new(vec![]);
        app.input = "test message".to_string();
        app.character_index = 5;

        app.submit_message();

        assert_eq!(app.message.len(), 1);
        assert_eq!(app.message[0], "test message");
        assert_eq!(app.input, "");
        assert_eq!(app.character_index, 0);
    }

    #[test]
    fn test_submit_multiple_messages() {
        let mut app = App::new(vec![]);

        app.input = "first message".to_string();
        app.submit_message();

        app.input = "second message".to_string();
        app.submit_message();

        assert_eq!(app.message.len(), 2);
        assert_eq!(app.message[0], "first message");
        assert_eq!(app.message[1], "second message");
    }
}

#[cfg(test)]
mod byte_index_tests {
    use super::*;

    #[test]
    fn test_byte_index_normal_ascii() {
        let mut app = App::new(vec![]);
        app.input = "hello".to_string();
        app.character_index = 3;

        let byte_idx = app.byte_index();
        assert_eq!(byte_idx, 3);
    }

    #[test]
    fn test_byte_index_at_end() {
        let mut app = App::new(vec![]);
        app.input = "hello".to_string();
        app.character_index = 5;

        let byte_idx = app.byte_index();
        assert_eq!(byte_idx, 5);
    }

    #[test]
    fn test_byte_index_beyond_string() {
        let mut app = App::new(vec![]);
        app.input = "hello".to_string();
        app.character_index = 10; // Beyond string length

        let byte_idx = app.byte_index();
        assert_eq!(byte_idx, 5); // Should return string length
    }

    #[test]
    fn test_byte_index_empty_string() {
        let mut app = App::new(vec![]);
        app.input = "".to_string();
        app.character_index = 0;

        let byte_idx = app.byte_index();
        assert_eq!(byte_idx, 0);
    }
}
