use app::{App, InputMode};
use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use file_reader_content::{FileContent, FileType};
use image::ImageReader;
use rayon::prelude::*;
use std::{
    env,
    fs::{self, File, Metadata},
    io::{self, ErrorKind, Stdout},
    path::{Path, PathBuf},
    process::Command,
};
use walkdir::WalkDir;

use ratatui::{prelude::*, widgets::Clear};

use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    style::{Color, Modifier, Style},
    text::Text,
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Terminal,
};

use ratatui_image::{picker::Picker, protocol::StatefulProtocol, StatefulImage};

use crate::directory_store::{
    build_directory_from_store, load_directory_from_file, save_directory_to_file,
};

extern crate copypasta;
use copypasta::{ClipboardContext, ClipboardProvider};

mod app;
mod configuration;
mod directory_store;
mod file_reader_content;
mod ui;

#[derive(Clone)]
enum SortType {
    ASC,
    DESC,
}

#[derive(Clone)]
enum SortBy {
    Name,
    Size,
    DateAdded,
    Default,
}

#[derive(Clone)]
struct ImageGenerator {
    image: Option<Box<dyn StatefulProtocol>>,
}

impl ImageGenerator {
    pub fn new() -> ImageGenerator {
        ImageGenerator { image: None }
    }

    pub fn load_img(&mut self, path: String) {
        let mut picker = Picker::new((8, 12));
        picker.guess_protocol();

        let dyn_img = ImageReader::open(path)
            .expect("unable to open img")
            .decode()
            .expect("unable to decode image");
        let image = picker.new_resize_protocol(dyn_img);

        self.image = Some(image);

        //ImageGenerator {image: Some(image)}
    }
}

fn sort_entries_by_type(
    sort_by: SortBy,
    sort_type: SortType,
    mut entries: Vec<PathBuf>,
) -> Vec<PathBuf> {
    match sort_type {
        SortType::ASC => match sort_by {
            SortBy::Name => entries.sort_by(|a, b| {
                a.file_name()
                    .unwrap()
                    .to_ascii_lowercase()
                    .cmp(&b.file_name().unwrap().to_ascii_lowercase())
            }),
            SortBy::Size => entries.sort_by(|a, b| {
                a.metadata()
                    .ok()
                    .map(|meta| meta.len())
                    .unwrap_or(0)
                    .cmp(&b.metadata().ok().map(|meta| meta.len()).unwrap_or(0))
            }),

            SortBy::DateAdded => entries.sort_by(|a, b| {
                a.metadata()
                    .ok()
                    .and_then(|meta| meta.created().ok())
                    .unwrap_or(std::time::SystemTime::now())
                    .cmp(
                        &b.metadata()
                            .ok()
                            .and_then(|meta| meta.created().ok())
                            .unwrap_or(std::time::SystemTime::now()),
                    )
            }),
            _ => {}
        },
        SortType::DESC => match sort_by {
            SortBy::Name => entries.sort_by(|a, b| {
                b.file_name()
                    .unwrap()
                    .to_ascii_lowercase()
                    .cmp(&a.file_name().unwrap().to_ascii_lowercase())
            }),
            SortBy::Size => entries.sort_by(|a, b| {
                b.metadata()
                    .ok()
                    .map(|meta| meta.len())
                    .unwrap_or(0)
                    .cmp(&a.metadata().ok().map(|meta| meta.len()).unwrap_or(0))
            }),
            SortBy::DateAdded => entries.sort_by(|a, b| {
                b.metadata()
                    .ok()
                    .and_then(|meta| meta.created().ok())
                    .unwrap_or(std::time::SystemTime::now())
                    .cmp(
                        &a.metadata()
                            .ok()
                            .and_then(|meta| meta.created().ok())
                            .unwrap_or(std::time::SystemTime::now()),
                    )
            }),
            _ => {}
        },
    }

    entries
}

// TODO: refator this method, too many string conversions
fn convert_file_path_to_string(
    entries: Vec<PathBuf>,
    show_hidden: bool,
    sort_by: SortBy,
    sort_type: SortType,
) -> Vec<String> {
    let mut file_strings: Vec<String> = Vec::new();

    let sort_entries = sort_entries_by_type(sort_by, sort_type, entries);
    let mut path_buf_list = Vec::new();

    for value in sort_entries {
        if value.is_dir() {
            path_buf_list.push(value);
        } else if value.is_file() {
            let file_name = value.file_name().unwrap();

            path_buf_list.push(value);
            /* if !file_name.to_str().unwrap().ends_with("png") {
                path_buf_list.push(value);
            } */
        }
    }
    if !show_hidden {
        for entry in path_buf_list {
            if entry.is_dir() {
                let file = entry.clone().into_os_string().to_str().unwrap().to_string();
                file_strings.push(file);
            } else if entry.is_file() {
                let file_name = entry.file_name().unwrap().to_str().unwrap();
                if !file_name.starts_with(".") {
                    let entry_value = entry.to_str().unwrap().to_string();
                    file_strings.push(entry_value);
                }
            }
        }
    } else {
        for entry in path_buf_list {
            let file = entry.clone().into_os_string().to_str().unwrap().to_string();
            file_strings.push(file);
        }
    }

    file_strings
}

fn handle_file_selection(
    file: &str,
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: App,
) -> anyhow::Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    terminal.clear()?;

    let ide = app.get_selected_ide();
    if ide.is_some() {
        let selected_ide = ide.unwrap();

        if Path::new(file).exists() {
            let output = Command::new(selected_ide.to_owned())
                .arg(file.to_owned())
                .status()
                .expect("Failed to open file");

            if output.success() {
                println!("Successfully opened file with {}", selected_ide);
            } else {
                println!("Failed to open file with {}", selected_ide);
            }
        }
    } else {
        let mut ctx = ClipboardContext::new().unwrap();
        ctx.set_contents(file.to_owned()).unwrap();
    }

    Ok(())
}

fn get_inner_files_info(
    file: String,
    show_hidden_files: bool,
    sort_by: SortBy,
    sort_type: &SortType,
) -> anyhow::Result<Option<Vec<String>>> {
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

    let file_strings =
        convert_file_path_to_string(entries, show_hidden_files, sort_by, sort_type.clone());
    Ok(Some(file_strings))
}

