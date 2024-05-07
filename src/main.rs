// get all files from Desktop dir
// have a loop that runs until user selects a file from list or exits
// as user types, filter the file list to show only files that match the user input
// if user selects a file, print the the file path (for now) and exit the program
//
//
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use std::io::{self, stdout, Stdout, Write};

use ratatui::{prelude::*, widgets::*};
use std::fs;

pub type Tui = Terminal<CrosstermBackend<Stdout>>;

fn init() -> io::Result<Tui> {
    execute!(stdout(), EnterAlternateScreen)?;
    enable_raw_mode()?;
    Ok(Terminal::new(CrosstermBackend::new(stdout()))?)
}

fn restore() -> anyhow::Result<()> {
    execute!(stdout(), LeaveAlternateScreen)?;
    disable_raw_mode()?;
    Ok(())
}

struct View {
    files: Vec<String>,
}

struct App {
    file: String,
    files: Vec<String>,
    exit: bool,
}

impl App {
    pub fn new(files: Vec<String>) -> App {
        App {
            file: String::from(""),
            files,
            exit: false,
        }
    }
    pub fn run(&mut self, terminal: &mut Tui) -> anyhow::Result<()> {
        println!("what do we have {:?}", self.files);
        while !self.exit {
            terminal.draw(|frame| self.render_frame(frame))?;
            self.handle_events()?;
        }
        Ok(())
    }

    fn render_frame(&mut self, frame: &mut Frame) {
        frame.render_widget(
            Paragraph::new("Hello workdd").block(Block::bordered().title("testing")),
            frame.size(),
        );
    }

    fn handle_events(&mut self) -> anyhow::Result<()> {
        match event::read()? {
            Event::Key(key_event) if key_event.kind == KeyEventKind::Press => {
                self.handle_key_event(key_event)
            }
            _ => {}
        }
        Ok(())
    }
    fn handle_key_event(&mut self, key_event: KeyEvent) {
        match key_event.code {
            KeyCode::Char('q') => self.handle_exit(),
            _ => {}
        }
    }

    fn handle_exit(&mut self) {
        self.exit = true;
    }
}

impl View {
    fn new() -> View {
        View { files: Vec::new() }
    }

    fn ui(&mut self, frame: &mut Frame) {
        frame.render_widget(
            Paragraph::new("Hello workdd").block(Block::bordered().title("testing")),
            frame.size(),
        );
    }
}

// TODO :
// Bonus (if user selects a file, paste the file path in terminal or copy to clipboard || open nvim
// with the file path)
// Bonus (Implement ratatui to create a basic UI for the file list)
// Bonus: research how to create a binary/executable from this code and have it run when user types a command in terminal
fn main() -> anyhow::Result<()> {
    println!("Hello, world!");
    let entries = fs::read_dir("../../")?
        .map(|res| res.map(|e| e.path()))
        .collect::<Result<Vec<_>, io::Error>>()?;

    let mut file_strings: Vec<String> = Vec::new();

    for value in entries.iter() {
        let val = value.clone().into_os_string().to_str().unwrap().to_string();

        file_strings.push(val.clone());
        if val.contains("web") {
            println!("what is this {}", val);
        }
    }
    let mut terminal = init()?;
    let mut app = App::new(file_strings);
    //    let view = View::new();

    let app_result = app.run(&mut terminal);
    restore()?;
    let _ = app_result;
    //let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;

    // let mut should_quit = false;

    /* while !should_quit {
        terminal.draw(view.ui)?;
        should_quit = handle_events()?;
    } */
    if entries.is_empty() {
        // display message error that something went wrong
    }

    disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;

    /* for value in entries.iter() {
        let val = value.clone().into_os_string().to_str().unwrap().to_string();

        if val.contains("web") {
            println!("what is this {}", val);
        }
    } */

    //println!("entries here {:?}", entries);

    Ok(())
}

fn handle_events() -> io::Result<bool> {
    if event::poll(std::time::Duration::from_millis(50))? {
        if let Event::Key(key) = event::read()? {
            if key.kind == event::KeyEventKind::Press && key.code == KeyCode::Char('q') {
                return Ok(true);
            }
        }
    }

    Ok(false)
}

fn ui(frame: &mut Frame) {
    frame.render_widget(
        Paragraph::new("Hello workdd").block(Block::bordered().title("testing")),
        frame.size(),
    );
}
