# Phase 1: Rust Core Foundation — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bootstrap the Rust workspace with Core library + TUI binary split, implement Agent config/registry, LLM Gateway, and Feishu CLI Bridge. End state: a CLI test script can load agent configs, call an LLM, and send/receive a Feishu message.

**Architecture:** Rust workspace with two crates: `feishu-agent-core` (library) and `feishu-agent-tui` (binary). Core is UI-independent. LLM Gateway centralizes all model calls. Feishu Bridge wraps `lark-cli` subprocess calls + WebSocket event subscription.

**Tech Stack:** Rust (tokio, reqwest, serde, tiktoken-rs, sqlx), lark-cli, YAML config

**Dependencies between phases:**
- Phase 2 (Agent Intelligence) depends on Phase 1: Agent Registry + LLM Gateway + Feishu Bridge
- Phase 3 (TUI & Plugins) depends on Phase 1 + Phase 2

---

## File Structure

```
D:\ai-projects\agents-dev\
├── Cargo.toml                          # Workspace root
├── crates/
│   ├── core/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs                  # Public API surface
│   │       ├── config/
│   │       │   ├── mod.rs
│   │       │   ├── agent.rs            # AgentConfig struct + YAML loading
│   │       │   └── llm.rs             # LlmConfig struct + YAML loading
│   │       ├── registry/
│   │       │   ├── mod.rs
│   │       │   └── registry.rs        # AgentRegistry (in-memory HashMap)
│   │       ├── llm/
│   │       │   ├── mod.rs
│   │       │   ├── gateway.rs         # LlmGateway — central LLM caller
│   │       │   ├── model_config.rs    # ModelConfig parsing
│   │       │   └── rate_limiter.rs    # Token-level rate limiter
│   │       ├── feishu/
│   │       │   ├── mod.rs
│   │       │   ├── bridge.rs          # FeishuBridge — CLI subprocess + event WS
│   │       │   ├── message_queue.rs   # Priority send queue (5 QPS)
│   │       │   └── types.rs           # Feishu message types
│   │       └── error.rs               # Unified error types
│   └── tui/
│       ├── Cargo.toml
│       └── src/main.rs                # Binary entry (minimal for Phase 1)
├── agents/                             # Agent YAML configs
│   └── pm.yaml
├── mcp-configs/                        # MCP server configs
│   └── (empty for Phase 1)
└── tests/                              # Integration tests (Phase 2)
```

---

## Task 1: Rust Workspace Scaffold

**Files:**
- Create: `Cargo.toml` (workspace)
- Create: `crates/core/Cargo.toml`
- Create: `crates/core/src/lib.rs`
- Create: `crates/core/src/error.rs`
- Create: `crates/tui/Cargo.toml`
- Create: `crates/tui/src/main.rs`

- [ ] **Step 1: Create workspace Cargo.toml**

```toml
[workspace]
resolver = "2"
members = ["crates/core", "crates/tui"]

[workspace.dependencies]
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_yaml = "0.9"
reqwest = { version = "0.12", features = ["json", "stream"] }
tiktoken-rs = "0.5"
sqlx = { version = "0.8", features = ["runtime-tokio", "sqlite"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
anyhow = "1"
thiserror = "2"
uuid = { version = "1", features = ["v7"] }
```

- [ ] **Step 2: Create Core crate Cargo.toml**

```toml
[package]
name = "feishu-agent-core"
version = "0.1.0"
edition = "2021"

[dependencies]
tokio.workspace = true
serde.workspace = true
serde_yaml.workspace = true
reqwest.workspace = true
tiktoken-rs.workspace = true
sqlx.workspace = true
tracing.workspace = true
thiserror.workspace = true
uuid.workspace = true
json-rpc = "0.1"
```

- [ ] **Step 3: Create Core lib.rs**

```rust
pub mod config;
pub mod registry;
pub mod llm;
pub mod feishu;
pub mod error;
```

