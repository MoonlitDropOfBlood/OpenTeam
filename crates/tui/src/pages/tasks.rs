use ratatui::layout::Rect;
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;
use crate::app::App;

pub fn draw(f: &mut Frame, area: Rect, app: &App) {
    let inner = area.inner(ratatui::layout::Margin { vertical: 1, horizontal: 1 });

    let task_lines: Vec<Line> = if app.tasks.is_empty() {
        vec![Line::from(vec![
            "No active tasks. Tasks will appear here when agents are processing requests."
                .dim()
                .into(),
        ])]
    } else {
        app.tasks
            .iter()
            .flat_map(|task| {
                let status_color = match task.status.as_str() {
                    "Completed" | "PRD Complete" => Color::Green,
                    "In Progress" | "Active" => Color::Yellow,
                    "Gathering Requirements" | "Pending" => Color::Blue,
                    _ => Color::Gray,
                };
                vec![
                    Line::from(vec![
                        format!("[#{}] {}", task.id, &task.title).into(),
                        format!("  Assignee: {}", &task.assignee).into(),
                        format!("  Status: {}", &task.status).fg(status_color),
                    ]),
                    Line::from(vec!["".into()]),
                ]
            })
            .collect()
    };

    let task_count = app.tasks.len();

    let block = Block::default()
        .title(format!(" Tasks ({}) ", task_count))
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::Blue));
    let block_inner = block.inner(inner);
    f.render_widget(block, inner);

    let list = Paragraph::new(task_lines)
        .wrap(Wrap { trim: false })
        .style(Style::default().fg(Color::White));
    f.render_widget(list, block_inner);
}
