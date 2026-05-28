# LLM Config Full Parity + Feishu Mandatory + Built-in Feishu Skill

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Full alignment of LLM config with OpenCode capabilities (all params + {provider}/{model} format), make feishu mandatory at startup, and auto-release built-in feishu-doc skill to global config.

**Architecture:** Three independent changes to `crates/core/src/`: (1) `config/agent.rs` ModelConfig add all OpenCode params + change model field to `{provider}/{model}` format + provider parsing, (2) `lib.rs` Core::new() validate feishu at startup, (3) `skill/registry.rs` add built-in skill release mechanism + create feishu-doc skill content.

**Tech Stack:** Rust, serde, reqwest, YAML config

**Related files:**
- `crates/core/src/config/agent.rs` — ModelConfig struct
- `crates/core/src/config/mod.rs` — config loading (test YAML)
- `crates/core/src/llm/gateway.rs` — API calls (temperature/top_p etc.)
- `crates/core/src/lib.rs` — Core::new() feishu validation
- `crates/core/src/skill/registry.rs` — built-in skill release
- `crates/tui/src/main.rs` — startup flow (may see feishu error)
- `agents/pm.yaml` — update to new format
- `skills/feishu-doc/SKILL.md` — reference (will be embedded)
- `llm_config.yaml` — update model format

---

### Task 1: ModelConfig — add OpenCode params + {provider}/{model} format

**Files:**
- Modify: `crates/core/src/config/agent.rs`
- Modify: `crates/core/src/config/mod.rs` (test YAML)
- Modify: `agents/pm.yaml`
- Modify: `llm_config.yaml`

- [ ] **Step 1: Rewrite ModelConfig in `config/agent.rs`**

Remove `provider` field, add all OpenCode params. Add `provider()` and `model_name()` helper methods.

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub name: String,
    pub role: String,
    pub personality: Option<String>,
    pub llm: LlmAgentConfig,
    pub triggers: Vec<TriggerConfig>,
    #[serde(default)]
    pub mcps: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmAgentConfig {
    pub primary: ModelConfig,
    pub fallback: Option<ModelConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    /// Format: "{provider}/{model_name}" e.g. "deepseek/deepseek-v4-flash"
    pub model: String,
    /// Environment variable name for API key (e.g. "DEEPSEEK_API_KEY")
    pub api_key_env: Option<String>,
    pub max_tokens: u32,
    /// Fallback model reference (model name from global pool, or full "{provider}/{name}")
    pub fallback: Option<String>,
    /// API timeout in seconds (default: 120)
    pub timeout_secs: Option<u64>,
    pub rate_limit: Option<RateLimitConfig>,
    /// Custom base URL for OpenAI-compatible API
    #[serde(default)]
    pub base_url: Option<String>,
    // === OpenCode-compatible parameters ===
    pub temperature: Option<f64>,
    pub top_p: Option<f64>,
    pub top_k: Option<u32>,
    pub stop: Option<Vec<String>>,
    pub presence_penalty: Option<f64>,
    pub frequency_penalty: Option<f64>,
    /// Anthropic thinking: "low" | "medium" | "high"
    pub reasoning_effort: Option<String>,
    /// DeepSeek thinking mode toggle
    pub thinking: Option<bool>,
    /// Max retries on API failure
    pub max_retries: Option<u32>,
    /// Skip SSL certificate verification
    pub skip_verify_ssl: Option<bool>,
}

impl ModelConfig {
    /// Extract provider name from model field (first part before '/')
    pub fn provider(&self) -> &str {
        self.model.split('/').next().unwrap_or("openai")
    }