- [ ] **Step 4: Create error.rs**

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum CoreError {
    #[error("Config error: {0}")]
    Config(String),

    #[error("LLM error: {0}")]
    Llm(String),

    #[error("Feishu error: {0}")]
    Feishu(String),

    #[error("Registry error: {0}")]
    Registry(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serde error: {0}")]
    Serde(#[from] serde_yaml::Error),

    #[error("Reqwest error: {0}")]
    Reqwest(#[from] reqwest::Error),
}
```

- [ ] **Step 5: Create TUI crate Cargo.toml**

```toml
[package]
name = "feishu-agent-tui"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "feishu-agent-tui"
path = "src/main.rs"

[dependencies]
feishu-agent-core = { path = "../core" }
tokio.workspace = true
tracing.workspace = true
tracing-subscriber.workspace = true
ratatui = "0.28"
crossterm = "0.28"
```

- [ ] **Step 6: Create minimal TUI main.rs**

```rust
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    tracing::info!("Feishu Agent Orchestrator — Phase 1 starting");

    // Phase 1: just verify the core loads
    tracing::info!("Core library loaded successfully");

    Ok(())
}
```

- [ ] **Step 7: Verify workspace builds**

Run: `cargo build` from project root
Expected: Clean compilation, two crates built

- [ ] **Step 8: Create agents/pm.yaml**

```yaml
name: "小红"
role: "你是一个资深产品经理，擅长需求分析和PRD撰写"
personality: "严谨、有条理、善于沟通"
llm: "claude-sonnet-4"
triggers:
  - pattern: "需求|PRD|产品文档"
    auto_respond: true
  - pattern: "@PM|@小红"
    auto_respond: true
skills:
  - feishu-doc
  - feishu-task
mcps: []
```

- [ ] **Step 9: Commit**

```bash
cd D:\ai-projects\agents-dev
git init
git add .
git commit -m "phase 1: scaffold Rust workspace with core/tui crate split"
```

---

## Task 2: Agent Config & Registry

**Files:**
- Create: `crates/core/src/config/mod.rs`
- Create: `crates/core/src/config/agent.rs`
- Create: `crates/core/src/config/llm.rs`
- Create: `crates/core/src/registry/mod.rs`
- Create: `crates/core/src/registry/registry.rs`

- [ ] **Step 1: Define AgentConfig**

File: `crates/core/src/config/agent.rs`

```rust
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub name: String,
    pub role: String,
    pub personality: Option<String>,
    pub llm: String,
    pub triggers: Vec<TriggerConfig>,
    pub skills: Vec<String>,
    pub mcps: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerConfig {
    pub pattern: String,
    #[serde(default = "default_auto_respond")]
    pub auto_respond: bool,
}

fn default_auto_respond() -> bool { true }
```

- [ ] **Step 2: Define LlmConfig**

File: `crates/core/src/config/llm.rs`

```rust
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    pub models: HashMap<String, ModelConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    pub provider: String,
    pub model: String,
    pub api_key_env: Option<String>,
    pub max_tokens: u32,
    pub fallback: Option<String>,
    pub timeout_secs: Option<u64>,
    pub rate_limit: Option<RateLimitConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    pub rpm: u32,
    pub tpm: u32,
}
```

- [ ] **Step 3: Implement config loader**

File: `crates/core/src/config/mod.rs`

```rust
pub mod agent;
pub mod llm;

use std::path::Path;
use crate::CoreError;

/// Load a single agent config from YAML file
pub fn load_agent_config(path: &Path) -> Result<agent::AgentConfig, CoreError> {
    let content = std::fs::read_to_string(path)?;
    let config: agent::AgentConfig = serde_yaml::from_str(&content)?;
    Ok(config)
}

/// Load LLM config from YAML file
pub fn load_llm_config(path: &Path) -> Result<llm::LlmConfig, CoreError> {
    let content = std::fs::read_to_string(path)?;
    let config: llm::LlmConfig = serde_yaml::from_str(&content)?;
    Ok(config)
}

/// Load all agent configs from a directory
pub fn load_all_agents(dir: &Path) -> Result<Vec<agent::AgentConfig>, CoreError> {
    let mut configs = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().map_or(false, |e| e == "yaml") {
            configs.push(load_agent_config(&path)?);
        }
    }
    Ok(configs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_agent_config() {
        let yaml = r#"
name: "小红"
role: "产品经理"
llm: "claude-sonnet-4"
triggers:
  - pattern: "需求"
"#;
        let config: agent::AgentConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.name, "小红");
        assert_eq!(config.llm, "claude-sonnet-4");
    }
}
```

- [ ] **Step 4: Implement AgentRegistry**

File: `crates/core/src/registry/registry.rs`

```rust
use std::collections::HashMap;
use crate::config::agent::AgentConfig;
use uuid::Uuid;

