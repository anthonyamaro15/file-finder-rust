use app::{App, InputMode};
use configuration::Configuration;
use crossterm::event::{DisableMouseCapture, EnableMouseCapture};

use std::{
    env, fs,
    io::{self, Stdout},
    path::{Path, PathBuf},
    process::Command,
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
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let input_arguments: Vec<String> = env::args().collect();

    let mut config = configuration::Configuration::new();

    config.handle_settings_configuration();
    // Setup terminal

    /* let entries = fs::read_dir(config.start_path.to_owned())?
        .map(|res| res.map(|e| e.path()))
        .collect::<Result<Vec<_>, io::Error>>()?; */

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
                let area = draw_popup(f.size(), 60, 5);
                    f.render_widget(Clear, area);
                f.render_widget(block, area);

        
            }
        })?;

        // Handle input
        if let Event::Key(key) = event::read()? { match app.input_mode { InputMode::Normal => match key.code { KeyCode::Char('i') => {
                        app.input_mode = InputMode::Editing;
                    }
                    KeyCode::Char('q') => {
                        break;
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        if !app.render_popup {

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
                                            }
                    KeyCode::Up | KeyCode::Char('k') => {
                if !app.render_popup {

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
                                            }
                    KeyCode::Char('h') => {
                if !app.render_popup { 
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
                            }
                        }

                }
                                            }
                    KeyCode::Char('l') => {
                if !app.render_popup {
                        let selected_index = state.selected();
                        if let Some(selected_indx) = selected_index {
                            let selected = &app.files[selected_indx];

                            match get_inner_files_info(selected.to_string()) {
                                Ok(files_strings) => {
                                    if let Some(files_strs) = files_strings {
                                        app.read_only_files = files_strs.clone();
                                        app.files = files_strs;
                                    }
                                }
                                Err(e) => {
                                    println!("Error: {}", e);
                                }
                            }
                        }


                }
                                            }
                    KeyCode::Char('d') => {
                        app.render_popup = !app.render_popup;
                    }
                    KeyCode::Char('n') => {
                // if delete popup is open then track what user wants to do, if not then close it
                        if app.render_popup {
                            app.render_popup = false;
                }
            }
            KeyCode::Char('y') => {
                if app.render_popup {
                    let selected_index = state.selected();

                    if let Some(selected_indx) = selected_index {
                        let selected  = &app.files[selected_indx];

                        handle_delete_based_on_type(selected).unwrap();

                        let file_path_list = get_file_path_data(config.start_path.to_owned())?;
                        app.render_popup = false;
                        app.files = file_path_list.clone();
                        app.read_only_files = file_path_list.clone();
                    }
                }
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
                InputMode::Editing => {}
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