    /// Extract model name from model field (second part after '/', or full string)
    pub fn model_name(&self) -> &str {
        self.model.split('/').nth(1).unwrap_or(&self.model)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    pub rpm: u32,
    pub tpm: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerConfig {
    pub pattern: String,
    #[serde(default = "default_auto_respond")]
    pub auto_respond: bool,
}

fn default_auto_respond() -> bool { true }
```

- [ ] **Step 2: Update `pm.yaml` to new format**

```yaml
name: "小红"
role: "你是一个资深产品经理，擅长需求分析和PRD撰写"
personality: "严谨、有条理、善于沟通"
llm:
  primary:
    model: anthropic/claude-sonnet-4-20250514
    api_key_env: ANTHROPIC_API_KEY
    max_tokens: 8192
    timeout_secs: 120
  fallback:
    model: anthropic/claude-haiku-3-5-20241022
    api_key_env: ANTHROPIC_API_KEY
    max_tokens: 4096
    timeout_secs: 60
triggers:
  - pattern: "需求|PRD|产品文档"
    auto_respond: true
  - pattern: "@PM|@小红"
    auto_respond: true
mcps: []
```

- [ ] **Step 3: Update `llm_config.yaml` to new format**

```yaml
models:
  claude-sonnet-4:
    model: anthropic/claude-sonnet-4-20250514
    api_key_env: ANTHROPIC_API_KEY
    max_tokens: 8192
    fallback: claude-haiku-3-5
    timeout_secs: 120
    rate_limit: { rpm: 50, tpm: 100000 }

  ollama-qwen:
    model: ollama/qwen2.5:3b
    max_tokens: 4096
    timeout_secs: 60

  deepseek-v4-pro:
    model: deepseek/deepseek-v4-pro
    api_key_env: DEEPSEEK_API_KEY
    max_tokens: 8192
    timeout_secs: 120
    rate_limit: { rpm: 50, tpm: 200000 }

  deepseek-v4-flash:
    model: deepseek/deepseek-v4-flash
    api_key_env: DEEPSEEK_API_KEY
    max_tokens: 8192
    timeout_secs: 120
    rate_limit: { rpm: 100, tpm: 500000 }
```

- [ ] **Step 4: Update test YAML in `config/mod.rs`**

Change all test YAML from `provider: anthropic` + `model: claude-sonnet-4-20250514` to `model: anthropic/claude-sonnet-4-20250514`.

```rust
#[test]
fn test_load_agent_config() {
    let yaml = r#"
name: "小红"
role: "产品经理"
llm:
  primary:
    model: anthropic/claude-sonnet-4-20250514
    api_key_env: ANTHROPIC_API_KEY
    max_tokens: 8192
triggers:
  - pattern: "需求"
"#;
    let config: agent::AgentConfig = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(config.name, "小红");
    assert_eq!(config.llm.primary.provider(), "anthropic");
}

#[test]
fn test_load_all_agents_from_dir() {
    // same yaml change
    let yaml = br#"name: "test-agent"
role: "test"
llm:
  primary:
    model: anthropic/claude-sonnet-4-20250514
    api_key_env: ANTHROPIC_API_KEY
    max_tokens: 8192
triggers: []"#;
    // ... rest unchanged
}
```

- [ ] **Step 5: Update all provider references in `llm/gateway.rs`**

Change from `model_config.provider.as_str()` to `model_config.provider()`.

In `chat()` method:
```rust
match model_config.provider() {
    "anthropic" => self.call_anthropic(model_config, request).await,
    "ollama" => self.call_ollama(model_config, request).await,
    "deepseek" | "openai" => self.call_openai_compat(model_config, request).await,
    provider => Err(CoreError::Llm(format!("Unsupported provider: {}", provider))),
}
```

In `call_anthropic()` — replace `config.model` with `config.model_name()` in the API body:
```rust
"model": config.model_name(),
```

In `call_ollama()` — replace `config.model` with `config.model_name()`:
```rust
"model": config.model_name(),
```

In `call_openai_compat()` — replace `config.model` with `config.model_name()` and `config.provider.as_str()` with `config.provider()`:
```rust
let api_endpoint = config.base_url.clone().unwrap_or_else(|| {
    match config.provider() {
        "deepseek" => "https://api.deepseek.com/v1/chat/completions".to_string(),
        "openai" => "https://api.openai.com/v1/chat/completions".to_string(),
        _ => "https://api.openai.com/v1/chat/completions".to_string(),
    }
});
```

Also update all `config.provider.as_str()` references.

- [ ] **Step 6: Update `lib.rs` — default_model_config reference**

In `Core::new()`, the `default_model_config` picks the first agent's primary config. No code change needed since ModelConfig structure is the same, just the field changed.

Run: `cargo test --workspace` to verify all pass.

---

### Task 2: Pass LLM params (temperature/top_p/stop etc.) in API calls

**Files:**
- Modify: `crates/core/src/llm/gateway.rs`

- [ ] **Step 1: Pass temperature/top_p/etc in `call_anthropic()`**

In the body construction, add optional params:
```rust
let mut body = serde_json::json!({
    "model": config.model_name(),
    "max_tokens": config.max_tokens,
    "system": request.system_prompt,
    "messages": messages,
});

// Optional params
if let Some(temp) = config.temperature {
    body["temperature"] = serde_json::json!(temp);
}
if let Some(top_p) = config.top_p {
    body["top_p"] = serde_json::json!(top_p);
}
if let Some(top_k) = config.top_k {
    body["top_k"] = serde_json::json!(top_k);
}
if let Some(stop) = &config.stop {
    body["stop_sequences"] = serde_json::json!(stop);
}
if let Some(effort) = &config.reasoning_effort {
    body["thinking"] = serde_json::json!({
        "type": "enabled",
        "budget_tokens": match effort.as_str() {
            "low" => 1024,
            "medium" => 2048,
            "high" => 4096,
            _ => 2048,
        }
    });
}
```

- [ ] **Step 2: Pass params in `call_openai_compat()`**

```rust
let mut body = serde_json::json!({
    "model": config.model_name(),
    "max_tokens": config.max_tokens,
    "messages": all_messages,
});

// Optional params
if let Some(temp) = config.temperature {
    body["temperature"] = serde_json::json!(temp);
}
if let Some(top_p) = config.top_p {
    body["top_p"] = serde_json::json!(top_p);
}
if let Some(top_k) = config.top_k {
    body["top_k"] = serde_json::json!(top_k);
}
if let Some(stop) = &config.stop {
    body["stop"] = serde_json::json!(stop);
}
if let Some(pp) = config.presence_penalty {
    body["presence_penalty"] = serde_json::json!(pp);
}
if let Some(fp) = config.frequency_penalty {
    body["frequency_penalty"] = serde_json::json!(fp);
}
// DeepSeek thinking mode toggle
if let Some(thinking) = config.thinking {
    if thinking {
        // Enable thinking mode for DeepSeek
        // Note: when thinking is enabled, temperature should be 1
        body["temperature"] = serde_json::json!(1);
    }
}
```

- [ ] **Step 3: Handle `skip_verify_ssl` in LlmGateway::new()**

In `LlmGateway::new()`, check if ANY model config has `skip_verify_ssl` set. If so, build client with `danger_accept_invalid_certs(true)`.

```rust
let skip_verify = config.models.values().any(|m| m.skip_verify_ssl.unwrap_or(false));

let client = if skip_verify {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(180))
        .danger_accept_invalid_certs(true)
        .build()
        .unwrap()
} else {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(180))
        .build()
        .unwrap()
};
```

- [ ] **Step 4: Handle `max_retries` in `chat()` method**

Add retry logic around the provider-specific call:
```rust
pub async fn chat(
    &self,
    model_config: &ModelConfig,
    request: &ChatRequest,
) -> Result<ChatResponse, CoreError> {
    if let Some(limiter) = self.rate_limiters.get(&request.model) {
        limiter.acquire().await;
    }

    let max_retries = model_config.max_retries.unwrap_or(0);
    let mut last_error = None;

    for attempt in 0..=max_retries {
        if attempt > 0 {
            tracing::warn!("Retry attempt {}/{} for model {}", attempt, max_retries, model_config.model);
            tokio::time::sleep(std::time::Duration::from_secs(2u64.pow(attempt))).await;
        }

        let result = match model_config.provider() {
            "anthropic" => self.call_anthropic(model_config, request).await,
            "ollama" => self.call_ollama(model_config, request).await,
            "deepseek" | "openai" => self.call_openai_compat(model_config, request).await,
            provider => Err(CoreError::Llm(format!("Unsupported provider: {}", provider))),
        };

        match result {
            Ok(resp) => return Ok(resp),
            Err(e) => {
                last_error = Some(e);
                // Don't retry on auth errors or invalid request
                if let CoreError::Llm(ref msg) = last_error.as_ref().unwrap() {
                    if msg.contains("401") || msg.contains("403") || msg.contains("400") {
                        return Err(last_error.take().unwrap());
                    }
                }
            }
        }
    }

    Err(last_error.unwrap_or_else(|| CoreError::Llm("Max retries exceeded".into())))
}
```

- [ ] **Step 5: Build and test**

Run: `cargo build`
Run: `cargo test --workspace`

---

### Task 3: Feishu mandatory startup validation

**Files:**
- Modify: `crates/core/src/lib.rs`
- Modify: `crates/core/src/feishu/bridge.rs` (static auth check)

- [ ] **Step 1: Add `check_feishu_available()` to `FeishuBridge` or as a standalone function**

In `bridge.rs`, change `check_auth` from instance method to also support static usage, or add a standalone function.

Actually, `check_auth` is already an instance method on `FeishuBridge`. We need to either:
a) Make it a static/associated function, or
b) Create the bridge first, then check

Option b is simpler and already works with the existing code. The bridge creation is lightweight (just creates a SendQueue).

- [ ] **Step 2: Update `Core::new()` in `lib.rs`**

Replace the current feishu_chat_id handling and add validation:

```rust
// === Feishu mandatory validation ===

