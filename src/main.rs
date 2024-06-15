use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use std::{
    fs,
    io::{self, Stdout},
    path::PathBuf,
    process::Command,
};
use walkdir::WalkDir;

use ratatui::prelude::*;

use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    style::{Color, Modifier, Style},
    text::Text,
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Terminal,
};

#[derive(Debug, Clone)]
enum InputMode {
    Normal,
    Editing,
}

#[derive(Debug, Clone)]
struct App {
    input: String,
    character_index: usize,
    input_mode: InputMode,
    message: Vec<String>,
    files: Vec<String>,
    read_only_files: Vec<String>,
    count_previous_navigation: usize,
}

impl App {
    fn new(files: Vec<String>) -> Self {
        let files_clone = files.clone();
        Self {
            input: String::new(),
            input_mode: InputMode::Normal,
            message: Vec::new(),
            files,
            read_only_files: files_clone,
            character_index: 0,
            count_previous_navigation: 0,
        }
    }

    fn move_cursor_left(&mut self) {
        let cursor_moved_left = self.character_index.saturating_sub(1);
        self.character_index = self.clamp_cursor(cursor_moved_left);
    }

    fn move_cursor_right(&mut self) {
        let cursor_moved_right = self.character_index.saturating_add(1);
        self.character_index = self.clamp_cursor(cursor_moved_right);
    }

    fn enter_char(&mut self, new_char: char) {
        let index = self.byte_index();
        self.input.insert(index, new_char);
        self.filter_files(self.input.clone());
        self.move_cursor_right();
    }
    // TODO: research if there is a way to implement a faster solution to this
    fn traverse_directory(&mut self, dir: &str, query: &str) -> Vec<String> {
        let mut results = Vec::new();

        for entry in WalkDir::new(dir) {
            if let Ok(entry) = entry {
                if entry.file_type().is_file() {
                    if let Some(file_name) = entry.path().file_name() {
                        if file_name.to_string_lossy().contains(query) {
                            results.push(entry.path().display().to_string());
                        }
                    }
                }
            }
        }

        results
    }
    fn filter_files(&mut self, input: String) {
        let mut new_files: Vec<String> = Vec::new();

        for file in self.read_only_files.iter() {
            if file.contains(&input) {
                new_files.push(file.clone());
            }
        }
        self.files = new_files;
    }

    fn byte_index(&mut self) -> usize {
        self.input
            .char_indices()
            .map(|(i, _)| i)
            .nth(self.character_index)
            .unwrap_or(self.input.len())
    }

    fn delete_char(&mut self) {
        let is_not_cursor_leftmost = self.character_index != 0;
        if is_not_cursor_leftmost {
            let current_index = self.character_index;
            let from_left_to_current_index = current_index - 1;

            let before_char_to_delete = self.input.chars().take(from_left_to_current_index);

            let after_char_to_delete = self.input.chars().skip(current_index);

            self.input = before_char_to_delete.chain(after_char_to_delete).collect();
            self.filter_files(self.input.clone());
            self.move_cursor_left();
        }
    }

    fn clamp_cursor(&mut self, new_cursor_pos: usize) -> usize {
        new_cursor_pos.clamp(0, self.input.chars().count())
    }

    fn reset_cursor(&mut self) {
        self.character_index = 0;
    }

    fn submit_message(&mut self) {
        self.message.push(self.input.clone());
        self.input.clear();
        self.reset_cursor();
    }
}

fn convert_file_path_to_string(entries: Vec<PathBuf>) -> Vec<String> {
    let mut file_strings: Vec<String> = Vec::new();

    for value in entries.iter() {
        if value.is_dir() {
            let val = value.clone().into_os_string().to_str().unwrap().to_string();
            file_strings.push(val.clone());
        }
    }

    file_strings
}
fn handle_file_selection(
    file: &str,
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
) -> anyhow::Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    Command::new("nvim")
        .arg(file)
        .status()
        .expect("Failed to open file");

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    terminal.clear()?;
    Ok(())
}

fn generate_path_based_on_navegation_count(count: usize) -> String {
    let mut path = String::new();

    for _ in 0..count {
        path.push_str("../");
    }

    path
}

