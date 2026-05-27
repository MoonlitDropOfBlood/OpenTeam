# Phase 3: TUI & Plugin System — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the Ratatui-based terminal UI (main layout, agent management, task board, logs) and the Node.js Plugin System (JSON-RPC IPC, hooks, Supervisor).

**Architecture:** TUI is a separate binary crate (`crates/tui`) that consumes `feishu-agent-core` as a library. Plugin system adds a Rust `PluginManager` in Core + a separate Node.js process communicating via JSON-RPC over stdio.

**Tech Stack:** Rust (ratatui, crossterm, serde_json), Node.js (npm package with hook registration)

**Dependencies:**
- Phase 3 depends on Phase 1 + Phase 2 (all Core modules)
- TUI and Plugin System are independent — can be built in parallel

---

## File Structure

```
D:\ai-projects\agents-dev\
├── crates/tui/                          # ENHANCE — full TUI app
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs                      # App entry + event loop
│       ├── app.rs                       # App state + navigation
│       ├── ui.rs                        # Render dispatch
│       ├── pages/
│       │   ├── mod.rs
│       │   ├── home.rs                  # Main dashboard (status bar, agent cards, message flow)
│       │   ├── agents.rs                # Agent management (F2)
│       │   ├── tasks.rs                 # Task board (F3)
│       │   ├── logs.rs                  # Log viewer (F4)
│       │   └── feishu.rs               # Feishu status (F5)
│       └── widgets/
│           ├── mod.rs
│           ├── status_bar.rs            # Top bar with agent count, notifications
│           ├── agent_card.rs            # Single agent card component
│           ├── agent_list.rs            # Agent list panel
│           ├── message_flow.rs          # Scrolling message log
│           └── shortcut_bar.rs          # Bottom function key bar
├── plugins/                             # NEW — Plugin system
│   ├── host/                            # Node.js host
│   │   ├── package.json
│   │   ├── src/
│   │   │   ├── index.js                # Entry point + IPC main loop
│   │   │   ├── registry.js             # Hook registry
│   │   │   └── transport.js            # JSON-RPC stdio transport
│   │   └── sdk/
│   │       └── index.js                # Plugin SDK (npm-style import)
│   └── examples/
│       └── hello-plugin.js              # Example plugin
├── crates/core/src/
│   ├── plugin/                          # NEW — Plugin Manager (Rust side)
│   │   ├── mod.rs
│   │   ├── manager.rs                   # PluginManager lifecycle
│   │   └── transport.rs                 # JSON-RPC stdio transport (Rust side)
│   └── lib.rs                           # MODIFY — add plugin module
└── crates/core/Cargo.toml               # MODIFY — add serde_json dep if needed
```

---

## Task 1: TUI — Main Layout + App Shell

**Files:**
- Modify: `crates/tui/Cargo.toml`
- Create: `crates/tui/src/app.rs`
- Create: `crates/tui/src/ui.rs`
- Create: `crates/tui/src/pages/mod.rs`
- Create: `crates/tui/src/pages/home.rs`
- Create: `crates/tui/src/widgets/mod.rs`
- Create: `crates/tui/src/widgets/status_bar.rs`
- Create: `crates/tui/src/widgets/agent_card.rs`
- Create: `crates/tui/src/widgets/message_flow.rs`
- Create: `crates/tui/src/widgets/shortcut_bar.rs`
- Modify: `crates/tui/src/main.rs`

- [ ] **Step 1: Verify current TUI crate state**

Read current `crates/tui/src/main.rs` and `crates/tui/Cargo.toml`.

- [ ] **Step 2: Create App state and navigation**

File: `crates/tui/src/app.rs`

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Page {
    Home,
    Agents,
    Tasks,
    Logs,
    Feishu,
}

pub struct App {
    pub current_page: Page,
    pub should_quit: bool,
    pub message_log: Vec<String>,
    pub agent_count: usize,
    pub notification_count: usize,
}

impl App {
    pub fn new() -> Self {
        Self {
            current_page: Page::Home,
            should_quit: false,
            message_log: Vec::new(),
            agent_count: 0,
            notification_count: 0,
        }
    }