// 1. Check FEISHU_CHAT_ID env var
let feishu_chat_id = std::env::var("FEISHU_CHAT_ID")
    .map_err(|_| CoreError::Config(
        "FEISHU_CHAT_ID environment variable must be set. \
         Get it from your Feishu group chat settings.".into()
    ))?;
tracing::info!("Feishu chat_id: {}", feishu_chat_id);

// 2. Check lark-cli is available
let lark_check = tokio::process::Command::new("lark-cli")
    .arg("--version")
    .output()
    .await
    .map_err(|e| CoreError::Config(format!(
        "lark-cli not found in PATH. Feishu CLI is required. Error: {}", e
    )))?;

if !lark_check.status.success() {
    let stderr = String::from_utf8_lossy(&lark_check.stderr);
    return Err(CoreError::Config(format!(
        "lark-cli check failed: {}", stderr
    )));
}
let version = String::from_utf8_lossy(&lark_check.stdout).trim().to_string();
tracing::info!("lark-cli available: {}", version);

// 3. Check Feishu auth
let feishu_bridge = feishu::bridge::FeishuBridge::new();
let auth_ok = feishu_bridge.check_auth().await
    .map_err(|e| CoreError::Config(format!("Feishu auth check failed: {}", e)))?;
if !auth_ok {
    return Err(CoreError::Config(
        "Feishu not authenticated. Run 'lark-cli login' first.".into()
    ));
}
tracing::info!("Feishu auth: OK");
```

The existing code below should be updated to remove the old `unwrap_or_default()`:
```
// REMOVE this old code:
// let feishu_chat_id = std::env::var("FEISHU_CHAT_ID").unwrap_or_default();
// tracing::info!("Feishu chat_id: {}", if feishu_chat_id.is_empty() { "(not set)" } else { &feishu_chat_id });
```

- [ ] **Step 3: Build and test**

Run: `cargo build`
Note: Tests that create Core will fail locally without FEISHU_CHAT_ID and lark-cli. Consider whether to handle this.

Actually, looking at the tests — the existing `config/mod.rs` tests don't create `Core`, they test config parsing directly. The TUI tests (if any) might be affected, but the current tests in `config/mod.rs` are unit tests that don't create `Core`.

Run: `cargo test --workspace`

---

### Task 4: Built-in feishu-doc skill auto-release

**Files:**
- Modify: `crates/core/src/skill/registry.rs`
- Create: (embedded content, no new file)

- [ ] **Step 1: Add `release_builtin_skills()` to `skill/registry.rs`**

```rust
/// Define built-in skills content (embedded at compile time)
const BUILT_IN_SKILLS: &[(&str, &str, &str)] = &[
    (
        "feishu-doc",
        "Create and manage Feishu documents",
        r#"# Feishu Doc Skill

## Instructions
You can create and edit Feishu documents. When asked to write documentation:

1. Create a new Feishu doc using `lark-cli doc +create --title <title>`
2. Write content using Feishu markdown format
3. Share the doc link in your response

## Commands

- `lark-cli doc +create --title "<title>"` — Create a new document
- `lark-cli doc +get --doc-token <token>` — Get document content
- `lark-cli doc +update --doc-token <token> --text "<content>"` — Update document
- `lark-cli doc +search --query "<query>"` — Search documents

## Best Practices

1. Always use a clear, descriptive title for new documents
2. Use Feishu markdown format for rich content (headings, lists, tables)
3. After creating a document, share the doc link with the user
4. For long documents, create multiple sections with clear headings
5. Review document content before sharing
"#,
    ),
];