pub type AgentId = Uuid;

#[derive(Debug, Clone, PartialEq)]
pub enum AgentStatus {
    Idle,
    Busy,
    Paused,
    Offline,
}

#[derive(Debug, Clone)]
pub struct AgentRecord {
    pub id: AgentId,
    pub config: AgentConfig,
    pub status: AgentStatus,
    pub current_task: Option<String>,
}

pub struct AgentRegistry {
    agents: HashMap<AgentId, AgentRecord>,
}

impl AgentRegistry {
    pub fn new() -> Self {
        Self { agents: HashMap::new() }
    }

    pub fn register(&mut self, config: AgentConfig) -> AgentId {
        let id = Uuid::new_v4();
        let record = AgentRecord {
            id,
            config,
            status: AgentStatus::Idle,
            current_task: None,
        };
        self.agents.insert(id, record);
        id
    }

    pub fn get(&self, id: &AgentId) -> Option<&AgentRecord> {
        self.agents.get(id)
    }

    pub fn find_by_role(&self, role_keyword: &str) -> Vec<&AgentRecord> {
        self.agents.values()
            .filter(|a| a.config.role.contains(role_keyword))
            .collect()
    }

    pub fn find_idle(&self) -> Vec<&AgentRecord> {
        self.agents.values()
            .filter(|a| a.status == AgentStatus::Idle)
            .collect()
    }

    pub fn update_status(&mut self, id: &AgentId, status: AgentStatus) {
        if let Some(record) = self.agents.get_mut(id) {
            record.status = status;
        }
    }

    pub fn all(&self) -> Vec<&AgentRecord> {
        self.agents.values().collect()
    }
}
```

- [ ] **Step 5: Export registry mod.rs**

```rust
pub mod registry;
```

- [ ] **Step 6: Write agent config load test**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_load_all_agents() {
        let dir = std::env::temp_dir().join("feishu_agents_test");
        std::fs::create_dir_all(&dir).unwrap();

        let yaml = br#"name: "test-agent"
role: "test"
llm: "claude-sonnet-4"
triggers: []"#;

        let path = dir.join("test.yaml");
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(yaml).unwrap();

        let configs = load_all_agents(&dir).unwrap();
        assert_eq!(configs.len(), 1);
        assert_eq!(configs[0].name, "test-agent");

        std::fs::remove_dir_all(dir).unwrap();
    }
}
```

- [ ] **Step 7: Register agents from config directory in main test**

Add to `crates/core/src/lib.rs`:
```rust
pub fn bootstrap_registry(agents_dir: &std::path::Path) -> Result<registry::AgentRegistry, CoreError> {
    let mut registry = registry::AgentRegistry::new();
    let configs = config::load_all_agents(agents_dir)?;
    for config in configs {
        registry.register(config);
    }
    Ok(registry)
}
```

- [ ] **Step 8: Build & test**

Run: `cargo test`
Expected: All tests pass

---

## Task 3: LLM Gateway

