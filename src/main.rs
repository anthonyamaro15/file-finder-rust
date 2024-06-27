use app::{App, InputMode};
use crossterm::event::{DisableMouseCapture, EnableMouseCapture};

use std::{
    env,  fs::{self, File}, io::{self, ErrorKind, Stdout}, path::{Path, PathBuf}, process::Command
};

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

use crate::directory_store::{
    build_directory_from_store, load_directory_from_file, save_directory_to_file,
};

extern crate copypasta;
use copypasta::{ClipboardContext, ClipboardProvider};

mod app;
mod configuration;
mod directory_store;

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

fn draw_popup(rect: Rect, percent_x: u16, percent_y: u16) -> Rect {

    let popup_layout = Layout::vertical([
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
    ]).split(rect);

    
    Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x)/2)
    ]).split(popup_layout[1])[1]
}

fn delete_file(file: &str) -> anyhow::Result<()> {
   match fs::remove_file(file) {
        Ok(_) => {},
        Err(e) => {
            // TODO: show notification to user
            println!("Error: {:?}", e);
        }
    }
    Ok(())
}

fn delete_dir(file: &str) -> anyhow::Result<()> {
     match fs::remove_dir_all(file) {
        Ok(_) => {},
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

fn get_file_path_data(start_path: String) -> anyhow::Result<Vec<String>> {
let entries = fs::read_dir(start_path)?
        .map(|res| res.map(|e| e.path()))
        .collect::<Result<Vec<_>, io::Error>>()?;

    let file_strings = convert_file_path_to_string(entries);

    Ok(file_strings)

}

fn create_new_dir(current_file_path: String, new_item: String) -> anyhow::Result<()>{
    let append_path  = format!("{}/{}", current_file_path, new_item);

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

fn create_item_based_on_type(current_file_path: String, new_item: String) -> anyhow::Result<()>{

    if new_item.contains(".") {
     let file_res = create_new_file(current_file_path, new_item);
        file_res
    } else {
       let dir_res = create_new_dir(current_file_path, new_item);
        dir_res
    }
}


fn handle_rename(app: App) ->io::Result<()>  {

    let curr_path = format!("{}/{}", app.current_path_to_edit, app.current_name_to_edit);
    let new_path  = format!("{}/{}", app.current_path_to_edit, app.create_edit_file_name);

    let result =match fs::rename(curr_path, new_path) {
        Ok(res) =>res,
        Err(error) => return Err(error),
    }; 
    Ok(result)
}

fn check_if_exists(new_path: String) -> bool {
   match  Path::new(&new_path).try_exists() {
        Ok(value) => {
            match value {
                true => true,
                false => false 
            }
        },
        Err(e) => {
            panic!("Error occured {:?}",e);
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let input_arguments: Vec<String> = env::args().collect();

    let mut config = configuration::Configuration::new();

    config.handle_settings_configuration();
    // Setup terminal


    let file_strings = get_file_path_data(config.start_path.clone())?;    //let file_strings = convert_file_path_to_string(entries);
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
                        Constraint::Length(3),
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
            };


            let inner_layout= Layout::default()
                .direction(Direction::Horizontal)
                .constraints(vec![
                    //Constraint::Percentage(50),
                    Constraint::Percentage(100),
                ])
                    .split(chunks[2]);

            // Input field
                       let input_block = Paragraph::new(app.input.clone())
                .block(Block::default().borders(Borders::ALL).title("Find"))
                .style(match app.input_mode {
                    InputMode::Editing => Style::default().fg(Color::Green),
                    InputMode::Normal => Style::default().fg(Color::White),
                    InputMode::WatchDelete => Style::default().fg(Color::Gray),
                    InputMode::WatchCreate => Style::default().fg(Color::Gray),
                    InputMode::WatchRename => Style::default().fg(Color::Gray),
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
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                )
                .highlight_symbol(">>")
                .style(match app.input_mode {
                    InputMode::Normal => Style::default().fg(Color::Green),
                    InputMode::Editing => Style::default().fg(Color::White),
                    InputMode::WatchDelete => Style::default().fg(Color::Gray),
                    InputMode::WatchCreate => Style::default().fg(Color::Gray),
                    InputMode::WatchRename => Style::default().fg(Color::Gray)
                });

            let bottom_instructions = Span::styled(
                "Use (j,k) to navigate, use(h,l) to navigate directory, 'Enter' to open with selected IDE",
                Style::default(),
            );
            let default_empty_label = Span::styled("",Style::default());

            let instructions = Text::from(Line::from(bottom_instructions));
            let parsed_instructions = Paragraph::new(instructions)
                .block(Block::default().borders(Borders::ALL))
                .style(Style::default());
            let default_label = Paragraph::new(default_empty_label)
                .block(Block::default().borders(Borders::ALL))
                .style(Style::default());
            let text = Text::from(Line::from(msg)).patch_style(style);
            let help_message = Paragraph::new(text);

            let input_area = chunks[1];
            match app.input_mode {
                InputMode::Normal => {},
                InputMode::WatchDelete => {},
                InputMode::WatchCreate => {},
                InputMode::WatchRename => {},
                InputMode::Editing => {
                    f.set_cursor(
                      input_area.x + app.character_index as u16 + 1,
                       input_area.y  + 1, 
                    )
                }
            }

            

            f.render_widget(help_message, chunks[0]);
            f.render_widget(parsed_instructions.clone(), chunks[3]);
            f.render_widget(input_block, chunks[1]);
            f.render_widget(default_label, chunks[2]);
            f.render_stateful_widget(list_block.clone(), inner_layout[0], &mut state);
           // f.render_widget(list_block, inner_layout[1]);
            //f.render_stateful_widget(list_block, chunks[2], &mut state);
            //
            if app.render_popup {
                let block = Block::bordered().title("Confirm to delete y/n").style(Style::default().fg(Color::Red));
                let area = draw_popup(f.size(), 40, 7);
                let popup_chuncks = Layout::default()
                    .direction(Direction::Horizontal)
                    .margin(1)
                    .constraints([Constraint::Percentage(100)]).split(area);
                    f.render_widget(Clear, area);
                f.render_widget(block, popup_chuncks[0]);

        
            }


                let area = draw_popup(f.size(), 40, 7);
                let popup_chuncks = Layout::default()
                    .direction(Direction::Horizontal)
                    .margin(1)
                    .constraints([Constraint::Percentage(100)]).split(area);

            match app.input_mode {
                InputMode::WatchCreate => {
                //f.render_widget(popup_block, area);

                let create_input_block = Paragraph::new(app.create_edit_file_name.clone())
                    .block(Block::default().borders(Borders::ALL).title(
                            match app.is_create_edit_error {
                                false => "Create File/Dir".to_string(),
                                true => app.error_message.to_owned()
                            }
                        ))
                    .style(
                            match app.is_create_edit_error {
                                true => Style::default().fg(Color::Red),
                                false => Style::default().fg(Color::LightGreen)
                            }
                        );

                f.render_widget(create_input_block, popup_chuncks[0]);


                },
                InputMode::WatchRename => {
                    let create_input_block = Paragraph::new(app.create_edit_file_name.clone())
                        .block(Block::default().borders(Borders::ALL).title(
                            match app.is_create_edit_error {
                                false => "Rename to".to_string(),
                                true => app.error_message.to_owned()
                            }
                        ))
                            .style(
                            match app.is_create_edit_error {
                                true => Style::default().fg(Color::Red),
                                false => Style::default().fg(Color::LightGreen)
                            }
                        );

                f.render_widget(create_input_block, popup_chuncks[0]);

                }
                _ => {}
            }
        })?;

        // Handle input
        if let Event::Key(key) = event::read()? { match app.input_mode { 
            InputMode::Normal => match key.code { KeyCode::Char('i') => {
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
                        let selected = &app.files[state.selected().unwrap()];
                        let mut split_path = selected.split("/").collect::<Vec<&str>>();

                        // TODO: refactor this to be more idiomatic
                        if split_path.len() > 4 {
                            split_path.pop();
                            split_path.pop();

                            let new_path = split_path.join("/");
                            let files_strings = get_inner_files_info(new_path.clone()).unwrap();

                            if let Some(f_s) = files_strings {
                                app.read_only_files = f_s.clone();
                                app.files = f_s;
                                state.select(Some(0));
                            }
                        }

                    }
                    KeyCode::Char('l') => {
                        let selected_index = state.selected();
                        if let Some(selected_indx) = selected_index {
                            let selected = &app.files[selected_indx];

                            match get_inner_files_info(selected.to_string()) {
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
                    KeyCode::Char('d') => {
                        app.render_popup = true;
                        app.input_mode = InputMode::WatchDelete;
                    }
                    KeyCode::Char('a') => {
                    app.input_mode = InputMode::WatchCreate;
                    }
                    KeyCode::Char('r') => {
                        let selected_index = state.selected();
                    // /Users/anthonyamaro/Desktop/testjs
                    // handle rename file/dir functionality
                    //let t = fs::rename(, to)
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

                },
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
                        let new_path = format!("{}/{}", app.current_path_to_edit, app.create_edit_file_name);
                        if !check_if_exists(new_path) {
                            match handle_rename(app.clone()) {
                            Ok(_) => {
                                app.reset_create_edit_values();
                                let file_path_list = get_file_path_data(config.start_path.to_owned())?;
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
                            app.is_create_edit_error  = true;
                            app.error_message = "Already exist".to_string();
                        }
                        
                    }
                }
                    _ => {}
                }
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
                        match  create_item_based_on_type(new_path, app.create_edit_file_name.clone()) {

                            Ok(_) => {
                                app.input_mode = InputMode::Normal;

                                app.reset_create_edit_values();
                                let file_path_list = get_file_path_data(config.start_path.to_owned())?;
                                app.files = file_path_list.clone();
                                app.read_only_files = file_path_list.clone();

                            },
                            Err(e) => {
                                let error = e.downcast_ref::<io::Error>().unwrap();
                                match error.kind() {
                                    ErrorKind::AlreadyExists => {
                                        app.error_message = "File Already Exists".to_string();
                                        app.is_create_edit_error = true;
                                    },
                                    _ => {}
                                }
                            }

                                // show error to user
                        }// test
                        
                    }
                }
                _ => {}
            }
            
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
                        let selected  = &app.files[selected_indx];

                        handle_delete_based_on_type(selected).unwrap();

                        let file_path_list = get_file_path_data(config.start_path.to_owned())?;
                        app.render_popup = false;
                        app.files = file_path_list.clone();
                        app.read_only_files = file_path_list.clone();
                        app.input_mode = InputMode::Normal;
                    }


                }
                _ => {}
            }
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
