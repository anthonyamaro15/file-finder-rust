use ratatui::style::{Color, Modifier, Style};

/// OneDark theme colors matching lazygit configuration
pub struct OneDarkTheme;

impl OneDarkTheme {
    // Core OneDark palette
    pub const BLUE: Color = Color::Rgb(97, 175, 239); // #61AFEF
    pub const GREEN: Color = Color::Rgb(152, 195, 121); // #98C379
    pub const RED: Color = Color::Rgb(224, 108, 117); // #E06C75
    pub const YELLOW: Color = Color::Rgb(229, 192, 123); // #E5C07B
    pub const PURPLE: Color = Color::Rgb(198, 120, 221); // #C678DD
    pub const CYAN: Color = Color::Rgb(86, 182, 194); // #56B6C2
    pub const ORANGE: Color = Color::Rgb(209, 154, 102); // #D19A66
    pub const FG: Color = Color::Rgb(171, 178, 191); // #ABB2BF
    pub const DIM: Color = Color::Rgb(62, 68, 81); // #3E4451
    pub const SELECTION_BG: Color = Color::Rgb(57, 65, 79); // #39414f (high contrast selection)

    // Theme styles for different UI elements
    pub fn normal() -> Style {
        Style::default().fg(Self::FG)
    }

    pub fn active_border() -> Style {
        Style::default().fg(Self::BLUE).add_modifier(Modifier::BOLD)
    }

    pub fn inactive_border() -> Style {
        Style::default().fg(Self::DIM)
    }

    pub fn selected() -> Style {
        Style::default()
            .bg(Self::SELECTION_BG)
            .fg(Color::White)
            .add_modifier(Modifier::BOLD)
    }

    pub fn search_active() -> Style {
        Style::default()
            .fg(Self::YELLOW)
            .add_modifier(Modifier::BOLD)
    }

    pub fn search_inactive() -> Style {
        Style::default().fg(Self::DIM)
    }

    pub fn success() -> Style {
        Style::default().fg(Self::GREEN)
    }

    pub fn error() -> Style {
        Style::default().fg(Self::RED)
    }

    pub fn warning() -> Style {
        Style::default().fg(Self::YELLOW)
    }

    pub fn info() -> Style {
        Style::default().fg(Self::CYAN)
    }

    pub fn directory() -> Style {
        Style::default().fg(Self::BLUE)
    }

    pub fn file() -> Style {
        Style::default().fg(Self::FG)
    }

    pub fn highlight() -> Style {
        Style::default()
            .fg(Self::YELLOW)
            .add_modifier(Modifier::BOLD)
    }

    pub fn disabled() -> Style {
        Style::default().fg(Self::DIM)
    }

    pub fn loading() -> Style {
        Style::default().fg(Self::YELLOW)
    }

    pub fn global_search() -> Style {
        Style::default()
            .fg(Self::PURPLE)
            .add_modifier(Modifier::BOLD)
    }

    pub fn local_search() -> Style {
        Style::default().fg(Self::CYAN).add_modifier(Modifier::BOLD)
    }

    pub fn search_highlight() -> Style {
        Style::default()
            .fg(Color::Black)
            .bg(Self::YELLOW)
            .add_modifier(Modifier::BOLD)
    }
}

/// Helper functions for common UI styling patterns
pub mod styles {
    use super::OneDarkTheme;
    use ratatui::style::Style;

    pub fn panel_active() -> Style {
        OneDarkTheme::active_border()
    }

    pub fn panel_inactive() -> Style {
        OneDarkTheme::inactive_border()
    }

    pub fn text_primary() -> Style {
        OneDarkTheme::normal()
    }

    pub fn text_secondary() -> Style {
        OneDarkTheme::disabled()
    }

    pub fn item_selected() -> Style {
        OneDarkTheme::selected()
    }

    pub fn item_normal() -> Style {
        OneDarkTheme::normal()
    }

    pub fn search_input_active() -> Style {
        OneDarkTheme::search_active()
    }

    pub fn search_input_inactive() -> Style {
        OneDarkTheme::search_inactive()
    }

    pub fn status_success() -> Style {
        OneDarkTheme::success()
    }

    pub fn status_error() -> Style {
        OneDarkTheme::error()
    }

    pub fn status_info() -> Style {
        OneDarkTheme::info()
    }

    pub fn file_directory() -> Style {
        OneDarkTheme::directory()
    }

    pub fn file_regular() -> Style {
        OneDarkTheme::file()
    }
}

/// Block builders for consistent UI styling with rounded borders
pub mod blocks {
    use super::OneDarkTheme;
    use ratatui::symbols::border;
    use ratatui::widgets::{Block, Borders};

    /// Create a rounded block with a title - active (focused) state
    pub fn panel_active<'a>(title: &'a str) -> Block<'a> {
        Block::default()
            .borders(Borders::ALL)
            .border_set(border::ROUNDED)
            .title(title)
            .style(OneDarkTheme::active_border())
    }

    /// Create a rounded block with a title - inactive state
    pub fn panel_inactive<'a>(title: &'a str) -> Block<'a> {
        Block::default()
            .borders(Borders::ALL)
            .border_set(border::ROUNDED)
            .title(title)
            .style(OneDarkTheme::inactive_border())
    }

    /// Create a rounded block for preview panels
    pub fn preview<'a>(title: &'a str) -> Block<'a> {
        Block::default()
            .borders(Borders::ALL)
            .border_set(border::ROUNDED)
            .title(title)
            .style(OneDarkTheme::inactive_border())
    }

    /// Create a rounded block for status/info displays
    pub fn status<'a>(title: &'a str) -> Block<'a> {
        Block::default()
            .borders(Borders::ALL)
            .border_set(border::ROUNDED)
            .title(title)
            .style(OneDarkTheme::info())
    }

    /// Create a rounded block for success messages
    pub fn success<'a>(title: &'a str) -> Block<'a> {
        Block::default()
            .borders(Borders::ALL)
            .border_set(border::ROUNDED)
            .title(title)
            .style(OneDarkTheme::success())
    }

    /// Create a rounded block for warnings
    pub fn warning<'a>(title: &'a str) -> Block<'a> {
        Block::default()
            .borders(Borders::ALL)
            .border_set(border::ROUNDED)
            .title(title)
            .style(OneDarkTheme::warning())
    }

    /// Create a rounded block for errors
    pub fn error<'a>(title: &'a str) -> Block<'a> {
        Block::default()
            .borders(Borders::ALL)
            .border_set(border::ROUNDED)
            .title(title)
            .style(OneDarkTheme::error())
    }

    /// Create a minimal rounded block (no title)
    pub fn minimal() -> Block<'static> {
        Block::default()
            .borders(Borders::ALL)
            .border_set(border::ROUNDED)
            .style(OneDarkTheme::inactive_border())
    }

    /// Create a popup block with rounded borders
    pub fn popup<'a>(title: &'a str) -> Block<'a> {
        Block::default()
            .borders(Borders::ALL)
            .border_set(border::ROUNDED)
            .title(title)
            .style(OneDarkTheme::info())
    }
}