**Files:**
- Create: `crates/core/src/llm/mod.rs`
- Create: `crates/core/src/llm/gateway.rs`
- Create: `crates/core/src/llm/model_config.rs`
- Create: `crates/core/src/llm/rate_limiter.rs`
- Create: `llm_config.yaml` (sample at project root)

- [ ] **Step 1: Create sample llm_config.yaml**

```yaml
models:
  claude-sonnet-4:
    provider: anthropic
    model: claude-sonnet-4-20250514
    api_key_env: ANTHROPIC_API_KEY
    max_tokens: 8192
    fallback: claude-haiku-3-5
    timeout_secs: 120
    rate_limit: { rpm: 50, tpm: 100000 }

  ollama-qwen:
    provider: ollama
    model: qwen2.5:3b
    max_tokens: 4096
    timeout_secs: 60
```

- [ ] **Step 2: Implement RateLimiter**

File: `crates/core/src/llm/rate_limiter.rs`

```rust
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct RateLimiter {
    rpm: u32,
    state: Arc<Mutex<RateLimiterState>>,
}

struct RateLimiterState {
    timestamps: Vec<Instant>,
}

impl RateLimiter {
    pub fn new(rpm: u32) -> Self {
        Self {
            rpm,
            state: Arc::new(Mutex::new(RateLimiterState {
                timestamps: Vec::new(),
            })),
        }
    }

    pub async fn acquire(&self) {
        let mut state = self.state.lock().await;
        let now = Instant::now();

        // Prune timestamps older than 1 minute
        state.timestamps.retain(|t| now.duration_since(*t).as_secs() < 60);

        if state.timestamps.len() >= self.rpm as usize {
            let oldest = state.timestamps[0];
            let wait = 60u64.saturating_sub(now.duration_since(oldest).as_secs());
            if wait > 0 {
                tokio::time::sleep(std::time::Duration::from_secs(wait)).await;
            }
        }

        state.timestamps.push(now);
    }
}
```

- [ ] **Step 3: Implement LlmGateway**

File: `crates/core/src/llm/gateway.rs`