fn get_content_from_path(path: String) -> Option<Vec<String>> {
    let mut file_name_list: Vec<String> = Vec::new();
    match fs::read_dir(path) {
        Ok(val) => {
            for name in val.into_iter() {
                match name {
                    Ok(result) => {
                        let file_name = result.file_name().to_str().unwrap().to_string();
                        file_name_list.push(file_name);
                    }
                    Err(e) => {
                        println!("error getting content from path: {:?}", e);
                        return None;
                    }
                }
            }
        }
        Err(e) => {
            println!("her: {:?}", e);
            return None;
        }
    };
    Some(file_name_list)
}

fn draw_popup(rect: Rect, percent_x: u16, percent_y: u16) -> Rect {
    let popup_layout = Layout::vertical([
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
    ])
    .split(rect);

    Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ])
    .split(popup_layout[1])[1]
}

fn delete_file(file: &str) -> anyhow::Result<()> {
    match fs::remove_file(file) {
        Ok(_) => {}
        Err(e) => {
            // TODO: show notification to user
            println!("Error: {:?}", e);
        }
    }
    Ok(())
}

fn delete_dir(file: &str) -> anyhow::Result<()> {
    match fs::remove_dir_all(file) {
        Ok(_) => {}
        Err(e) => {
            println!("Error: {:?}", e);
        }
    }

    Ok(())
}

fn handle_delete_based_on_type(file: &str) -> anyhow::Result<()> {
    let metadata = fs::metadata(file)?;
    let file_type = metadata.file_type();

    if file_type.is_dir() {
        delete_dir(file)?;
    } else {
        delete_file(file)?;
    }
    Ok(())
}

fn get_file_path_data(
    start_path: String,
    show_hidden: bool,
    sort_by: SortBy,
    sort_type: &SortType,
) -> anyhow::Result<Vec<String>> {
    let entries = fs::read_dir(start_path)?
        .map(|res| res.map(|e| e.path()))
        .collect::<Result<Vec<_>, io::Error>>()?;

    let file_strings =
        convert_file_path_to_string(entries, show_hidden, sort_by, sort_type.clone());

    Ok(file_strings)
}

fn create_new_dir(current_file_path: String, new_item: String) -> anyhow::Result<()> {
    let append_path = format!("{}/{}", current_file_path, new_item);

    // TODO: implications of using (create_dir) || (create_dir_all)
    let response = match fs::create_dir(append_path) {
        Ok(_) => Ok(()),
        Err(e) => {
            return Err(e.into());
        }
    };
    response
}

fn create_new_file(current_file_path: String, file_name: String) -> anyhow::Result<()> {
    let append_path = format!("{}/{}", current_file_path, file_name);
    let response = match File::create_new(append_path) {
        Ok(_) => Ok(()),
        Err(e) => {
            return Err(e.into());
            // kind: AlreadyExists
        }
    };
    response
}