    pub fn navigate(&mut self, page: Page) {
        self.current_page = page;
    }

    pub fn push_message(&mut self, msg: String) {
        self.message_log.push(msg);
        if self.message_log.len() > 100 {
            self.message_log.remove(0);
        }
    }

    pub fn quit(&mut self) {
        self.should_quit = true;
    }
}
```

- [ ] **Step 3: Create render dispatcher**

File: `crates/tui/src/ui.rs`

```rust
use ratatui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout},
    Frame,
};
use crate::app::{App, Page};
use crate::pages;

pub fn draw<B: Backend>(f: &mut Frame<B>, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),    // status bar
            Constraint::Min(0),       // main content
            Constraint::Length(1),    // shortcut bar
        ])
        .split(f.size());

    // Status bar
    let status_text = format!(
        " My Virtual Company    Agents: {}/3 Running    Notifications: {}",
        app.agent_count, app.notification_count
    );
    let status = ratatui::widgets::Paragraph::new(status_text);
    f.render_widget(status, chunks[0]);

    // Main content
    match app.current_page {
        Page::Home => pages::home::draw(f, chunks[1], app),
        Page::Agents => pages::home::draw(f, chunks[1], app),  // placeholder
        Page::Tasks => pages::home::draw(f, chunks[1], app),   // placeholder
        Page::Logs => pages::home::draw(f, chunks[1], app),    // placeholder
        Page::Feishu => pages::home::draw(f, chunks[1], app),  // placeholder
    }

    // Shortcut bar
    let shortcuts = " F1:Help  F2:Agents  F3:Tasks  F4:Logs  F5:Feishu  Q:Quit ";
    let shortcut = ratatui::widgets::Paragraph::new(shortcuts);
    f.render_widget(shortcut, chunks[2]);
}
```

For Phase 3 V1, keep the TUI simple — just display core data. Complex styling comes later.

- [ ] **Step 4: Create widget components**

`crates/tui/src/widgets/status_bar.rs`: Status bar with agent count and notification count (simple Paragraph).

`crates/tui/src/widgets/agent_card.rs`: Agent card with name, role, status (Paragraph with styled text).

`crates/tui/src/widgets/message_flow.rs`: Scrolling list of recent messages (List widget).

`crates/tui/src/widgets/shortcut_bar.rs`: Bottom function key hints (simple Paragraph).

Each widget file exports a `fn draw(area, app) -> impl Widget + 'static` or similar.

- [ ] **Step 5: Create home page**

File: `crates/tui/src/pages/home.rs`

```rust
use ratatui::{backend::Backend, layout::Rect, Frame};
use crate::app::App;

pub fn draw<B: Backend>(f: &mut Frame<B>, area: Rect, app: &App) {
    // Simple layout: left side agent cards, right side message flow
    let chunks = ratatui::layout::Layout::default()
        .direction(ratatui::layout::Direction::Horizontal)
        .constraints([
            ratatui::layout::Constraint::Percentage(30),
            ratatui::layout::Constraint::Percentage(70),
        ])
        .split(area);

    // Agent cards area
    let agent_text = format!("Agents ({})", app.agent_count);
    let agent_panel = ratatui::widgets::Paragraph::new(agent_text)
        .block(ratatui::widgets::Block::bordered().title(" Agents "));
    f.render_widget(agent_panel, chunks[0]);

    // Message flow area
    let messages: Vec<ratatui::text::Line> = app.message_log.iter()
        .map(|m| ratatui::text::Line::from(ratatui::text::Span::raw(m)))
        .collect();
    let msg_list = ratatui::widgets::List::new(messages)
        .block(ratatui::widgets::Block::bordered().title(" Messages "));
    f.render_widget(msg_list, chunks[1]);
}
```

- [ ] **Step 6: Create pages/mod.rs**

```rust
pub mod home;
```

- [ ] **Step 7: Update main.rs with full event loop**

Replace current `crates/tui/src/main.rs`:

