use ratatui::layout::{Rect, Alignment};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;
use crate::app::App;

pub fn draw(f: &mut Frame, area: Rect, app: &App) {
    let title = Paragraph::new("Feishu Status")
        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        .alignment(Alignment::Center);
    f.render_widget(title, area);

    let inner = area.inner(ratatui::layout::Margin { vertical: 2, horizontal: 2 });

    let feishu_status = if app.feishu_connected {
        "Connected".to_string().fg(Color::Green)
    } else {
        "Disconnected".to_string().fg(Color::Red)
    };

    let plugin_status = if app.plugin_running {
        "Running".to_string().fg(Color::Green)
    } else {
        "Stopped".to_string().fg(Color::DarkGray)
    };

    let status = vec![
        Line::from(vec!["Feishu CLI: ".into(), feishu_status]),
        Line::from(vec!["Plugin System: ".into(), plugin_status]),
        Line::from(vec!["".into()]),
        Line::from(vec!["Recent Events:".bold().into()]),
        Line::from(vec!["  im.message.receive_v1 — monitoring...".dim().into()]),
        Line::from(vec!["  im.message.reaction.created_v1 — monitoring...".dim().into()]),
        Line::from(vec!["".into()]),
        Line::from(format!("Agents: {} loaded | Database: sqlite:tui.db", app.agent_count)),
    ];

    let block = Block::default()
        .title(" Feishu Status ")
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::Magenta));
    let block_inner = block.inner(inner);
    f.render_widget(block, inner);

    let list = Paragraph::new(status)
        .wrap(Wrap { trim: false })
        .style(Style::default().fg(Color::White));
    f.render_widget(list, block_inner);
}