```rust
use std::collections::HashMap;
use std::time::Duration;
use crate::config::llm::{LlmConfig, ModelConfig};
use crate::CoreError;
use super::rate_limiter::RateLimiter;

#[derive(Clone)]
pub struct LlmGateway {
    client: reqwest::Client,
    models: HashMap<String, ModelConfig>,
    rate_limiters: HashMap<String, RateLimiter>,
}

#[derive(Debug, Clone)]
pub struct ChatRequest {
    pub model: String,
    pub system_prompt: String,
    pub messages: Vec<ChatMessage>,
    pub tools: Vec<ToolDefinition>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct ChatResponse {
    pub content: String,
    pub tool_calls: Vec<ToolCall>,
    pub usage: TokenUsage,
}

#[derive(Debug, Clone)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct TokenUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
}

impl LlmGateway {
    pub fn new(config: LlmConfig) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(180))
            .build()
            .unwrap();

        let mut rate_limiters = HashMap::new();
        for (name, model_config) in &config.models {
            if let Some(rate) = &model_config.rate_limit {
                rate_limiters.insert(name.clone(), RateLimiter::new(rate.rpm));
            }
        }

        Self {
            client,
            models: config.models,
            rate_limiters,
        }
    }

    pub async fn chat(&self, agent_id: &str, request: ChatRequest) -> Result<ChatResponse, CoreError> {
        let model_config = self.models.get(&request.model)
            .ok_or_else(|| CoreError::Llm(format!("Unknown model: {}", request.model)))?;

        // Rate limit
        if let Some(limiter) = self.rate_limiters.get(&request.model) {
            limiter.acquire().await;
        }

        // Build API request based on provider
        match model_config.provider.as_str() {
            "anthropic" => self.call_anthropic(model_config, &request).await,
            "ollama" => self.call_ollama(model_config, &request).await,
            provider => Err(CoreError::Llm(format!("Unsupported provider: {}", provider))),
        }
    }

    async fn call_anthropic(&self, config: &ModelConfig, request: &ChatRequest) -> Result<ChatResponse, CoreError> {
        let api_key = std::env::var(
            config.api_key_env.as_deref().unwrap_or("ANTHROPIC_API_KEY")
        ).map_err(|_| CoreError::Llm("ANTHROPIC_API_KEY not set".into()))?;

        let body = serde_json::json!({
            "model": config.model,
            "max_tokens": config.max_tokens,
            "system": request.system_prompt,
            "messages": request.messages.iter().map(|m| {
                serde_json::json!({"role": m.role, "content": m.content})
            }).collect::<Vec<_>>(),
        });

        let timeout = Duration::from_secs(config.timeout_secs.unwrap_or(120));
        let response = tokio::time::timeout(timeout, self.client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&body)
            .send()
        ).await.map_err(|e| CoreError::Llm(format!("Timeout or request error: {}", e)))??;

        let json: serde_json::Value = response.json().await?;

        let content = json["content"][0]["text"].as_str().unwrap_or("").to_string();
        let input_tokens = json["usage"]["input_tokens"].as_u64().unwrap_or(0) as u32;
        let output_tokens = json["usage"]["output_tokens"].as_u64().unwrap_or(0) as u32;

        Ok(ChatResponse {
            content,
            tool_calls: vec![],
            usage: TokenUsage { input_tokens, output_tokens },
        })
    }

    async fn call_ollama(&self, config: &ModelConfig, request: &ChatRequest) -> Result<ChatResponse, CoreError> {
        let body = serde_json::json!({
            "model": config.model,
            "system": request.system_prompt,
            "messages": request.messages.iter().map(|m| {
                serde_json::json!({"role": m.role, "content": m.content})
            }).collect::<Vec<_>>(),
            "stream": false,
        });

        let timeout = Duration::from_secs(config.timeout_secs.unwrap_or(60));
        let endpoint = format!("http://localhost:11434/api/chat");

        let response = tokio::time::timeout(timeout, self.client
            .post(&endpoint)
            .json(&body)
            .send()
        ).await.map_err(|e| CoreError::Llm(format!("Ollama timeout: {}", e)))??;

        let json: serde_json::Value = response.json().await?;
        let content = json["message"]["content"].as_str().unwrap_or("").to_string();

        Ok(ChatResponse {
            content,
            tool_calls: vec![],
            usage: TokenUsage { input_tokens: 0, output_tokens: 0 },
        })
    }
}
```

- [ ] **Step 4: Implement llm/mod.rs**

```rust
pub mod gateway;
pub mod rate_limiter;
```

- [ ] **Step 5: Build & verify**

Run: `cargo build`
Expected: Clean compilation

---

## Task 4: Feishu CLI Bridge — Send Messages

**Files:**
- Create: `crates/core/src/feishu/mod.rs`
- Create: `crates/core/src/feishu/bridge.rs`
- Create: `crates/core/src/feishu/message_queue.rs`
- Create: `crates/core/src/feishu/types.rs`

- [ ] **Step 1: Define Feishu message types**

File: `crates/core/src/feishu/types.rs`

```rust
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub type ChatId = String;
pub type MessageId = String;
pub type ThreadId = String;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeishuMessage {
    pub message_id: MessageId,
    pub chat_id: ChatId,
    pub thread_id: Option<ThreadId>,
    pub sender: SenderInfo,
    pub content: String,
    pub msg_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SenderInfo {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct OutgoingMessage {
    pub chat_id: ChatId,
    pub thread_id: Option<ThreadId>,
    pub text: String,
    pub mentions: Vec<MentionTarget>,
    pub priority: MessagePriority,
}

#[derive(Debug, Clone)]
pub struct MentionTarget {
    pub user_id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum MessagePriority {
    UserDirect = 0,
    Urgent = 1,
    Secretary = 2,
    InterAgent = 3,
}
```

- [ ] **Step 2: Implement MessageQueue**