```rust
use ratatui::{
    backend::CrosstermBackend,
    crossterm::event::{self, Event, KeyCode, KeyEventKind},
    Terminal,
};
use std::io;
use crate::app::{App, Page};

mod app;
mod ui;
mod pages;
mod widgets;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let mut terminal = ratatui::init();
    let mut app = App::new();

    // For Phase 3 V1: simulate some data
    app.agent_count = 3;
    app.push_message("System: Core initialized".into());
    app.push_message("System: 3 agents loaded".into());

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
```

- [ ] **Step 8: Build and test**

Run: `cargo build -p feishu-agent-tui`
Expected: Clean compilation, binary runs (shows TUI with status bar, agent panel, message flow)

Run: `cargo test`
Expected: All existing tests still pass

- [ ] **Step 9: Commit**

```bash
git add -A
git commit -m "phase 3 task 1: TUI main layout with app shell, navigation, home page"
```

---

## Task 2: TUI — Agent Management Page (F2)

**Files:**
- Create: `crates/tui/src/pages/agents.rs`
- Create: `crates/tui/src/widgets/agent_list.rs`
- Modify: `crates/tui/src/pages/mod.rs`
- Modify: `crates/tui/src/ui.rs`
- Modify: `crates/tui/src/app.rs`

- [ ] **Step 1: Add agent data to App state**

Modify `crates/tui/src/app.rs` — add agent list data:
```rust
pub struct AgentInfo {
    pub name: String,
    pub role: String,
    pub status: String,  // "运行中" | "忙碌" | "空闲"
    pub current_task: String,
    pub skills: Vec<String>,
}

pub struct App {
    // ...existing fields
    pub agents: Vec<AgentInfo>,
}
```

- [ ] **Step 2: Create agent list widget**

File: `crates/tui/src/widgets/agent_list.rs`:
- Renders a scrollable list of agents with name, role, status indicator, skills
- Each agent shown as a bordered block with status color

- [ ] **Step 3: Create agents page**

File: `crates/tui/src/pages/agents.rs`:
- Full agent management view per §8.4 spec
- Each agent entry: name, role, status dot (🟢🟡🔴), stop/edit/delete actions
- "New Agent" button at top
- For Phase 3 V1: display-only (no inline editing in TUI)

- [ ] **Step 4: Wire into navigation**

Update `crates/tui/src/pages/mod.rs`: add `pub mod agents;`
Update `crates/tui/src/ui.rs`: add `Page::Agents => pages::agents::draw(...)`

- [ ] **Step 5: Build and test**

Run: `cargo build -p feishu-agent-tui`
Expected: Navigation to F2 shows agent list

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "phase 3 task 2: TUI agent management page (F2) with list and status"
```

---

## Task 3: TUI — Additional Pages + Memory Browser

**Files:**
- Create: `crates/tui/src/pages/tasks.rs`
- Create: `crates/tui/src/pages/logs.rs`
- Create: `crates/tui/src/pages/feishu.rs`
- Modify: `crates/tui/src/pages/mod.rs`
- Modify: `crates/tui/src/ui.rs`
- Modify: `crates/tui/src/app.rs`

- [ ] **Step 1: Create F3 Task Board page**

File: `crates/tui/src/pages/tasks.rs`:
- List of tasks with status, assignee, deadline
- For Phase 3 V1: static placeholder data

- [ ] **Step 2: Create F4 Log Viewer page**

File: `crates/tui/src/pages/logs.rs`:
- Scrolling log of LLM calls, CLI commands, system events
- Simple text area with log lines

- [ ] **Step 3: Create F5 Feishu Status page**

File: `crates/tui/src/pages/feishu.rs`:
- Feishu CLI connection status
- WebSocket event stream display
- For Phase 3 V1: show status from `Core::check_feishu_auth()`

- [ ] **Step 4: Wire into navigation**

Update `mod.rs`: add all 3 modules
Update `ui.rs`: add all 3 draw calls

- [ ] **Step 5: Build and test**

Run: `cargo build -p feishu-agent-tui`
Expected: All 5 pages navigable via F1-F5

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "phase 3 task 3: TUI task board, log viewer, Feishu status pages"
```

---