fn is_file(path: String) -> bool {
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

fn get_metadata_info(path: String) -> anyhow::Result<Option<Metadata>> {
    let metadata = match fs::metadata(path) {
        Ok(info) => Some(info),
        Err(_) => None,
    };

    Ok(metadata)
}

fn generate_metadata_str_info(metadata: anyhow::Result<Option<Metadata>>) -> String {
    let metadata_info = match metadata {
        Ok(res) => match res {
            Some(info) => {
                let size = info.len();
                let permissions = info.permissions();

                let format_str = format!("size: {} | permission: {}", size, permissions.readonly());
                format_str
            }
            None => String::from("Info not available"),
        },
        Err(_) => {
            println!("errr from here",);
            String::from("Encounter an error")
        }
    };

    metadata_info
}

fn generate_copy_file_dir_name(curr_path: String, new_path: String) -> String {
    let get_info = Path::new(&curr_path);

    let file_name = get_info.file_name().unwrap().to_str().unwrap();

    let create_new_file_name = format!("{}/copy_{}", new_path, file_name);
    create_new_file_name
}

fn create_item_based_on_type(current_file_path: String, new_item: String) -> anyhow::Result<()> {
    if new_item.contains(".") {
        let file_res = create_new_file(current_file_path, new_item);
        file_res
    } else {
        let dir_res = create_new_dir(current_file_path, new_item);
        dir_res
    }
}

fn handle_rename(app: App) -> io::Result<()> {
    let curr_path = format!("{}/{}", app.current_path_to_edit, app.current_name_to_edit);
    let new_path = format!("{}/{}", app.current_path_to_edit, app.create_edit_file_name);

    let result = match fs::rename(curr_path, new_path) {
        Ok(res) => res,
        Err(error) => return Err(error),
    };
    Ok(result)
}

fn check_if_exists(new_path: String) -> bool {
    match Path::new(&new_path).try_exists() {
        Ok(value) => match value {
            true => true,
            false => false,
        },
        Err(e) => {
            panic!("Error occured {:?}", e);
        }
    }
}

fn get_curr_path(path: String) -> String {
    let mut split_path = path.split("/").collect::<Vec<&str>>();
    split_path.pop();
    let vec_to_str = split_path.join("/");
    vec_to_str
}

fn copy_dir_file_helper(src: &Path, new_src: &Path) -> anyhow::Result<()> {
    if src.is_file() {
        fs::copy(src, new_src)?;
    } else {
        let entries: Vec<_> = WalkDir::new(src)
            .into_iter()
            .filter_map(Result::ok)
            .collect();
        entries.par_iter().try_for_each(|entry| {
            let entry_path = entry.path();
            let relative_path = entry_path.strip_prefix(src).unwrap();
            let dst_path = new_src.join(relative_path);

            if entry_path.is_dir() {
                fs::create_dir_all(&dst_path)?;
            } else if entry_path.is_file() {
                if let Some(parent) = dst_path.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::copy(entry_path, dst_path)?;
            } else {
                println!("Error, file type not supported");
                return Err(io::Error::new(ErrorKind::Other, "unsuported file type"));
            }

            Ok(())
        })?;
    }

    Ok(())
}

fn generate_sort_by_string(sort_type: &SortType) -> String {
    let str_sort_type = match sort_type {
        SortType::ASC => "ASC",
        SortType::DESC => "DESC",
    };
    let join_str = format!("Sort By: '{}'", str_sort_type);
    join_str
}

fn get_preview_path(files: Vec<String>) -> Option<String> {
    let curr_path = if files.len() == 0 {
        None
    } else {
        let fi = files[0].clone();
        Some(fi)
    };
    curr_path
}

fn validate_file_path(file_path: Option<String>) -> Option<bool> {
    let check_type = if let Some(curr_path) = file_path {
        if is_file(curr_path.to_string()) {
            Some(true)
        } else {
            Some(false)
        }
    } else {
        None
    };

    check_type
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let input_arguments: Vec<String> = env::args().collect();

    let mut config = configuration::Configuration::new();
    let mut sort_type = SortType::ASC;

    let mut file_reader_content = FileContent::new();
    let mut image_generator = ImageGenerator::new();

    config.handle_settings_configuration();
    // Setup terminal

    let file_strings = get_file_path_data(
        config.start_path.clone(),
        false,
        SortBy::Default,
        &sort_type,
    )?; //let file_strings = convert_file_path_to_string(entries);
    let mut app = App::new(file_strings.clone());

    // handle ide selection from arguments
    app.handle_arguments(input_arguments);

    let store = if Path::new(&config.cache_directory).exists() {
        let res = load_directory_from_file(&config.cache_directory.to_owned()).unwrap();
        println!("Loading directory cache from file");
        res
    } else {
        println!("Building directory cache, Please wait...");
        let new_store =
            build_directory_from_store(&config.start_path.to_owned(), config.ignore_directories);
        save_directory_to_file(&new_store, &config.cache_directory.to_owned())?;
        new_store
    };

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    terminal.clear()?;
    // Initial selected state
    let mut state = ListState::default();
    state.select(Some(0)); // Select the first item by default
                           //
    let mut read_only_state = ListState::default();
    read_only_state.select(Some(0));

    // Main loop
    loop {
        // Filtered items based on input
        let filtered_items: Vec<ListItem> = app
            .files
            .iter()
            .map(|file| ListItem::new(file.clone()))
            .collect();

        let filtered_read_only_items: Vec<ListItem> = app
            .copy_move_read_only_files
            .iter()
            .map(|file| ListItem::new(file.clone()))
            .collect();

        // Draw UI
        terminal.draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .margin(1)
                .constraints(
                    [
                        Constraint::Length(2),
                        Constraint::Length(3),
                        Constraint::Min(1),
                        Constraint::Length(3),
                        //Constraint::Length(1),
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
                    Style::default(),
                ),
                InputMode::Editing => (vec!["Normal Mode (Esc)".bold()], Style::default()),
                InputMode::WatchDelete => (vec!["Watch Delete Mode".bold()], Style::default()),
                InputMode::WatchCreate => (vec!["Watch Delete Mode".bold()], Style::default()),
                InputMode::WatchRename => (vec!["Watch Delete Mode".bold()], Style::default()),
                InputMode::WatchSort => (vec!["Watch Delete Mode".bold()], Style::default()),
                _ => (vec!["Default".bold()], Style::default()),
            };

            let inner_layout = Layout::default()
                .direction(Direction::Horizontal)
                .constraints(vec![
                    Constraint::Percentage(50),
                    Constraint::Percentage(50),
                ])
                .split(chunks[2]);

            // Input field
            let input_block = Paragraph::new(app.input.clone())
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title("Search")
                        .style(match app.input_mode {
                            InputMode::Normal => Style::default().fg(Color::White),
                            InputMode::Editing => Style::default().fg(Color::Green),
                            _ => Style::default().fg(Color::White),
                        }),
                )
                .style(match app.input_mode {
                    InputMode::Editing => Style::default().fg(Color::White),
                    InputMode::Normal => Style::default().fg(Color::White),
                    InputMode::WatchDelete => Style::default().fg(Color::Gray),
                    InputMode::WatchCreate => Style::default().fg(Color::Gray),
                    InputMode::WatchRename => Style::default().fg(Color::Gray),
                    InputMode::WatchSort => Style::default().fg(Color::Gray),
                    _ => Style::default().fg(Color::Gray),
                });

            let mut list_title = String::new();
            if app.loading {
                let title_with_loader = format!("Copying Files...");
                list_title.push_str(&title_with_loader);
            } else {
                list_title.push_str(&"List");
            }
            // List of filtered items
            // TODO: get first item from the list, 
            // 1. get first item from list
            // 2. render content based on type 
            //    - if type if dir then render its content 
            //    - if type is file then display content of file if posible
            // 3. preview mode will only apply when in normal MODE, 
            let list_block = List::new(filtered_items.clone())
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(list_title.as_str())
                        .style(match app.input_mode {
                            InputMode::Normal => Style::default().fg(Color::Green),
                            InputMode::Editing => Style::default().fg(Color::White),
                            _ => Style::default().fg(Color::White),
                        }), //.title("Filtered List"),
                )
                .highlight_style(
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                )
                .highlight_symbol(">")
                .style(match app.input_mode {
                    InputMode::Normal => Style::default().fg(Color::White),
                    InputMode::Editing => Style::default().fg(Color::White),
                    InputMode::WatchDelete => Style::default().fg(Color::Gray),
                    InputMode::WatchCreate => Style::default().fg(Color::Gray),
                    InputMode::WatchRename => Style::default().fg(Color::Gray),
                    InputMode::WatchSort => Style::default().fg(Color::Gray),
                    _ => Style::default().fg(Color::Gray),
                });

            //let preview_list_path = get_preview_path(app.files.clone());

            /* let validate_is_file = match validate_file_path(preview_list_path.clone()) {
                Some(v) => v,
                _=>  {
                    println!("not a valid file or empty");
                    false
                },
            }; */

            /* match validate_is_file {
                false => {
let new_preview_files = get_file_path_data(preview_list_path.unwrap(), false, SortBy::Default, &sort_type);
                    app.preview_files = new_preview_files.unwrap();

                },
                _ => app.preview_files = Vec::new()
            }; */


            //let file_list = get_file_path_data(valid_preview_list_path.unwrap(), false, SortBy::Default, &sort_type);

            /* let file_list_res = match file_list {
                Ok(list) => list,
                Err(err) =>  {
                   Vec::new() 
                },

            }; */
            // TODO: handle first item preview
            let list_preview_block = List::new(app.preview_files.clone()).block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title("Preview")
                        .style(match app.input_mode {
                            InputMode::Normal => Style::default().fg(Color::Green),
                            InputMode::Editing => Style::default().fg(Color::White),
                            _ => Style::default().fg(Color::White),
                        }), //.title("Filtered List"),
                )
                .highlight_style(
                    Style::default()
                        .fg(Color::White)
                        //.add_modifier(Modifier::BOLD),
                )
                //.highlight_symbol(">")
                .style(match app.input_mode {
                    InputMode::Normal => Style::default().fg(Color::White),
                    InputMode::Editing => Style::default().fg(Color::White),
                    InputMode::WatchDelete => Style::default().fg(Color::Gray),
                    InputMode::WatchCreate => Style::default().fg(Color::Gray),
                    InputMode::WatchRename => Style::default().fg(Color::Gray),
                    InputMode::WatchSort => Style::default().fg(Color::Gray),
                    _ => Style::default().fg(Color::Gray),
                });


            let footer_outer_layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints(vec![Constraint::Percentage(100)])
                .split(chunks[3]);

            let footer_inner_layout = Layout::default()
                .direction(Direction::Horizontal)
                .constraints(vec![Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(footer_outer_layout[0]);

            let bottom_instructions = Span::styled(
                "Open with selected IDE: <Enter> | Keybindings: ?",
                Style::default(),
            );
            //let default_empty_label = Span::styled("", Style::default());
let footer_stats =
                Text::from(Line::from(Span::styled(app.curr_stats.clone(), Style::default())));
                        let footer_stats_paragraph = Paragraph::new(footer_stats)
                .block(Block::default().borders(Borders::ALL))
                .style(Style::default());
                 f.render_widget(footer_stats_paragraph, footer_inner_layout[1]);

             match app.files.len() > 0 {
                true => {
        },
                false =>{}
                };

            let instructions = Text::from(Line::from(bottom_instructions));

            let parsed_instructions = Paragraph::new(instructions)
                .block(Block::default().borders(Borders::ALL))
                .style(Style::default());
            let text = Text::from(Line::from(msg)).patch_style(style);
            let help_message = Paragraph::new(text);

            let input_area = chunks[1];
            match app.input_mode {
                InputMode::Normal => {}
                InputMode::WatchDelete => {}
                InputMode::WatchCreate => {}
                InputMode::WatchRename => {}
                InputMode::WatchSort => {}
                InputMode::Editing => f.set_cursor(
                    input_area.x + app.character_index as u16 + 1,
                    input_area.y + 1,
                ),
                _ => {}
            }

            f.render_widget(help_message, chunks[0]);
            f.render_widget(input_block, chunks[1]);
            //f.render_widget(paragraph, chunks[2]);
            //f.render_widget(default_label, chunks[2]);
            //f.render_widget(parsed_instructions.clone(), footer_outer_layout[0]);
            //f.render_widget(parsed_instructions.clone(), footer_layout[1]);
            //f.render_widget(parsed_instructions.clone(), chunks[3]);
            //f.render_stateful_widget(list_block.clone(), inner_layout[0], &mut state);
            // f.render_widget(list_block, inner_layout[1]);
            f.render_stateful_widget(list_block.clone(), inner_layout[0], &mut state);


            match file_reader_content.file_type {
                FileType::FILE => {
        image_generator.image = None;
let file_preview_text = Paragraph::new(app.preview_file_content.clone())
                .block(Block::default().borders(Borders::ALL))
                .style(Style::default());
            f.render_widget(file_preview_text, inner_layout[1] );
                }
                FileType::IMG => {

                     //let _ =image_generator.load_img(file_reader_content.curr_asset_path.clone()).clone();
                    let image = StatefulImage::new(None);


                    //let img = image_generator.image.unwrap().clone();
            f.render_stateful_widget(image, inner_layout[1], &mut image_generator.image.clone().unwrap());

                }
                FileType::ZIP => {
let zip_list_content = List::new(file_reader_content.curr_zip_content.clone()).block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title("Preview")
                        .style(match app.input_mode {
                            InputMode::Normal => Style::default().fg(Color::Green),
                            InputMode::Editing => Style::default().fg(Color::Gray),
                            _ => Style::default().fg(Color::Gray),
                        }), //.title("Filtered List"),
                )
                //.highlight_symbol(">")
                .style(Style::default().fg(Color::DarkGray));
            f.render_widget(zip_list_content, inner_layout[1], );

                }
                _ => {

        image_generator.image = None;
            f.render_stateful_widget(list_preview_block, inner_layout[1], &mut state);
                }
            }
            //TODO: add match method here
            //f.render_stateful_widget(list_block, chunks[2], &mut state);
            f.render_widget(parsed_instructions.clone(), footer_inner_layout[0]);
            //f.render_widget(footer_stats_paragraph, footer_inner_layout[1]);

            if app.render_popup {
                let block = Block::bordered()
                    .title("Confirm to delete y/n")
                    .style(Style::default().fg(Color::Red));
                let area = draw_popup(f.size(), 40, 7);
                let popup_chuncks = Layout::default()
                    .direction(Direction::Horizontal)
                    .margin(1)
                    .constraints([Constraint::Percentage(100)])
                    .split(area);
                f.render_widget(Clear, area);
                f.render_widget(block, popup_chuncks[0]);
            }

            let area = draw_popup(f.size(), 40, 7);
            let popup_chuncks = Layout::default()
                .direction(Direction::Horizontal)
                .margin(1)
                .constraints([Constraint::Percentage(100)])
                .split(area);

            let sort_option_area = draw_popup(f.size(), 90, 20);
            let sort_options_chunks = Layout::default()
                .direction(Direction::Vertical)
                .margin(1)
                .constraints([Constraint::Percentage(100)])
                .split(sort_option_area);

            let keybinding_area = draw_popup(f.size(), 80, 20);
            let keybinding_chunks = Layout::default()
                .direction(Direction::Vertical)
                .margin(1)
                .constraints([Constraint::Percentage(100)])
                .split(keybinding_area);

            match app.input_mode {
                InputMode::WatchCreate => {
                    //f.render_widget(popup_block, area);

                    let create_input_block = Paragraph::new(app.create_edit_file_name.clone())
                        .block(Block::default().borders(Borders::ALL).title(
                            match app.is_create_edit_error {
                                false => "Create File/Dir".to_string(),
                                true => app.error_message.to_owned(),
                            },
                        ))
                        .style(match app.is_create_edit_error {
                            true => Style::default().fg(Color::Red),
                            false => Style::default().fg(Color::LightGreen),
                        });

                    f.render_widget(Clear, popup_chuncks[0]);
                    f.render_widget(create_input_block, popup_chuncks[0]);
                }
                InputMode::WatchRename => {
                    let create_input_block = Paragraph::new(app.create_edit_file_name.clone())
                        .block(Block::default().borders(Borders::ALL).title("Enter file/dir name"))
                        .style(Style::default().fg(Color::LightGreen));

                    f.render_widget(create_input_block, popup_chuncks[0]);
                }
                InputMode::WatchSort => {
                    let lines = vec![
                        Line::from("Press (a) to sort ASC or (d) to sort DESC, (q) to exit"),
                        Line::from("Name: (n)"),
                        Line::from("Date Created: (t)"),
                        Line::from("Size: (s)"),
                    ];

                    let sort_by_text = generate_sort_by_string(&sort_type);
                    let list_items = Text::from(lines);
                    let p = Paragraph::new(list_items)
                        .block(Block::default().borders(Borders::ALL).title(sort_by_text))
                        .style(Style::default().fg(Color::LightGreen));
                    f.render_widget(Clear, sort_options_chunks[0]);
                    f.render_widget(p, sort_options_chunks[0]);

                    //f.render_widget(create_input_block, sort_options_chunks[0]);
                }
                InputMode::WatchKeyBinding => {
                    let lines = vec![
                        Line::from("< Enter >: Open directory with selected IDE. copy path if not IDE option provided."),
                        Line::from("< s >: Sort"),
                        Line::from("< a >: Create new"),
                        Line::from("< d >: Delete"),
                        Line::from("< i >: Search mode"),
                        Line::from("< c >: Copy dir/file"),
                        Line::from("<.> : Show hidden files"),
                    ];

                    let sort_by_text = generate_sort_by_string(&sort_type);
                    let list_items = Text::from(lines);
                    let paragraph = Paragraph::new(list_items)
                        .block(Block::default().borders(Borders::ALL).title(sort_by_text))
                        .style(Style::default().fg(Color::LightGreen));
                    f.render_widget(Clear, keybinding_chunks[0]);
                    f.render_widget(paragraph, keybinding_chunks[0]);
                }
                InputMode::WatchCopy => {
                    let copy_area= draw_popup(f.size(), 80, 60);
                    let copy_popup_chuncks = Layout::default()
                    .direction(Direction::Horizontal)
                    .margin(1)
                    .constraints([Constraint::Percentage(100)])
                    .split(copy_area);
            // TODO: add dir list here: 
            let read_only_list = List::new(filtered_read_only_items)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title("Select Location")
                        .style(match app.input_mode {
                            InputMode::Normal => Style::default().fg(Color::Green),
                            InputMode::Editing => Style::default().fg(Color::White),
                            _ => Style::default().fg(Color::White).bg(Color::Black),
                        }),
                )
                .highlight_style(
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                )
                .highlight_symbol(">")
                .style(match app.input_mode {
                    InputMode::Normal => Style::default().fg(Color::White),
                    InputMode::Editing => Style::default().fg(Color::White),
                    InputMode::WatchDelete => Style::default().fg(Color::Gray),
                    InputMode::WatchCreate => Style::default().fg(Color::Gray),
                    InputMode::WatchRename => Style::default().fg(Color::Gray),
                    InputMode::WatchSort => Style::default().fg(Color::Gray),
                    _ => Style::default().fg(Color::Gray),
                });
                f.render_widget(Clear, copy_area);
                f.render_stateful_widget(read_only_list, copy_popup_chuncks[0], &mut read_only_state);
                }
                _ => {}
            }
        })?;

        // Handle input
        if let Event::Key(key) = event::read()? {
            match app.input_mode {
                InputMode::Normal => match key.code {
                    KeyCode::Char('i') => {
                        app.input_mode = InputMode::Editing;
                        file_reader_content.file_type = FileType::NotAvailable;
                        image_generator.image = None;
                    }
                    KeyCode::Char('q') => {
                        break;
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        if app.files.len() > 0 {
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
                            app.curr_index = Some(i);

                            let selected_cur_path = &app.files[i];
                            let get_metadata = get_metadata_info(selected_cur_path.to_owned());
                            let generated_metadata_str = generate_metadata_str_info(get_metadata);

                            app.curr_stats = generated_metadata_str.clone();

                            if !is_file(selected_cur_path.to_string()) {
                                if let Some(file_names) =
                                    get_content_from_path(selected_cur_path.to_string())
                                {
                                    image_generator.image = None;
                                    file_reader_content.file_type = FileType::NotAvailable;
                                    app.preview_files = file_names;
                                }
                            } else {
                                let file_extension = file_reader_content
                                    .get_file_extension(selected_cur_path.clone());

                                match file_extension {
                                    FileType::FILE => {
                                        image_generator.image = None;
                                        file_reader_content.file_type = FileType::FILE;
                                        let file_content = file_reader_content
                                            .read_file_content(selected_cur_path.to_string());

                                        // only update if there are no errors
                                        if !file_reader_content.is_error {
                                            app.preview_file_content = file_content;
                                        }
                                    }
                                    FileType::IMG => {
                                        image_generator.image = None;
                                        file_reader_content.curr_asset_path =
                                            selected_cur_path.to_string();

                                        image_generator.load_img(selected_cur_path.clone());
                                        file_reader_content.file_type = FileType::IMG;
                                    }
                                    FileType::ZIP => {
                                        image_generator.image = None;
                                        file_reader_content
                                            .read_zip_content(selected_cur_path.clone());
                                        file_reader_content.file_type = FileType::ZIP;
                                    }
                                    _ => {
                                        image_generator.image = None;
                                        file_reader_content.file_type = FileType::NotAvailable;
                                    }
                                }

                                app.preview_files = Vec::new();
                            }
                        }
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        if app.files.len() > 0 {
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
                            app.curr_index = Some(i);
                            let selected_cur_path = &app.files[i];
                            let get_metadata = get_metadata_info(selected_cur_path.to_owned());
                            let generated_metadata_str = generate_metadata_str_info(get_metadata);
                            app.curr_stats = generated_metadata_str.clone();

                            // INFO: update preview list

                            if !is_file(selected_cur_path.clone()) {
                                if let Some(file_names) =
                                    get_content_from_path(selected_cur_path.to_string())
                                {
                                    image_generator.image = None;
                                    file_reader_content.file_type = FileType::NotAvailable;
                                    app.preview_files = file_names;
                                }
                            } else {
                                let file_extension = file_reader_content
                                    .get_file_extension(selected_cur_path.clone());

                                match file_extension {
                                    FileType::FILE => {
                                        image_generator.image = None;
                                        file_reader_content.file_type = FileType::FILE;
                                        let file_content = file_reader_content
                                            .read_file_content(selected_cur_path.to_string());

                                        // only update if there are no errors
                                        if !file_reader_content.is_error {
                                            app.preview_file_content = file_content;
                                        }
                                    }
                                    FileType::IMG => {
                                        image_generator.image = None;
                                        file_reader_content.curr_asset_path =
                                            selected_cur_path.to_string();

                                        image_generator.load_img(selected_cur_path.clone());
                                        file_reader_content.file_type = FileType::IMG;
                                    }
                                    FileType::ZIP => {
                                        image_generator.image = None;
                                        file_reader_content
                                            .read_zip_content(selected_cur_path.clone());
                                        file_reader_content.file_type = FileType::ZIP;
                                    }
                                    _ => {
                                        image_generator.image = None;
                                        file_reader_content.file_type = FileType::NotAvailable;
                                    }
                                }
                                app.preview_files = Vec::new()
                            }
                        }
                    }
                    KeyCode::Char('h') => {
                        if app.files.len() > 0 {
                            let selected = &app.files[state.selected().unwrap()];
                            let mut split_path = selected.split("/").collect::<Vec<&str>>();

                            let sort_type_copy = sort_type.clone();
                            // TODO: refactor this to be more idiomatic
                            if split_path.len() > 4 {
                                split_path.pop();
                                split_path.pop();

                                let new_path = split_path.join("/");
                                let files_strings = get_inner_files_info(
                                    new_path.clone(),
                                    app.show_hidden_files,
                                    SortBy::Default,
                                    &sort_type_copy,
                                )
                                .unwrap();

                                if let Some(f_s) = files_strings {
                                    app.read_only_files = f_s.clone();
                                    app.files = f_s;
                                    state.select(Some(0));
                                }
                            }
                        } else {
                            let copy = sort_type.clone();
                            let files_strings = get_inner_files_info(
                                app.prev_dir.clone(),
                                app.show_hidden_files,
                                SortBy::Default,
                                &copy,
                            )
                            .unwrap();

                            if let Some(f_s) = files_strings {
                                app.read_only_files = f_s.clone();
                                app.files = f_s;
                                state.select(Some(0));
                            }
                        }
                    }
                    KeyCode::Char('l') => {
                        let selected_index = state.selected();
                        if app.files.len() > 0 {
                            if let Some(selected_indx) = selected_index {
                                let selected = &app.files[selected_indx];

                                app.prev_dir = get_curr_path(selected.to_string());
                                if !is_file(selected.to_string()) {
                                    match get_inner_files_info(
                                        selected.to_string(),
                                        app.show_hidden_files,
                                        SortBy::Default,
                                        &sort_type,
                                    ) {
                                        Ok(files_strings) => {
                                            if let Some(files_strs) = files_strings {
                                                app.read_only_files = files_strs.clone();
                                                app.files = files_strs;
                                                state.select(Some(0));
                                            }
                                        }
                                        Err(e) => {
                                            println!("Error: {}", e);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    KeyCode::Char('d') => {
                        app.render_popup = true;
                        app.input_mode = InputMode::WatchDelete;
                    }
                    KeyCode::Char('a') => {
                        app.input_mode = InputMode::WatchCreate;
                    }
                    KeyCode::Char('r') => {
                        let selected_index = state.selected();
                        if let Some(index) = selected_index {
                            let selected = &app.files[index];
                            let mut split_path = selected.split("/").collect::<Vec<&str>>();
                            let placeholder_name = split_path.pop().unwrap();
                            let new_path = split_path.join("/");
                            let placeholder_name_copy = placeholder_name;
                            app.current_path_to_edit = new_path;
                            app.current_name_to_edit = placeholder_name_copy.to_string();
                            app.create_edit_file_name = placeholder_name.to_string();
                            app.char_index = placeholder_name.len();
                        }
                        app.input_mode = InputMode::WatchRename;
                    }
                    KeyCode::Char('.') => {
                        let is_hidden = !app.show_hidden_files;
                        app.show_hidden_files = is_hidden;
                        let selected_index = state.selected();
                        if let Some(indx) = selected_index {
                            let selected = &app.files[indx];

                            let mut split_path = selected.split("/").collect::<Vec<&str>>();
                            split_path.pop();
                            let new_path = split_path.join("/");
                            match get_inner_files_info(
                                new_path,
                                is_hidden,
                                SortBy::Default,
                                &sort_type,
                            ) {
                                Ok(files) => {
                                    if let Some(file_strs) = files {
                                        app.read_only_files = file_strs.clone();
                                        app.files = file_strs;
                                    }
                                }
                                Err(e) => {
                                    println!("error  {}", e);
                                }
                            }
                        }
                    }
                    KeyCode::Char('c') => {
                        // item path to copy
                        app.input_mode = InputMode::WatchCopy;
                        let index = state.selected().unwrap();
                        let selected_path = &app.files[index].to_owned();
                        app.item_to_copy_path = selected_path.clone();
                    }

                    KeyCode::Char('s') => {
                        app.input_mode = InputMode::WatchSort;
                    }

                    KeyCode::Char('?') => {
                        app.input_mode = InputMode::WatchKeyBinding;
                    }

                    KeyCode::Enter => {
                        let app_files = app.files.clone();
                        let selected = &app_files[state.selected().unwrap()];

                        app.input = selected.clone();

                        let _ = handle_file_selection(&selected, &mut terminal, app.clone());
                        break;
                    }
                    _ => {}
                },

                InputMode::WatchRename if key.kind == KeyEventKind::Press => match key.code {
                    KeyCode::Char(c) => {
                        app.add_char(c);
                    }
                    KeyCode::Backspace => {
                        app.delete_c();
                    }
                    KeyCode::Esc => {
                        app.input_mode = InputMode::Normal;
                        app.reset_create_edit_values();
                    }
                    KeyCode::Enter => {
                        // rename file to new name
                        // validate tha the new name and the previous name are not the same,
                        // if names are equal then exit the current mode
                        if app.create_edit_file_name == app.current_name_to_edit {
                            app.input_mode = InputMode::Normal;
                            app.reset_create_edit_values();
                        } else {
                            // proceed with operation
                            let new_path = format!(
                                "{}/{}",
                                app.current_path_to_edit, app.create_edit_file_name
                            );
                            if !check_if_exists(new_path) {
                                match handle_rename(app.clone()) {
                                    Ok(_) => {
                                        app.reset_create_edit_values();
                                        let file_path_list = get_file_path_data(
                                            config.start_path.to_owned(),
                                            app.show_hidden_files,
                                            SortBy::Default,
                                            &sort_type,
                                        )?;
                                        app.files = file_path_list.clone();
                                        app.read_only_files = file_path_list.clone();
                                        app.input_mode = InputMode::Normal;
                                    }
                                    Err(e) => {
                                        app.is_create_edit_error = true;
                                        match e.kind() {
                                            ErrorKind::InvalidInput => {
                                                app.error_message = "Invalid input".to_string();
                                            }
                                            _ => {
                                                app.error_message = "Other error".to_string();
                                            }
                                        }
                                    }
                                }
                            } else {
                                app.is_create_edit_error = true;
                                app.error_message = "Already exist".to_string();
                            }
                        }
                    }
                    _ => {}
                },
                InputMode::WatchCreate if key.kind == KeyEventKind::Press => match key.code {
                    KeyCode::Char(c) => {
                        app.add_char(c);
                    }
                    KeyCode::Backspace => {
                        app.delete_c();
                    }
                    KeyCode::Left => {
                        app.move_create_edit_cursor_left();
                    }
                    KeyCode::Right => {
                        app.move_create_edit_cursor_right();
                    }
                    KeyCode::Esc => {
                        app.input_mode = InputMode::Normal;
                        app.reset_create_edit_values();
                        // create methods to reset the create_edit_file_name and state of it
                    }
                    KeyCode::Enter => {
                        // create file/dir
                        if !app.create_edit_file_name.is_empty() {
                            let selected_index = state.selected();
                            let selected = &app.files[selected_index.unwrap()];
                            let mut split_path = selected.split("/").collect::<Vec<&str>>();
                            split_path.pop();
                            let new_path = split_path.join("/");
                            match create_item_based_on_type(
                                new_path,
                                app.create_edit_file_name.clone(),
                            ) {
                                Ok(_) => {
                                    app.input_mode = InputMode::Normal;

                                    app.reset_create_edit_values();
                                    let file_path_list = get_file_path_data(
                                        config.start_path.to_owned(),
                                        app.show_hidden_files,
                                        SortBy::Default,
                                        &sort_type,
                                    )?;
                                    app.files = file_path_list.clone();
                                    app.read_only_files = file_path_list.clone();
                                }
                                Err(e) => {
                                    let error = e.downcast_ref::<io::Error>().unwrap();
                                    match error.kind() {
                                        ErrorKind::AlreadyExists => {
                                            app.error_message = "File Already Exists".to_string();
                                            app.is_create_edit_error = true;
                                        }
                                        _ => {}
                                    }
                                } // show error to user
                            } // test
                        }
                    }
                    _ => {}
                },

                InputMode::Editing if key.kind == KeyEventKind::Press => match key.code {
                    KeyCode::Enter => app.submit_message(),
                    KeyCode::Char(to_insert) => {
                        app.enter_char(to_insert, store.clone());
                    }
                    KeyCode::Backspace => {
                        app.delete_char(store.clone());
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
                InputMode::WatchDelete => match key.code {
                    KeyCode::Char('q') => {
                        app.render_popup = false;
                        app.input_mode = InputMode::Normal;
                        break;
                    }
                    KeyCode::Char('n') => {
                        app.render_popup = false;
                        app.input_mode = InputMode::Normal;
                    }

                    KeyCode::Char('y') => {
                        let selected_index = state.selected();

                        if let Some(selected_indx) = selected_index {
                            let selected = &app.files[selected_indx];

                            handle_delete_based_on_type(selected).unwrap();

                            let file_path_list = get_file_path_data(
                                config.start_path.to_owned(),
                                app.show_hidden_files,
                                SortBy::Default,
                                &sort_type,
                            )?;
                            app.render_popup = false;
                            app.files = file_path_list.clone();
                            app.read_only_files = file_path_list.clone();
                            app.input_mode = InputMode::Normal;
                        }
                    }
                    _ => {}
                },
                InputMode::WatchSort => match key.code {
                    KeyCode::Char('q') => {
                        app.input_mode = InputMode::Normal;
                    }
                    KeyCode::Char('n') => {
                        // sort by name

                        // we only care about the path not the selcted item
                        let get_path_from_list = &app.files[0];
                        let cur_path = get_curr_path(get_path_from_list.to_string());
                        let file_path_list = get_file_path_data(
                            cur_path,
                            app.show_hidden_files,
                            SortBy::Name,
                            &sort_type,
                        )?;
                        app.files = file_path_list.clone();
                        app.read_only_files = file_path_list.clone();
                        app.input_mode = InputMode::Normal;
                    }

                    KeyCode::Char('s') => {
                        // TODO: this code should be refactor to into a reusable method since is
                        // used in multiple places
                        let get_path_from_list = &app.files[0];
                        let cur_path = get_curr_path(get_path_from_list.to_string());

                        let file_path_list = get_file_path_data(
                            cur_path,
                            app.show_hidden_files,
                            SortBy::Size,
                            &sort_type,
                        )?;
                        app.files = file_path_list.clone();
                        app.read_only_files = file_path_list.clone();
                        app.input_mode = InputMode::Normal;
                    }

                    KeyCode::Char('t') => {
                        let get_path_from_list = &app.files[0];
                        let cur_path = get_curr_path(get_path_from_list.to_string());

                        let file_path_list = get_file_path_data(
                            cur_path,
                            app.show_hidden_files,
                            SortBy::DateAdded,
                            &sort_type,
                        )?;
                        app.files = file_path_list.clone();
                        app.read_only_files = file_path_list.clone();
                        app.input_mode = InputMode::Normal;
                    }
                    KeyCode::Char('a') => {
                        sort_type = SortType::ASC;
                    }
                    KeyCode::Char('d') => {
                        sort_type = SortType::DESC;
                    }
                    _ => {}
                },

                InputMode::WatchKeyBinding => match key.code {
                    KeyCode::Char('q') => {
                        app.input_mode = InputMode::Normal;
                    }
                    _ => {}
                },
                InputMode::WatchCopy => match key.code {
                    KeyCode::Char('q') => {
                        read_only_state.select(Some(0));
                        let copy_curr_files = app.files.clone();
                        app.copy_move_read_only_files = copy_curr_files;
                        app.copy_move_read_only_files = app.files.clone();
                        app.input_mode = InputMode::Normal;
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        if app.files.len() > 0 {
                            let i = match read_only_state.selected() {
                                Some(i) => {
                                    if i >= app.copy_move_read_only_files.len() - 1 {
                                        0
                                    } else {
                                        i + 1
                                    }
                                }
                                None => 0,
                            };
                            read_only_state.select(Some(i));
                        }
                    }
                    // BUG: for some reason this is not rendering stats corectly
                    KeyCode::Up | KeyCode::Char('k') => {
                        if app.files.len() > 0 {
                            let i = match read_only_state.selected() {
                                Some(i) => {
                                    if i == 0 {
                                        app.copy_move_read_only_files.len() - 1
                                    } else {
                                        i - 1
                                    }
                                }
                                None => 0,
                            };
                            read_only_state.select(Some(i));
                        }
                    }
                    KeyCode::Char('h') => {
                        if app.files.len() > 0 {
                            let selected =
                                &app.copy_move_read_only_files[read_only_state.selected().unwrap()];
                            let mut split_path = selected.split("/").collect::<Vec<&str>>();

                            let sort_type_copy = sort_type.clone();
                            if split_path.len() > 4 {
                                split_path.pop();
                                split_path.pop();

                                let new_path = split_path.join("/");
                                app.input = new_path.clone();
                                let files_strings = get_inner_files_info(
                                    new_path.clone(),
                                    app.show_hidden_files,
                                    SortBy::Default,
                                    &sort_type_copy,
                                )
                                .unwrap();

                                if let Some(f_s) = files_strings {
                                    app.copy_move_read_only_files = f_s.clone();
                                    read_only_state.select(Some(0));
                                }
                            }
                        } else {
                            let copy = sort_type.clone();
                            let files_strings = get_inner_files_info(
                                app.copy_move_read_only_files_prev.clone(),
                                app.show_hidden_files,
                                SortBy::Default,
                                &copy,
                            )
                            .unwrap();

                            if let Some(f_s) = files_strings {
                                app.copy_move_read_only_files = f_s.clone();
                                read_only_state.select(Some(0));
                            }
                        }
                    }

                    KeyCode::Char('l') => {
                        let selected_index = read_only_state.selected();
                        if app.copy_move_read_only_files.len() > 0 {
                            if let Some(selected_indx) = selected_index {
                                let selected = &app.copy_move_read_only_files[selected_indx];

                                app.copy_move_read_only_files_prev =
                                    get_curr_path(selected.to_string());
                                if !is_file(selected.to_string()) {
                                    match get_inner_files_info(
                                        selected.to_string(),
                                        app.show_hidden_files,
                                        SortBy::Default,
                                        &sort_type,
                                    ) {
                                        Ok(files_strings) => {
                                            if let Some(files_strs) = files_strings {
                                                app.copy_move_read_only_files = files_strs.clone();
                                                read_only_state.select(Some(0));
                                            }
                                        }
                                        Err(e) => {
                                            println!("Error: {}", e);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    KeyCode::Enter => {
                        //if app.copy_move_read_only_files.len() > 0 {
                        let index = read_only_state.selected();
                        app.loading = true;
                        if let Some(indx) = index {
                            // item to copy
                            let selected_path = &app.copy_move_read_only_files[indx];

                            // get current path to add new item
                            let mut split_path = selected_path.split("/").collect::<Vec<&str>>();
                            split_path.pop();
                            let string_path = split_path.join("/");
                            // append copy to new dir/file
                            let item_to_copy_cur_path = Path::new(&app.item_to_copy_path);

                            let new_path_with_new_name = generate_copy_file_dir_name(
                                app.item_to_copy_path.clone(),
                                string_path.clone(),
                            );

                            // item to copy path => app.item_to_copy_path.clone();
                            let new_src = Path::new(&new_path_with_new_name);
                            copy_dir_file_helper(item_to_copy_cur_path, new_src)?;
                            // show spinner that is downloading?
                            app.loading = false;
                            let copy_curr_files = app.files.clone();
                            app.copy_move_read_only_files = copy_curr_files;
                            read_only_state.select(Some(0));

                            app.copy_move_read_only_files = app.files.clone();
                            app.input_mode = InputMode::Normal;
                        }
                        //}
                    }
                    _ => {}
                },
                _ => {}
            }
        }
    }

    // Restore terminal
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
