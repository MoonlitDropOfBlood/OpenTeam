use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, List, Paragraph};
use ratatui::Frame;

use crate::app::App;

pub fn draw(f: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
        .split(area);

    // Agent cards panel
    let agent_block = Block::default()
        .title(format!(" Agents ({}) ", app.agent_count))
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::Green));
    let agent_inner = agent_block.inner(chunks[0]);
    f.render_widget(agent_block, chunks[0]);

    let cards_text: String = if app.agents.is_empty() {
        "No agents loaded.".into()
    } else {
        app.agents.iter().map(|a| {
            let status_icon = match a.status.as_str() {
                "Running" | "Busy" => "\u{1f7e1}",   // yellow circle
                "Idle" => "\u{1f7e2}",                // green circle
                "Offline" | "Paused" => "\u{26aa}",   // white circle
                _ => "\u{1f7e0}",                     // orange circle
            };
            format!("{} ({})  {} {}", a.name, a.role, status_icon, a.status)
        }).collect::<Vec<_>>().join("\n")
    };

    let cards = Paragraph::new(cards_text)
        .style(Style::default().fg(Color::White));
    f.render_widget(cards, agent_inner);

    // Message flow panel
    let items: Vec<Line> = if app.message_log.is_empty() {
        vec![Line::from("System ready. Waiting for messages...")]
    } else {
        app.message_log
            .iter()
            .map(|m| Line::from(ratatui::text::Span::raw(m.as_str())))
            .collect()
    };
    let msg_list = List::new(items).block(Block::default().title(" Messages ").borders(Borders::ALL));
    f.render_widget(msg_list, chunks[1]);
}
