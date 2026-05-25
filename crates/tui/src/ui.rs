use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Style};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::{App, Page};

pub fn draw(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // status bar
            Constraint::Min(0),    // main content
            Constraint::Length(1), // shortcut bar
        ])
        .split(f.area());

    // Status bar
    let status_text = format!(
        " My Virtual Company    Agents: {} loaded    Notifications: {}",
        app.agent_count, app.notification_count
    );
    let status = Paragraph::new(status_text).style(Style::default().fg(Color::Cyan));
    f.render_widget(status, chunks[0]);

    // Main content
    match app.current_page {
        Page::Home => crate::pages::home::draw(f, chunks[1], app),
        Page::Agents => crate::pages::agents::draw(f, chunks[1], app),
        Page::Tasks => crate::pages::tasks::draw(f, chunks[1], app),
        Page::Logs => crate::pages::logs::draw(f, chunks[1], app),
        Page::Feishu => crate::pages::feishu::draw(f, chunks[1], app),
        Page::Memory => crate::pages::memory::draw(f, chunks[1], app),
    }

    // Shortcut bar
    let shortcuts = " F1:Help  F2:Agents  F3:Tasks  F4:Logs  F5:Feishu  F6:Memory  Q:Quit ";
    let shortcut = Paragraph::new(shortcuts).style(Style::default().fg(Color::DarkGray));
    f.render_widget(shortcut, chunks[2]);
}
