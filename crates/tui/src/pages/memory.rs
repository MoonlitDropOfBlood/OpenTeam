use ratatui::layout::{Rect, Alignment, Margin};
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;
use crate::app::App;

pub fn draw(f: &mut Frame, area: Rect, app: &App) {
    let inner = area.inner(Margin { vertical: 1, horizontal: 1 });

    if app.memories.is_empty() {
        let empty = Paragraph::new("No memories stored yet.\n\nPress 'r' to refresh from Core.\n(F6 to view this page)")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center);
        f.render_widget(empty, area);
        return;
    }

    let mut lines = Vec::new();
    for mem in &app.memories {
        let icon = match mem.memory_type.as_str() {
            "ShortTerm" => "[S]",
            "LongTerm" => "[L]",
            _ => "[?]",
        };
        let short_id = if mem.id.len() > 8 { &mem.id[..8] } else { &mem.id };
        let importance_stars = "*".repeat(mem.importance as usize);
        lines.push(Line::from(vec![
            format!("{icon}[{short_id}] {:.60}", mem.title).into(),
        ]));
        lines.push(Line::from(vec![
            format!("   Summary: {:.50}", mem.summary).dim().into(),
        ]));
        lines.push(Line::from(vec![
            format!("   Importance: {}  Agent: {}", importance_stars, mem.agent_name).dim().into(),
        ]));
        lines.push(Line::from(vec!["".into()]));
    }

    let block = Block::default()
        .title(format!(" Memories ({}) ", app.memories.len()))
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::Blue));
    let block_inner = block.inner(inner);
    f.render_widget(block, inner);

    let list = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .style(Style::default().fg(Color::White));
    f.render_widget(list, block_inner);
}
