use std::fs;
use std::io::{BufRead, BufReader};
use ratatui::layout::{Rect, Alignment};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;
use crate::app::App;

/// Max lines to show in the log viewer
const MAX_LOG_LINES: usize = 200;

/// Read the latest log file and return its last N lines
fn read_recent_logs() -> Vec<String> {
    let logs_dir = feishu_agent_core::skill::registry::global_logs_dir();
    if !logs_dir.exists() {
        return vec!["(Log directory not found)".into()];
    }

    // Find the most recent log file (openteam.YYYY-MM-DD)
    let mut entries: Vec<_> = match fs::read_dir(&logs_dir) {
        Ok(e) => e.filter_map(|e| e.ok()).collect(),
        Err(_) => return vec!["(Cannot read log directory)".into()],
    };

    // Sort by modification time (newest first)
    entries.sort_by(|a, b| {
        b.metadata()
            .and_then(|m| m.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
            .cmp(
                &a.metadata()
                    .and_then(|m| m.modified())
                    .unwrap_or(std::time::SystemTime::UNIX_EPOCH),
            )
    });

    let latest = entries.into_iter().find(|e| {
        e.file_name().to_string_lossy().starts_with("openteam.")
    });

    let path = match latest {
        Some(e) => e.path(),
        None => return vec!["(No log files found)".into()],
    };

    // Read all lines, keep last N
    let file = match fs::File::open(&path) {
        Ok(f) => f,
        Err(e) => return vec![format!("(Error reading log: {e})")],
    };

    let reader = BufReader::new(file);
    let lines: Vec<String> = reader
        .lines()
        .filter_map(|l| l.ok())
        .filter(|l| !l.is_empty())
        .collect();

    let tail = if lines.len() > MAX_LOG_LINES {
        lines[lines.len() - MAX_LOG_LINES..].to_vec()
    } else {
        lines
    };

    if tail.is_empty() {
        vec!["(Log file is empty)".into()]
    } else {
        tail
    }
}

pub fn draw(f: &mut Frame, area: Rect, _app: &App) {
    let title = Paragraph::new("Log Viewer")
        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        .alignment(Alignment::Center);
    f.render_widget(title, area);

    let inner = area.inner(ratatui::layout::Margin { vertical: 2, horizontal: 2 });

    let block = Block::default()
        .title(" Logs (latest) ")
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::Yellow));
    let block_inner = block.inner(inner);
    f.render_widget(block, inner);

    let log_lines: Vec<Line> = read_recent_logs()
        .into_iter()
        .map(|l| Line::from(l.dim()))
        .collect();

    let list = Paragraph::new(log_lines)
        .wrap(Wrap { trim: false })
        .style(Style::default().fg(Color::White));
    f.render_widget(list, block_inner);
}