fn get_inner_files_info(file: String) -> anyhow::Result<Option<Vec<String>>> {
    let entries = match fs::read_dir(file) {
        Ok(en) => {
            let val = en.map(|res| res.map(|e| e.path())).collect();
            match val {
                Ok(v) => v,
                Err(e) => {
                    println!("Error: {}", e);
                    return Ok(None);
                }
            }
        }
        Err(e) => {
            println!("Error: {}", e);
            return Ok(None);
        }
    };

    let file_strings = convert_file_path_to_string(entries);
    Ok(Some(file_strings))
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Setup terminal
    let entries = fs::read_dir(".")?
        .map(|res| res.map(|e| e.path()))
        .collect::<Result<Vec<_>, io::Error>>()?;

    let file_strings = convert_file_path_to_string(entries);
    let mut app = App::new(file_strings.clone());
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Initial selected state
    let mut state = ListState::default();
    state.select(Some(0)); // Select the first item by default

    // Main loop
    loop {
        // Filtered items based on input
        let filtered_items: Vec<ListItem> = app
            .files
            .iter()
            .map(|file| ListItem::new(file.clone()))
            .collect();

        // Draw UI
        terminal.draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .margin(2)
                .constraints(
                    [
                        Constraint::Length(2),
                        Constraint::Length(3),
                        Constraint::Min(1),
                    ]
                    .as_ref(),
                )
                .split(f.size());

            let (msg, style) = match app.input_mode {
                InputMode::Normal => (
                    vec![
                        "Exit (q)".bold(),
                        " find (i)".bold(),
                        app.input.clone().bold(),
                        " Enter to select file (enter)".bold(),
                    ],
                    Style::default().add_modifier(Modifier::RAPID_BLINK),
                ),
                InputMode::Editing => (vec!["Normal Mode (Esc)".bold()], Style::default()),
            };

            // Input field
            let input_block = Paragraph::new(app.input.clone())
                .block(Block::default().borders(Borders::ALL).title("Find"))
                .style(match app.input_mode {
                    InputMode::Editing => Style::default().fg(Color::Green),
                    InputMode::Normal => Style::default().fg(Color::White),
                });

            // List of filtered items
            let list_block = List::new(filtered_items.clone())
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title("Filtered List"),
                )
                .highlight_style(
                    Style::default()
                        .bg(Color::Rgb(50, 105, 168))
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                )
                .highlight_symbol(">>")
                .style(match app.input_mode {
                    InputMode::Normal => Style::default().fg(Color::Green),
                    InputMode::Editing => Style::default().fg(Color::White),
                });
            let text = Text::from(Line::from(msg)).patch_style(style);
            let help_message = Paragraph::new(text);
            f.render_widget(help_message, chunks[0]);

            f.render_widget(input_block, chunks[1]);
            //f.render_widget(list_block, chunks[1]);
            f.render_stateful_widget(list_block, chunks[2], &mut state);
        })?;

        // Handle input
        if let Event::Key(key) = event::read()? {
            match app.input_mode {
                InputMode::Normal => match key.code {
                    KeyCode::Char('i') => {
                        app.input_mode = InputMode::Editing;
                    }
                    KeyCode::Char('q') => {
                        break;
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        let i = match state.selected() {
                            Some(i) => {
                                if i >= app.files.len() - 1 {
                                    0
                                } else {
                                    i + 1
                                }
                            }
                            None => 0,
                        };
                        state.select(Some(i));
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        let i = match state.selected() {
                            Some(i) => {
                                if i == 0 {
                                    app.files.len() - 1
                                } else {
                                    i - 1
                                }
                            }
                            None => 0,
                        };
                        state.select(Some(i));
                    }
                    KeyCode::Char('h') => {
                        app.count_previous_navigation += 1;

                        let current_path =
                            generate_path_based_on_navegation_count(app.count_previous_navigation);

                        let files_strings = get_inner_files_info(current_path).unwrap();

                        if let Some(f_s) = files_strings {
                            app.read_only_files = f_s.clone();
                            app.files = f_s;
                        }
                    }
                    KeyCode::Char('l') => {
                        if app.count_previous_navigation > 0 {
                            app.count_previous_navigation -= 1;
                        }
                        let selected = &app.files[state.selected().unwrap()];
                        let files_strings = get_inner_files_info(selected.to_owned()).unwrap();

                        if let Some(files_strs) = files_strings {
                            app.read_only_files = files_strs.clone();
                            app.files = files_strs;
                        }
                    }
                    KeyCode::Enter => {
                        let selected = &app.files[state.selected().unwrap()];

                        app.input = selected.clone();

                        let _ = handle_file_selection(selected, &mut terminal);
                        break;
                    }
                    _ => {}
                },

                InputMode::Editing if key.kind == KeyEventKind::Press => match key.code {
                    KeyCode::Enter => app.submit_message(),
                    KeyCode::Char(to_insert) => {
                        app.enter_char(to_insert);
                    }
                    KeyCode::Backspace => {
                        app.delete_char();
                    }
                    KeyCode::Left => {
                        app.move_cursor_left();
                    }

                    KeyCode::Right => {
                        app.move_cursor_right();
                    }
                    KeyCode::Esc => {
                        app.input_mode = InputMode::Normal;
                    }

                    _ => {}
                },
                InputMode::Editing => {}
            }
        }
    }

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}