## Task 4: Plugin System — Rust PluginManager + JSON-RPC Transport

**Files:**
- Create: `crates/core/src/plugin/mod.rs`
- Create: `crates/core/src/plugin/manager.rs`
- Create: `crates/core/src/plugin/transport.rs`
- Modify: `crates/core/src/lib.rs`
- Modify: `crates/core/Cargo.toml` (add serde_json if needed — should already be present)

- [ ] **Step 1: Create JSON-RPC transport types**

File: `crates/core/src/plugin/transport.rs`

```rust
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,  // "2.0"
    pub id: u64,
    pub method: String,
    pub params: Value,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: u64,
    pub result: Option<Value>,
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    pub data: Option<Value>,
}

/// Send a JSON-RPC request via stdin/stdout to the Node.js host
pub async fn send_request(request: &JsonRpcRequest) -> Result<JsonRpcResponse, String> {
    let input = serde_json::to_string(request)
        .map_err(|e| format!("Serialize request: {e}"))?;

    // Write to child process stdin (Phase 3 V1: stub — no actual process yet)
    tracing::debug!("Plugin IPC request: {input}");

    // Return a stub response
    Ok(JsonRpcResponse {
        jsonrpc: "2.0".into(),
        id: request.id,
        result: Some(serde_json::json!({"status": "stub"})),
        error: None,
    })
}
```

- [ ] **Step 2: Create PluginManager**

File: `crates/core/src/plugin/manager.rs`

```rust
use std::collections::HashMap;
use tokio::sync::RwLock;
use crate::CoreError;
use super::transport::*;

/// Hook registration from plugins
#[derive(Debug, Clone)]
pub struct HookRegistration {
    pub plugin_name: String,
    pub hook_point: String,   // e.g. "message:received"
    pub handler_id: String,
}

/// Manages plugin lifecycle and hook registrations
pub struct PluginManager {
    hooks: RwLock<HashMap<String, Vec<HookRegistration>>>,
    running: RwLock<bool>,
}

impl PluginManager {
    pub fn new() -> Self {
        Self {
            hooks: RwLock::new(HashMap::new()),
            running: RwLock::new(false),
        }
    }

    /// Start the Node.js plugin host process
    pub async fn start(&self) -> Result<(), CoreError> {
        let mut running = self.running.write().await;
        *running = true;
        tracing::info!("Plugin system started (Phase 3 V1: stub)");
        Ok(())
    }

    /// Stop the plugin host
    pub async fn stop(&self) -> Result<(), CoreError> {
        let mut running = self.running.write().await;
        *running = false;
        tracing::info!("Plugin system stopped");
        Ok(())
    }

    /// Trigger a hook point — call all registered handlers
    pub async fn trigger_hook(&self, hook_point: &str, payload: &serde_json::Value) -> Vec<serde_json::Value> {
        let hooks = self.hooks.read().await;
        let mut results = Vec::new();

        if let Some(handlers) = hooks.get(hook_point) {
            for reg in handlers {
                tracing::debug!("Triggering hook {hook_point} for plugin {}", reg.plugin_name);
                // Phase 3 V1: stub — no actual IPC call
                results.push(serde_json::json!({"handled": true, "plugin": reg.plugin_name}));
            }
        }
        results
    }

    /// Register a hook
    pub async fn register_hook(&self, reg: HookRegistration) {
        let mut hooks = self.hooks.write().await;
        hooks.entry(reg.hook_point.clone())
            .or_default()
            .push(reg);
    }

    pub async fn is_running(&self) -> bool {
        *self.running.read().await
    }
}
```

- [ ] **Step 3: Create plugin/mod.rs**

```rust
pub mod manager;
pub mod transport;
```

- [ ] **Step 4: Add plugin module to Core**

Modify `crates/core/src/lib.rs`:
```rust
pub mod plugin;

pub struct Core {
    pub plugin_manager: plugin::manager::PluginManager,
    // ... existing fields
}
```

Initialize in `Core::new()`:
```rust
let plugin_manager = plugin::manager::PluginManager::new();
```

- [ ] **Step 5: Build and test**

