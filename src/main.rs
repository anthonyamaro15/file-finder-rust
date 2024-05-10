/* // get all files from Desktop dir
// have a loop that runs until user selects a file from list or exits
// as user types, filter the file list to show only files that match the user input
// if user selects a file, print the the file path (for now) and exit the program
//
//
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind},
    execute,
    terminal::{
        self, disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
    },
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

#[derive(Debug, Clone)]
struct App {
    files: Vec<String>,
    exit: bool,
}

impl App {
    pub fn new(files: Vec<String>) -> App {
        App { files, exit: false }
    }
    pub fn run(&mut self, terminal: &mut Tui) -> anyhow::Result<()> {
        while !self.exit {
            terminal.draw(|frame| self.render_frame(frame))?;
            self.handle_events()?;
        }
        Ok(())
    }

    fn render_frame(&mut self, frame: &mut Frame) {
        let list = List::new(self.files.clone())
            .block(Block::default().title("list").borders(Borders::ALL))
            .style(Style::default().fg(Color::White))
            .highlight_style(Style::default().add_modifier(Modifier::ITALIC))
            .highlight_symbol(">>>")
            .repeat_highlight_symbol(true)
            .direction(ListDirection::BottomToTop);
        frame.render_widget(list, frame.size());
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

    let app_result = app.run(&mut terminal);
    restore()?;
    let _ = app_result;
    disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;

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
} */

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
use ratatui::{
    prelude::*,
    symbols::border,
    widgets::{block::*, *},
};
use std::{fs, io};

mod tui;

#[derive(Debug, Default, Clone)]
pub struct App {
    counter: u8,
    exit: bool,
    files: Vec<String>,
}

impl App {
    pub fn run(&mut self, terminal: &mut tui::Tui, files: Vec<String>) -> io::Result<()> {
        self.files = files;

        while !self.exit {
            terminal.draw(|frame| self.render_frame(frame))?;
            self.handle_events()?;
        }
        Ok(())
    }

    fn render_frame(&self, frame: &mut Frame) {
        let list = List::new(self.files.clone())
            .block(Block::default().title("test list").borders(Borders::ALL))
            .highlight_style(Style::default().add_modifier(Modifier::ITALIC))
            .direction(ListDirection::TopToBottom)
            .highlight_symbol(">>")
            .repeat_highlight_symbol(true);

        frame.render_widget(list, frame.size());
    }

    fn handle_events(&mut self) -> io::Result<()> {
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
            KeyCode::Char('q') => self.exit(),
            KeyCode::Left => self.decrement_counter(),
            KeyCode::Right => self.increment_counter(),
            _ => {}
        }
    }

    fn exit(&mut self) {
        self.exit = true;
    }

    fn increment_counter(&mut self) {
        self.counter += 1;
    }

    fn decrement_counter(&mut self) {
        self.counter -= 1;
    }
}

impl Widget for App {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let title = Title::from("Counter App".bold());
        let instructions = Title::from(Line::from(vec![
            "Decrement".into(),
            "<left>".blue().bold(),
            "Increment".into(),
            "Right".blue().bold(),
            "Quit".into(),
            "<Q>".blue().bold(),
        ]));

        let mut state = ListState::default();
        let block = Block::default()
            .title(title.alignment(Alignment::Center))
            .title(
                instructions
                    .alignment(Alignment::Center)
                    .position(Position::Bottom),
            )
            .borders(Borders::ALL)
            .border_set(border::THICK);

        let counter_text = Text::from(vec![Line::from(vec![
            "Value:".into(),
            self.counter.to_string().yellow(),
        ])]);

        /* let list = List::new(self.files.clone())
        .block(Block::default().title("test list").borders(Borders::ALL))
        .highlight_symbol(">>")
        .repeat_highlight_symbol(true)
        .render(area, buf); */

        /* Paragraph::new(counter_text)
        .centered()
        .block(block)
        .render(area, buf); */
    }
}

fn main() -> io::Result<()> {
    let entries = fs::read_dir("../../")?
        .map(|res| res.map(|e| e.path()))
        .collect::<Result<Vec<_>, io::Error>>()?;

    let mut file_strings: Vec<String> = Vec::new();

    for value in entries.iter() {
        let val = value.clone().into_os_string().to_str().unwrap().to_string();

        file_strings.push(val.clone());
    }

    println!("whar are values, {:?}", file_strings);

    let mut terminal = tui::init()?;
    let app_result = App::default().run(&mut terminal, file_strings);
    tui::restore()?;
    app_result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handle_key_event() -> io::Result<()> {
        let mut app = App::default();
        app.handle_key_event(KeyCode::Right.into());
        assert_eq!(app.counter, 1);

        app.handle_key_event(KeyCode::Left.into());
        assert_eq!(app.counter, 0);

        let mut app = App::default();
        app.handle_key_event(KeyCode::Char('q').into());
        assert_eq!(app.exit, true);

        Ok(())
    }
}
