use ratatui::layout::{Margin, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::App;

pub fn draw(f: &mut Frame, area: Rect, app: &App) {
    // Page title
    let title = Paragraph::new("Agent Management")
        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));
    f.render_widget(title, area);

    // Agent list area (below title)
    let inner = area.inner(Margin {
        vertical: 2,
        horizontal: 2,
    });

    let agents_display: Vec<Line> = if app.agents.is_empty() {
        vec![Line::from("No agents registered. Press 'r' to refresh.")]
    } else {
        let mut lines: Vec<Line> = app
            .agents
            .iter()
            .flat_map(|agent| {
                let is_running = agent.status != "Idle" && agent.status != "Offline" && agent.status != "Paused";
                let run_icon = if is_running { "\u{25c9}" } else { "\u{25cb}" };
                let action = if is_running { "[Stop]" } else { "[Start]" };

                let status_color = match agent.status.as_str() {
                    "Running" | "Busy" => Color::Green,
                    "Idle" => Color::Yellow,
                    _ => Color::Gray,
                };

                vec![
                    Line::from(vec![
                        format!(
                            "{} {} ({})            ",
                            run_icon, agent.name, agent.role
                        )
                        .into(),
                        agent.status.clone().fg(status_color).into(),
                        format!("  {} [Edit] [Delete]", action).into(),
                    ]),
                    Line::from(vec!["".into()]),
                ]
            })
            .collect();
        // Add Phase 3 V3 note for stop/edit/delete buttons
        lines.push(Line::from(vec![
            "  [Stop] [Edit] [Delete] — Phase 3 V3: requires Feishu integration"
                .dim()
                .into(),
        ]));
        lines
    };

    let agent_block = Block::default()
        .title(format!(" Agents ({}) ", app.agents.len()))
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::Green));
    let agent_inner = agent_block.inner(inner);
    f.render_widget(agent_block, inner);

    let agent_list = Paragraph::new(agents_display)
        .style(Style::default().fg(Color::White))
        .wrap(Wrap { trim: false });
    f.render_widget(agent_list, agent_inner);
}
