use ratatui::layout::{Rect, Alignment};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;
use crate::app::App;

pub fn draw(f: &mut Frame, area: Rect, _app: &App) {
    let title = Paragraph::new("Log Viewer")
        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        .alignment(Alignment::Center);
    f.render_widget(title, area);

    let inner = area.inner(ratatui::layout::Margin { vertical: 2, horizontal: 2 });

    let logs = vec![
        Line::from(vec!["14:32:01 [LLM] 小红 → claude-sonnet-4: 'Write PRD for user registration'".dim().into()]),
        Line::from(vec!["14:32:15 [LLM] claude-sonnet-4 → 小红: Response (342 tokens)".dim().into()]),
        Line::from(vec!["14:33:00 [FEISHU] Sent message to chat_xxxxxx".dim().into()]),
        Line::from(vec!["14:33:10 [CLI] lark-cli im +messages-send --chat-id ...".dim().into()]),
        Line::from(vec!["14:34:00 [SYSTEM] Agent CodeCat status: Busy".dim().into()]),
    ];

    let block = Block::default()
        .title(" Logs ")
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::Yellow));
    let block_inner = block.inner(inner);
    f.render_widget(block, inner);

    let list = Paragraph::new(logs)
        .wrap(Wrap { trim: false })
        .style(Style::default().fg(Color::White));
    f.render_widget(list, block_inner);
}
