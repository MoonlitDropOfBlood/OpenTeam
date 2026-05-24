use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind};

use crate::app::{App, Page};

mod app;
mod pages;
mod ui;
mod widgets;

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let mut terminal = ratatui::init();
    let mut app = App::new();

    // Seed demo data
    app.agent_count = 3;
    app.push_message("System: Core initialized".into());
    app.push_message("System: 3 agents loaded".into());
    app.push_message("小红: PRD written, @CodeCat please review".into());

    while !app.should_quit {
        terminal.draw(|f| ui::draw(f, &app))?;

        if let Event::Key(key) = event::read()? {
            if key.kind == KeyEventKind::Press {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => app.quit(),
                    KeyCode::F(1) => app.navigate(Page::Home),
                    KeyCode::F(2) => app.navigate(Page::Agents),
                    KeyCode::F(3) => app.navigate(Page::Tasks),
                    KeyCode::F(4) => app.navigate(Page::Logs),
                    KeyCode::F(5) => app.navigate(Page::Feishu),
                    _ => {}
                }
            }
        }
    }

    ratatui::restore();
    Ok(())
}
