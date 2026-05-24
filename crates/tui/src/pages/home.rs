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

    let cards = Paragraph::new("小红 (PM)  🟢 Running\nCodeCat (Dev)  🟡 Busy\n小蓝 (QA)  🟢 Idle")
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