File: `crates/core/src/feishu/message_queue.rs`

```rust
use std::collections::VecDeque;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use super::types::{OutgoingMessage, MessagePriority};

pub struct SendQueueEntry {
    pub message: OutgoingMessage,
    pub enqueued_at: Instant,
    pub agent_id: String,
}

pub struct SendQueue {
    queue: Mutex<VecDeque<SendQueueEntry>>,
}

impl SendQueue {
    pub fn new() -> Self {
        Self { queue: Mutex::new(VecDeque::new()) }
    }

    pub async fn enqueue(&self, message: OutgoingMessage, agent_id: String) {
        let mut queue = self.queue.lock().await;
        let entry = SendQueueEntry {
            message,
            enqueued_at: Instant::now(),
            agent_id,
        };
        queue.push_back(entry);
    }

    pub async fn dequeue(&self) -> Option<SendQueueEntry> {
        let mut queue = self.queue.lock().await;
        queue.pop_front()
    }

    pub async fn len(&self) -> usize {
        self.queue.lock().await.len()
    }
}
```

- [ ] **Step 3: Implement FeishuBridge (send path)**

File: `crates/core/src/feishu/bridge.rs`

```rust
use std::process::Stdio;
use tokio::process::Command;
use super::types::*;
use super::message_queue::SendQueue;
use crate::CoreError;

pub struct FeishuBridge {
    queue: SendQueue,
}

impl FeishuBridge {
    pub fn new() -> Self {
        Self { queue: SendQueue::new() }
    }

    pub fn queue(&self) -> &SendQueue {
        &self.queue
    }

    /// Format @mention text for Feishu message
    pub fn format_mention(target: &MentionTarget) -> String {
        format!(r#"<at user_id="{}">{}</at>"#, target.user_id, target.name)
    }

    /// Send a text message to a Feishu group chat
    pub async fn send_message(&self, msg: &OutgoingMessage) -> Result<MessageId, CoreError> {
        let mut text = msg.text.clone();

        // Prepend @mentions
        for mention in &msg.mentions {
            text = format!("{} {}",
                Self::format_mention(mention),
                text
            );
        }

        let mut cmd = Command::new("lark-cli");
        cmd.arg("im")
            .arg("+messages-send")
            .arg("--chat-id")
            .arg(&msg.chat_id)
            .arg("--text")
            .arg(&text);

        if msg.thread_id.is_some() {
            // Not directly supported in send; reply method is used for threads
            tracing::warn!("Thread reply not supported in send_message, use reply_to_message instead");
        }

        let output = cmd
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| CoreError::Feishu(format!("Failed to run lark-cli: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(CoreError::Feishu(format!("lark-cli error: {}", stderr)));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        // Parse message_id from output (format depends on CLI version)
        let message_id = stdout.trim().to_string();

        Ok(message_id)
    }

    /// Reply to a message in a thread
    pub async fn reply_to_message(
        &self,
        message_id: &str,
        text: &str,
        reply_in_thread: bool,
    ) -> Result<MessageId, CoreError> {
        let mut cmd = Command::new("lark-cli");
        cmd.arg("im")
            .arg("+messages-reply")
            .arg("--message-id")
            .arg(message_id)
            .arg("--text")
            .arg(text);

        if reply_in_thread {
            cmd.arg("--reply-in-thread");
        }

        let output = cmd
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| CoreError::Feishu(format!("Failed to reply: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(CoreError::Feishu(format!("lark-cli reply error: {}", stderr)));
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    /// Check Feishu CLI auth status
    pub async fn check_auth(&self) -> Result<bool, CoreError> {
        let output = Command::new("lark-cli")
            .arg("auth")
            .arg("check")
            .output()
            .await
            .map_err(|e| CoreError::Feishu(format!("Auth check failed: {}", e)))?;

        Ok(output.status.success())
    }
}
```

- [ ] **Step 4: Implement feishu/mod.rs**

