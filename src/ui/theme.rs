pub struct OneDarkTheme;

impl OneDarkTheme {
    pub fn active_border() -> ratatui::style::Style {
        ratatui::style::Style::default()
            .fg(ratatui::style::Color::Cyan)
            .add_modifier(ratatui::style::Modifier::BOLD)
    }

    pub fn global_search() -> ratatui::style::Style {
        ratatui::style::Style::default()
            .fg(ratatui::style::Color::Yellow)
            .add_modifier(ratatui::style::Modifier::BOLD)
    }

    pub fn local_search() -> ratatui::style::Style {
        ratatui::style::Style::default()
            .fg(ratatui::style::Color::Green)
            .add_modifier(ratatui::style::Modifier::BOLD)
    }

    pub fn inactive_border() -> ratatui::style::Style {
        ratatui::style::Style::default().fg(ratatui::style::Color::Gray)
    }

    pub fn selected() -> ratatui::style::Style {
        ratatui::style::Style::default()
            .fg(ratatui::style::Color::Black)
            .bg(ratatui::style::Color::Cyan)
            .add_modifier(ratatui::style::Modifier::BOLD)
    }

    pub fn normal() -> ratatui::style::Style {
        ratatui::style::Style::default().fg(ratatui::style::Color::White)
    }

    pub fn disabled() -> ratatui::style::Style {
        ratatui::style::Style::default().fg(ratatui::style::Color::DarkGray)
    }

    pub fn search_highlight() -> ratatui::style::Style {
        ratatui::style::Style::default()
            .fg(ratatui::style::Color::Black)
            .bg(ratatui::style::Color::Yellow)
            .add_modifier(ratatui::style::Modifier::BOLD)
    }

    pub fn info() -> ratatui::style::Style {
        ratatui::style::Style::default().fg(ratatui::style::Color::Cyan)
    }
}