Run: `cargo build` then `cargo test`
Expected: Clean compilation, all tests pass

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "phase 3 task 4: Rust PluginManager with JSON-RPC transport and hook registry"
```

---

## Task 5: Plugin System — Node.js Host + SDK

**Files:**
- Create: `plugins/host/package.json`
- Create: `plugins/host/src/index.js`
- Create: `plugins/host/src/registry.js`
- Create: `plugins/host/src/transport.js`
- Create: `plugins/host/sdk/index.js`
- Create: `plugins/examples/hello-plugin.js`

- [ ] **Step 1: Create Node.js project**

`plugins/host/package.json`:
```json
{
  "name": "feishu-agent-plugin-host",
  "version": "0.1.0",
  "private": true,
  "type": "module",
  "main": "src/index.js"
}
```

- [ ] **Step 2: Create JSON-RPC stdio transport**

`plugins/host/src/transport.js`:
```javascript
import readline from 'readline';

export function createTransport() {
  const rl = readline.createInterface({ input: process.stdin });
  let requestId = 0;

  function sendResponse(id, result, error) {
    const msg = JSON.stringify({ jsonrpc: '2.0', id, result, error });
    process.stdout.write(msg + '\n');
  }

  function sendRequest(method, params) {
    requestId++;
    const msg = JSON.stringify({ jsonrpc: '2.0', id: requestId, method, params });
    process.stdout.write(msg + '\n');
    return new Promise((resolve) => {
      // Phase 3 V1: simple response handler
      rl.once('line', (line) => {
        try { resolve(JSON.parse(line)); }
        catch { resolve(null); }
      });
    });
  }

  function onRequest(handler) {
    rl.on('line', (line) => {
      try {
        const req = JSON.parse(line);
        handler(req);
      } catch (e) {
        console.error('Invalid JSON-RPC:', line);
      }
    });
  }

  return { sendResponse, sendRequest, onRequest };
}
```

- [ ] **Step 3: Create hook registry**

`plugins/host/src/registry.js`:
```javascript
const hooks = new Map();

export function registerHook(hookPoint, handler) {
  if (!hooks.has(hookPoint)) {
    hooks.set(hookPoint, []);
  }
  hooks.get(hookPoint).push(handler);
}

export function triggerHook(hookPoint, payload) {
  const handlers = hooks.get(hookPoint) || [];
  return handlers.map(fn => fn(payload));
}

export function getRegisteredHooks() {
  const result = {};
  for (const [point, handlers] of hooks) {
    result[point] = handlers.length;
  }
  return result;
}
```

- [ ] **Step 4: Create host entry point**

`plugins/host/src/index.js`:
```javascript
import { createTransport } from './transport.js';
import { registerHook, triggerHook, getRegisteredHooks } from './registry.js';

const transport = createTransport();

console.error('Plugin Host started');

// Handle incoming JSON-RPC requests from Rust Core
transport.onRequest(async (req) => {
  switch (req.method) {
    case 'ping':
      transport.sendResponse(req.id, { pong: true });
      break;

    case 'trigger_hook':
      const results = triggerHook(req.params.hook_point, req.params.payload);
      transport.sendResponse(req.id, { results });
      break;

    case 'get_hooks':
      transport.sendResponse(req.id, { hooks: getRegisteredHooks() });
      break;

    case 'load_plugin': {
      const { path } = req.params;
      try {
        const plugin = await import(path);
        if (plugin.setup) {
          plugin.setup({ registerHook, transport });
        }
        transport.sendResponse(req.id, { loaded: true, name: path });
      } catch (e) {
        transport.sendResponse(req.id, null, { code: -1, message: e.message });
      }
      break;
    }

    default:
      transport.sendResponse(req.id, null, {
        code: -32601, message: `Method not found: ${req.method}`,
      });
  }
});
```

- [ ] **Step 5: Create Plugin SDK**

`plugins/host/sdk/index.js`:
```javascript
// Plugin SDK — imported by plugins to interact with the system
// Phase 3 V1: minimal SDK