/// Release built-in skills to the global skills directory if they don't exist.
/// Called once at startup.
pub fn release_builtin_skills() -> Result<(), CoreError> {
    let global_dir = global_skills_dir();
    
    for (name, description, instructions) in BUILT_IN_SKILLS {
        let skill_dir = global_dir.join(name);
        let skill_file = skill_dir.join("SKILL.md");
        
        if skill_file.exists() {
            tracing::debug!("Built-in skill already exists, skipping: {name}");
            continue;
        }
        
        std::fs::create_dir_all(&skill_dir)?;
        
        let frontmatter = format!(
            "---\nname: {name}\ndescription: {description}\n---\n\n"
        );
        let content = format!("{frontmatter}{instructions}");
        std::fs::write(&skill_file, &content)?;
        
        tracing::info!("Released built-in skill: {name} -> {:?}", skill_file);
    }
    
    Ok(())
}
```

- [ ] **Step 2: Call `release_builtin_skills()` in `Core::new()`**

In `lib.rs`, right before the global skills discovery:
```rust
// Release built-in skills to global config directory
skill::registry::release_builtin_skills()?;

// Discover global skills
let global_skills_dir = skill::registry::global_skills_dir();
let mut skill_registry = skill::registry::SkillRegistry::discover(&global_skills_dir)?;
```

- [ ] **Step 3: Build and test**

Run: `cargo build`
Run: `cargo test --workspace`