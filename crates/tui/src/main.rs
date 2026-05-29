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
    // Enable info-level tracing by default for visibility
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "info");
    }
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let mut terminal = ratatui::init();

    // Show loading screen while Core initializes
    let mut core = {
        let agents_dir = feishu_agent_core::skill::registry::global_agents_dir();
        let llm_config = Path::new("llm_config.yaml");
        let frames = ["|", "/", "-", "\\"];
        let steps = [
            ("Loading configuration...", 0.1),
            ("Initializing LLM Gateway...", 0.2),
            ("Connecting memory store...", 0.3),
            ("Discovering skills...", 0.5),
            ("Probing MCP servers...", 0.6),
            ("Starting plugin system...", 0.8),
            ("Ready!", 1.0),
        ];
        let mut step_idx = 0;

        // Spawn Core init in background
        let core_fut = Core::new(&agents_dir, llm_config, "sqlite:tui.db");
        let mut core_poll = std::pin::pin!(core_fut);

        loop {
            tokio::select! {
                result = &mut core_poll => {
                    match result {
                        Ok(core) => break core,
                        Err(e) => {
                            // Show error on loading screen
                            terminal.draw(|f| {
                                let area = f.area();
                                let lines = vec![
                                    ratatui::text::Line::from(ratatui::text::Span::styled(
                                        "OpenTeam",
                                        ratatui::style::Style::default()
                                            .fg(ratatui::style::Color::Cyan)
                                            .add_modifier(ratatui::style::Modifier::BOLD),
                                    )),
                                    ratatui::text::Line::from(""),
                                    ratatui::text::Line::from(ratatui::text::Span::styled(
                                        format!("Error: {e}"),
                                        ratatui::style::Style::default().fg(ratatui::style::Color::Red),
                                    )),
                                    ratatui::text::Line::from(""),
                                    ratatui::text::Line::from("Press any key to exit"),
                                ];
                                let block = ratatui::widgets::Block::bordered().title(" Startup Failed ");
                                let p = ratatui::widgets::Paragraph::new(lines).block(block).alignment(ratatui::layout::Alignment::Center);
                                let area = ratatui::layout::Layout::default()
                                    .direction(ratatui::layout::Direction::Vertical)
                                    .constraints([ratatui::layout::Constraint::Min(0)])
                                    .margin(4)
                                    .split(area)[0];
                                f.render_widget(p, area);
                            })?;
                            // Wait for keypress
                            loop {
                                if let Event::Key(_) = event::read()? { break; }
                            }
                            ratatui::restore();
                            return Err(e.into());
                        }
                    }
                }
                _ = tokio::time::sleep(std::time::Duration::from_millis(80)) => {
                    let progress = if step_idx < steps.len() - 1 {
                        steps[step_idx].1
                    } else {
                        1.0
                    };
                    let spinner = frames[step_idx % 4];
                    let label = if step_idx < steps.len() - 1 {
                        steps[step_idx].0
                    } else {
                        steps[steps.len() - 1].0
                    };
                    if step_idx < steps.len() - 1 {
                        step_idx += 1;
                    }

                    terminal.draw(|f| {
                        let area = f.area();
                        let bar_width = (area.width as f64 * 0.4) as u16;
                        let filled = (bar_width as f64 * progress) as u16;
                        let bar: String = (0..filled).map(|_| '█').chain((filled..bar_width).map(|_| '░')).collect();

                        let lines = vec![
                            ratatui::text::Line::from(ratatui::text::Span::styled(
                                "OpenTeam",
                                ratatui::style::Style::default()
                                    .fg(ratatui::style::Color::Cyan)
                                    .add_modifier(ratatui::style::Modifier::BOLD),
                            )),
                            ratatui::text::Line::from("AI Agent Orchestrator"),
                            ratatui::text::Line::from(""),
                            ratatui::text::Line::from(ratatui::text::Span::raw(format!(" {spinner} {label}"))),
                            ratatui::text::Line::from(ratatui::text::Span::raw(format!(" [{bar}]"))),
                        ];
                        let block = ratatui::widgets::Block::bordered().title(" Starting... ");
                        let p = ratatui::widgets::Paragraph::new(lines).block(block).alignment(ratatui::layout::Alignment::Center);
                        let area = ratatui::layout::Layout::default()
                            .direction(ratatui::layout::Direction::Vertical)
                            .constraints([ratatui::layout::Constraint::Min(0)])
                            .margin(4)
                            .split(area)[0];
                        f.render_widget(p, area);
                    })?;
                }
            }
        }
    };

    // Start background scheduler: Channel Bridge, send queue, MCP, file watchers
    core.start_scheduler().await;
    core.spawn_all_agents().await;

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
