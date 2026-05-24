use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Borders, Paragraph};
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
        " My Virtual Company    Agents: {}/3 Running    Notifications: {}",
        app.agent_count, app.notification_count
    );
    let status = Paragraph::new(status_text).style(Style::default().fg(Color::Cyan));
    f.render_widget(status, chunks[0]);

    // Main content
    match app.current_page {
        Page::Home => crate::pages::home::draw(f, chunks[1], app),
        Page::Agents => crate::pages::agents::draw(f, chunks[1], app),
        Page::Tasks => draw_placeholder(f, chunks[1], "Task Board (F3)"),
        Page::Logs => draw_placeholder(f, chunks[1], "Log Viewer (F4)"),
        Page::Feishu => draw_placeholder(f, chunks[1], "Feishu Status (F5)"),
    }

    // Shortcut bar
    let shortcuts = " F1:Help  F2:Agents  F3:Tasks  F4:Logs  F5:Feishu  Q:Quit ";
    let shortcut = Paragraph::new(shortcuts).style(Style::default().fg(Color::DarkGray));
    f.render_widget(shortcut, chunks[2]);
}

fn draw_placeholder(f: &mut Frame, area: Rect, title: &str) {
    let block = Block::default()
        .title(format!(" {} ", title))
        .borders(Borders::ALL);
    f.render_widget(block, area);
}