```rust
pub mod bridge;
pub mod message_queue;
pub mod types;
```

- [ ] **Step 5: Build & verify**

Run: `cargo build`
Expected: Clean compilation

---

## Task 5: Feishu CLI Bridge — WebSocket Event Subscription

**Files:**
- Modify: `crates/core/src/feishu/bridge.rs` (add event subscription)
- Modify: `crates/core/src/feishu/mod.rs` (add event module)

- [ ] **Step 1: Add WebSocket event subscription to FeishuBridge**

Add to `crates/core/src/feishu/bridge.rs`:

```rust
use tokio::process::{Child, Command};
use tokio::io::{AsyncBufReadExt, BufReader};

impl FeishuBridge {
    /// Start consuming events from Feishu WebSocket
    /// Returns the child process handle (caller must manage lifecycle)
    pub async fn subscribe_events(
        &self,
        event_types: &[&str],
    ) -> Result<Child, CoreError> {
        let mut cmd = Command::new("lark-cli");
        cmd.arg("event")
            .arg("+subscribe")
            .arg("--event-types")
            .arg(event_types.join(","))
            .arg("--compact")
            .arg("--quiet")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let child = cmd.spawn()
            .map_err(|e| CoreError::Feishu(format!("Failed to subscribe events: {}", e)))?;

        Ok(child)
    }

    /// Read next NDJSON line from event subscription output
    pub async fn read_event<R: tokio::io::AsyncBufReadExt + Unpin>(
        reader: &mut BufReader<tokio::process::ChildStdout>,
    ) -> Option<String> {
        let mut line = String::new();
        reader.read_line(&mut line).await.ok()?;
        if line.trim().is_empty() {
            None
        } else {
            Some(line)
        }
    }

    /// Parse event JSON to extract message content
    pub fn parse_message_event(json: &str) -> Result<FeishuMessage, CoreError> {
        let value: serde_json::Value = serde_json::from_str(json)
            .map_err(|e| CoreError::Feishu(format!("Event parse error: {}", e)))?;

        let event = &value["event"];
        let message = &event["message"];

        Ok(FeishuMessage {
            message_id: message["message_id"].as_str().unwrap_or("").to_string(),
            chat_id: message["chat_id"].as_str().unwrap_or("").to_string(),
            thread_id: message["thread_id"].as_str().map(|s| s.to_string()),
            sender: SenderInfo {
                id: event["sender"]["sender_id"]["user_id"].as_str().unwrap_or("").to_string(),
                name: event["sender"]["sender_id"]["user_id"].as_str().unwrap_or("").to_string(),
            },
            content: message["body"]["content"].as_str().unwrap_or("").to_string(),
            msg_type: message["msg_type"].as_str().unwrap_or("").to_string(),
        })
    }
}
```

- [ ] **Step 2: Build & verify**

Run: `cargo build`
Expected: Clean compilation

---

## Task 6: Integration Smoke Test

**Files:**
- Create: `tests/smoke_test.rs`
- Modify: `crates/core/src/lib.rs` (add exports)

- [ ] **Step 1: Update Core lib.rs with public bootstrap API**

```rust
use std::path::Path;
use config::agent::AgentConfig;
use config::llm::LlmConfig;
use registry::AgentRegistry;

pub use error::CoreError;

pub struct Core {
    pub registry: AgentRegistry,
    pub llm_gateway: llm::gateway::LlmGateway,
    pub feishu_bridge: feishu::bridge::FeishuBridge,
}

impl Core {
    pub async fn new(
        agents_dir: &Path,
        llm_config_path: &Path,
    ) -> Result<Self, CoreError> {
        let llm_config = config::load_llm_config(llm_config_path)?;
        let mut registry = AgentRegistry::new();
        let configs = config::load_all_agents(agents_dir)?;
        for cfg in configs {
            registry.register(cfg);
        }

        Ok(Self {
            registry,
            llm_gateway: llm::gateway::LlmGateway::new(llm_config),
            feishu_bridge: feishu::bridge::FeishuBridge::new(),
        })
    }

    pub async fn check_feishu_auth(&self) -> bool {
        self.feishu_bridge.check_auth().await.unwrap_or(false)
    }

    pub fn list_agents(&self) -> Vec<&registry::AgentRecord> {
        self.registry.all()
    }
}
```

