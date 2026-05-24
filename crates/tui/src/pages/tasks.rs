use ratatui::layout::{Rect, Alignment};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;
use crate::app::App;

pub fn draw(f: &mut Frame, area: Rect, _app: &App) {
    let title = Paragraph::new("Task Board")
        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        .alignment(Alignment::Center);
    f.render_widget(title, area);

    let inner = area.inner(ratatui::layout::Margin { vertical: 2, horizontal: 2 });

    let tasks = vec![
        Line::from(vec![
            "[#42] User Registration Module".into(),
            "  Assignee: 小红 (PM)".into(),
            "  Status: PRD Complete".to_string().fg(Color::Green),
        ]),
        Line::from(vec!["".into()]),
        Line::from(vec![
            "[#43] Homepage Performance".into(),
            "  Assignee: CodeCat (Dev)".into(),
            "  Status: In Progress".to_string().fg(Color::Yellow),
        ]),
        Line::from(vec!["".into()]),
        Line::from(vec![
            "[#44] Dashboard Requirements".into(),
            "  Assignee: 小红 (PM)".into(),
            "  Status: Gathering Requirements".to_string().fg(Color::Blue),
        ]),
    ];

    let block = Block::default()
        .title(" Tasks ")
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::Blue));
    let block_inner = block.inner(inner);
    f.render_widget(block, inner);

    let list = Paragraph::new(tasks)
        .wrap(Wrap { trim: false })
        .style(Style::default().fg(Color::White));
    f.render_widget(list, block_inner);
}