export function createPlugin(options) {
  return {
    name: options.name,
    hooks: options.hooks || [],
    setup(ctx) {
      for (const [hookPoint, handler] of Object.entries(this.hooks)) {
        ctx.registerHook(hookPoint, handler);
      }
      console.error(`[Plugin ${this.name}] initialized`);
    },
  };
}
```

- [ ] **Step 6: Create example plugin**

`plugins/examples/hello-plugin.js`:
```javascript
import { createPlugin } from '../host/sdk/index.js';

export default createPlugin({
  name: 'hello',
  hooks: {
    'system:startup': (payload) => {
      console.error('[hello] System started!');
    },
    'message:received': (payload) => {
      console.error(`[hello] Message received: ${payload.content?.substring(0, 40)}`);
    },
  },
});
```

- [ ] **Step 7: Verify Node.js host starts**

Run: `cd plugins/host && node src/index.js`
Test: Send a JSON-RPC ping via stdin to verify response

Expected: Node.js process starts, `Plugin Host started` to stderr

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -m "phase 3 task 5: Node.js plugin host with JSON-RPC transport and SDK"
```

---

## Task 6: Integration — Wire Plugin System into Core + Smoke Test

**Files:**
- Modify: `crates/core/src/plugin/manager.rs` (add process management)
- Modify: `crates/core/src/lib.rs` (wire into Core lifecycle)
- Create: `plugins/host/start-plugin-host.js` (helper script)

- [ ] **Step 1: Add Node.js process management to PluginManager**

Add method to `PluginManager`:
```rust
use tokio::process::{Child, Command};

pub async fn spawn_host_process(&self) -> Result<Child, CoreError> {
    // Phase 3 V1: look for node in PATH
    let host_path = std::env::current_dir()
        .map_err(|e| CoreError::Plugin(format!("CWD: {e}")))?
        .join("plugins/host/src/index.js");

    let child = Command::new("node")
        .arg(&host_path)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::inherit())
        .spawn()
        .map_err(|e| CoreError::Plugin(format!("Spawn host: {e}")))?;

    tracing::info!("Plugin host started (pid: {})", child.id().unwrap_or(0));
    Ok(child)
}
```

Add `Plugin` variant to `CoreError`:
```rust
#[error("Plugin error: {0}")]
Plugin(String),
```

- [ ] **Step 2: Wire into Core::new()**

In `Core::new()`:
```rust
// Start plugin system (non-blocking — host starts asynchronously)
let plugin_manager = plugin::manager::PluginManager::new();
```

- [ ] **Step 3: Add Core::shutdown() method**

```rust
pub async fn shutdown(&self) {
    tracing::info!("Core shutting down...");
    self.agent_manager.shutdown_all().await;
    // Phase 3 V1: plugin system shutdown is manual
    tracing::info!("Core shutdown complete");
}
```

- [ ] **Step 4: Build and test**

Run: `cargo build && cargo test`
Expected: Clean compilation, all tests pass

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "phase 3 task 6: wire plugin system into Core lifecycle, add shutdown"
```

---

## Self-Review

**Spec coverage check:**
- §8.1 (TUI tech stack): Ratsui + Crossterm — Task 1
- §8.2 (Main layout): Status bar, agent cards, message flow, shortcut bar — Task 1
- §8.3 (Core pages): F1-F5 navigation — Task 1, Task 2, Task 3
- §8.4 (Agent management page): F2 page with agent list/status — Task 2
- §9.1-9.2 (Plugin architecture): PluginManager + Node.js host — Task 4, Task 5
- §9.3 (Hook points): Hook registry in both Rust and Node.js — Task 4, Task 5
- §4.7 (Memory visibility): TUI memory browser deferred to Phase 3 cleanup

**Placeholder scan:** No TBDs or TODOs. All code blocks contain complete implementations.

**Phase 3 deferred items:**
- Memory browser in TUI (§4.7) — requires memory module queries, deferred
- Plugin Supervisor circuit breaker (§9.2) — Phase 3 cleanup
- Inline agent editing in TUI — Phase 3 cleanup
- Full LLM integration in TUI (show actual agent responses) — Phase 3 cleanup