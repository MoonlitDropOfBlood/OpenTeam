use std::path::Path;
use std::time::Duration;

use feishu_agent_core::Core;
use feishu_agent_core::registry::AgentStatus;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind};

use crate::app::{AgentInfo, App, MemoryDisplay, Page};

mod app;
mod pages;
mod ui;
mod widgets;

async fn refresh_from_core(app: &mut App, core: &Core) {
    // Load agents from registry
    app.agents = core.registry.all().iter().map(|record| {
        let status_str = match record.status {
            AgentStatus::Idle => "Idle",
            AgentStatus::Busy => "Busy",
            AgentStatus::Paused => "Paused",
            AgentStatus::Offline => "Offline",
        };
        AgentInfo {
            name: record.config.name.clone(),
            role: record.config.role.clone(),
            status: status_str.into(),
        }
    }).collect();
    app.agent_count = app.agents.len();

    // Feishu connection status
    app.feishu_connected = core.check_feishu_auth().await;

    // Plugin system status
    app.plugin_running = core.plugin_manager.is_running().await;

    // Load memories from all agents
    app.memories.clear();
    for agent in core.list_agents() {
        let entries = core.memory_store.list_by_agent(&agent.config.name).await.unwrap_or_default();
        for entry in &entries {
            app.memories.push(MemoryDisplay {
                id: entry.id.to_string(),
                title: entry.title.clone(),
                summary: entry.summary.clone(),
                memory_type: format!("{:?}", entry.memory_type),
                importance: entry.importance,
                agent_name: entry.agent_id.clone(),
            });
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    // Initialize Core
    let agents_dir = Path::new("agents");
    let llm_config = Path::new("llm_config.yaml");
    let core = Core::new(agents_dir, llm_config, "sqlite:tui.db").await?;

    let mut terminal = ratatui::init();
    let mut app = App::new();

    // Populate app state from Core
    refresh_from_core(&mut app, &core).await;
    app.push_message("System: Core initialized".into());
    app.push_message(format!("System: {} agents loaded", app.agent_count));

    let mut last_refresh = tokio::time::Instant::now();
    let refresh_interval = Duration::from_secs(5);

    while !app.should_quit {
        terminal.draw(|f| ui::draw(f, &app))?;

        // Auto-refresh
        if app.auto_refresh && last_refresh.elapsed() >= refresh_interval {
            refresh_from_core(&mut app, &core).await;
            last_refresh = tokio::time::Instant::now();
        }

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => app.quit(),
                        KeyCode::Char('r') => {
                            refresh_from_core(&mut app, &core).await;
                            app.push_message("System: Data refreshed".into());
                            last_refresh = tokio::time::Instant::now();
                        }
                        KeyCode::Char('a') => {
                            app.auto_refresh = !app.auto_refresh;
                            let status = if app.auto_refresh { "ON" } else { "OFF" };
                            app.push_message(format!("System: Auto-refresh {status}"));
                        }
                        KeyCode::F(1) => app.navigate(Page::Home),
                        KeyCode::F(2) => app.navigate(Page::Agents),
                        KeyCode::F(3) => app.navigate(Page::Tasks),
                        KeyCode::F(4) => app.navigate(Page::Logs),
                        KeyCode::F(5) => app.navigate(Page::Feishu),
                        KeyCode::F(6) => app.navigate(Page::Memory),
                        _ => {}
                    }
                }
            }
        }
    }

    core.shutdown().await;
    ratatui::restore();
    Ok(())
}
