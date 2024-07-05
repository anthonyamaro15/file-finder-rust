use std::rc::Rc;

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    Frame,
};

#[derive(Debug)]
pub struct Ui<'a> {
    f: Frame<'a>,
    container_chunk: Rc<[ratatui::layout::Rect]>,
}

impl Ui<'_> {
    pub fn new(f: Frame) -> Ui {
        let config = Ui {
            f,
            container_chunk: Layout::default(),
        };

        config
    }
    pub fn generate_ui_container(&mut self, f: Frame) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(2)
            .constraints(
                [
                    Constraint::Length(2),
                    Constraint::Length(3),
                    Constraint::Min(1),
                    Constraint::Length(3),
                    Constraint::Length(1),
                ]
                .as_ref(),
            )
            .split(f.size());

        self.container_chunk = chunks;
    }
}