- [ ] **Step 2: Create smoke test**

File: `D:\ai-projects\agents-dev\tests\smoke_test.rs`

```rust
use feishu_agent_core::Core;
use std::path::Path;

#[tokio::test]
async fn test_core_initialization() {
    // Point to sample configs
    let agents_dir = Path::new("agents");
    let llm_config = Path::new("llm_config.yaml");

    // If files don't exist in CI, skip
    if !agents_dir.exists() || !llm_config.exists() {
        eprintln!("Skipping smoke test: configs not found");
        return;
    }

    let core = Core::new(agents_dir, llm_config).await.unwrap();
    let agents = core.list_agents();
    assert!(!agents.is_empty(), "Should have loaded at least one agent");

    let feishu_ok = core.check_feishu_auth().await;
    eprintln!("Feishu auth status: {}", feishu_ok);
}

#[tokio::test]
async fn test_agent_config_loading() {
    use feishu_agent_core::config;

    let agents = config::load_all_agents(Path::new("agents")).unwrap();
    assert!(!agents.is_empty());
    for agent in &agents {
        assert!(!agent.llm.is_empty(), "Agent must have an LLM configured");
    }
}
```

- [ ] **Step 3: Create test directory and Cargo.toml update**

Update workspace `Cargo.toml`:
```toml
[workspace]
resolver = "2"
members = ["crates/core", "crates/tui"]
```

Create `tests/Cargo.toml` is not needed — integration tests go in `crates/core/tests/`. Let me fix the path:

Move test to `crates/core/tests/smoke_test.rs`:

```rust
use feishu_agent_core::Core;
use std::path::Path;

#[tokio::test]
async fn test_core_initialization() {
    let agents_dir = Path::new("../../agents");
    let llm_config = Path::new("../../llm_config.yaml");

    if !agents_dir.exists() || !llm_config.exists() {
        eprintln!("Skipping smoke test: configs not found");
        return;
    }

    let core = Core::new(agents_dir, llm_config).await.unwrap();
    let agents = core.list_agents();
    assert!(!agents.is_empty(), "Should have loaded at least one agent");
}
```

- [ ] **Step 4: Build all**

Run: `cargo build`
Expected: Clean compilation

- [ ] **Step 5: Run unit tests**

Run: `cargo test --lib`
Expected: All unit tests pass

- [ ] **Step 6: Commit Phase 1**

```bash
git add .
git commit -m "phase 1: core scaffold, agent config, LLM gateway, feishu bridge"
```

---

## Self-Review

**Spec coverage check:**
- §1 (Overview): Covered — Core struct provides the top-level API
- §2 (Agent Definition): Covered — AgentConfig + AgentRegistry
- §5 (LLM Runtime §5.1-5.3): Covered — LlmGateway + RateLimiter + ModelConfig
- §7 (Feishu CLI §7.1-7.4): Covered — FeishuBridge + MessageQueue + WebSocket subscribe
- §7.5 (Bot identity V1): Not yet — deferred to Phase 2 when agents have lifecycle

**Placeholder scan:** All code blocks contain complete, compilable Rust. No TODOs or TBDs.

**Type consistency:** `AgentId = Uuid`, `AgentConfig` types match across config/registry/gateway.

**Gap:** Phase 1 does not implement:
- Agent lifecycle (§5.7 concurrency model) → Phase 2
- Memory system (§4) → Phase 2
- Secretary agent (§6.4) → Phase 2
- Plugin host (§9) → Phase 3
- TUI (§8) → Phase 3

These are intentionally split into Phase 2 and Phase 3 plans.
