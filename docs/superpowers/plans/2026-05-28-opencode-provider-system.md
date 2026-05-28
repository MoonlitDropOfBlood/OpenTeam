# OpenCode Provider System â€?Full Replication

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Full replication of OpenCode's provider configuration system â€?built-in model definitions with defaults (cost, context window, capabilities), user-overridable provider config in YAML, model resolution chain, and all config fields wired into actual LLM API requests.

**Architecture:** Three-layer resolution: (1) built-in Rust constants define default model metadata (baseURL, apiKey env, context window, costs), (2) user `provider:` YAML config overrides per-provider options (timeout, headers, apiKey, baseURL) and per-model overrides (cost, limit, headers), (3) agent `ModelConfig` overrides final values. All resolved parameters are passed to the actual API calls â€?no empty methods, no TODO stubs.

**Tech Stack:** Rust, serde, serde_yaml, reqwest

---

## File Structure

```
crates/core/src/
â”œâ”€â”€ config/
â”?  â”œâ”€â”€ agent.rs          â€?ModelConfig (simplified, model: "provider/name"), resolve() method
â”?  â”œâ”€â”€ llm.rs            â€?LlmConfig + NEW ProviderConfig, ProviderOptions, ProviderModelConfig
â”?  â”œâ”€â”€ mod.rs            â€?load_llm_config() updated, load_provider_config() added
â”?  â””â”€â”€ provider.rs       â€?NEW: ProviderRegistry, provider config YAML parsing
â”œâ”€â”€ llm/
â”?  â”œâ”€â”€ mod.rs            â€?re-exports
â”?  â”œâ”€â”€ models.rs         â€?NEW: ModelDefinition, ModelLimits, ModelCost, built-in model constants
â”?  â”œâ”€â”€ provider.rs       â€?NEW: ProviderResolver, resolution chain logic
â”?  â”œâ”€â”€ gateway.rs        â€?use ResolvedModel for all API calls
â”?  â””â”€â”€ rate_limiter.rs   â€?unchanged
```

---

### Task 1: ModelDefinition + Built-in Model Constants (Static Storage)

**Files:**
- Create: `crates/core/src/llm/models.rs`
- Modify: `crates/core/src/llm/mod.rs` (add `pub mod models;`)

- [ ] **Step 1: Define core types in `models.rs`**

```rust
use serde::{Deserialize, Serialize};

/// Token limits for a model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelLimits {
    pub context: u32,    // max input + output context window
    pub input: u32,      // max input tokens (0 = same as context)
    pub output: u32,     // max output tokens (0 = use default_max_tokens)
}

impl Default for ModelLimits {
    fn default() -> Self {
        Self { context: 128000, input: 128000, output: 8192 }
    }
}

/// Token cost per 1M tokens (USD)
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ModelCost {
    pub input: f64,
    pub output: f64,
    pub cache_read: f64,
    pub cache_write: f64,
}

impl Default for ModelCost {
    fn default() -> Self {
        Self { input: 0.0, output: 0.0, cache_read: 0.0, cache_write: 0.0 }
    }
}

/// Model capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCapabilities {
    pub can_reason: bool,
    pub supports_attachments: bool,
    pub supports_tools: bool,
    pub supports_temperature: bool,
}

impl Default for ModelCapabilities {
    fn default() -> Self {
        Self {
            can_reason: false,
            supports_attachments: false,
            supports_tools: true,
            supports_temperature: true,
        }
    }
}

/// Built-in provider metadata (default endpoint and API key env var)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderDefaults {
    pub name: String,
    pub base_url: String,
    pub api_key_env: String,
    pub env_vars: Vec<String>,   // auto-detect env vars (e.g. ["ANTHROPIC_API_KEY"])
    pub timeout_ms: u32,         // default request timeout in ms
}

/// A fully resolved model definition (built-in defaults + provider config overrides)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelDefinition {
    pub id: String,                  // e.g. "claude-sonnet-4-20250514"
    pub name: String,                // display name e.g. "Claude Sonnet 4"
    pub provider: String,            // e.g. "anthropic"
    pub api_model: String,           // actual API model string e.g. "claude-sonnet-4-20250514"
    pub cost: ModelCost,
    pub limits: ModelLimits,
    pub capabilities: ModelCapabilities,
    pub default_max_tokens: u32,
    pub headers: Option<Vec<(String, String)>>,  // per-model custom headers
}
```

- [ ] **Step 2: Define built-in model definitions**

The full list of built-in model definitions:

**Anthropic models:**
- `claude-sonnet-4-20250514` (Claude Sonnet 4): $3/$15 per 1M in/out, 200K context, reasoning=true, attachments=true, default_max=50000
- `claude-opus-4-20250514` (Claude Opus 4): $15/$75, 200K, reasoning=true, attachments=true, default_max=4096
- `claude-3-7-sonnet-latest` (Claude 3.7 Sonnet): $3/$15, 200K, reasoning=true, attachments=true, default_max=50000
- `claude-3-5-sonnet-latest` (Claude 3.5 Sonnet): $3/$15, 200K, attachments=true, default_max=5000
- `claude-3-5-haiku-latest` (Claude 3.5 Haiku): $0.80/$4.0, 200K, attachments=true, default_max=4096
- `claude-3-haiku-20240307` (Claude 3 Haiku): $0.25/$1.25, 200K, attachments=true, default_max=4096
- `claude-3-opus-latest` (Claude 3 Opus): $15/$75, 200K, attachments=true, default_max=4096

**DeepSeek models:**
- `deepseek-v4-pro` (DeepSeek V4 Pro): $2/$8, 64K, reasoning=true, attachments=false, default_max=8192
- `deepseek-v4-flash` (DeepSeek V4 Flash): $0.50/$2, 64K, reasoning=true, attachments=false, default_max=8192
- `deepseek-chat` (DeepSeek Chat): $0.14/$0.28, 64K, reasoning=true, attachments=false, default_max=8192
- `deepseek-reasoner` (DeepSeek Reasoner): $0.55/$2.19, 64K, reasoning=true, attachments=false, default_max=8192

**OpenAI models:**
- `gpt-4o` (GPT 4o): $2.50/$10, 128K, attachments=true, default_max=4096
- `gpt-4o-mini` (GPT 4o Mini): $0.15/$0.60, 128K, attachments=true, default_max=4096
- `gpt-4.1` (GPT 4.1): $2/$8, 1M context, attachments=true, default_max=20000
- `gpt-4.1-mini` (GPT 4.1 mini): $0.40/$1.60, 200K, attachments=true, default_max=20000
- `gpt-4.1-nano` (GPT 4.1 nano): $0.10/$0.40, 1M, attachments=true, default_max=20000
- `o3` (o3): $10/$40, 200K, reasoning=true, attachments=true, default_max=50000
- `o3-mini` (o3 mini): $1.10/$4.40, 200K, reasoning=true, attachments=false, default_max=50000
- `o4-mini` (o4 mini): $1.10/$4.40, 128K, reasoning=true, attachments=true, default_max=50000
- `o1` (o1): $15/$60, 200K, reasoning=true, attachments=true, default_max=50000

**Ollama (generic fallback):**
- `*` (Ollama generic): $0/$0, 128K context, reasoning=false, attachments=false, default_max=4096

- [ ] **Step 3: Add `resolve_builtin_model()` function â€?use static storage**

```rust
use std::sync::LazyLock;

static BUILTIN_MODELS: LazyLock<Vec<ModelDefinition>> = LazyLock::new(|| builtin_models_inner());

/// Internal helper: builds the Vec of model definitions
fn builtin_models_inner() -> Vec<ModelDefinition> {
    vec![
        // === Anthropic ===
        ModelDefinition { ... }, // full list below
        // === DeepSeek ===
        ModelDefinition { ... },
        // === OpenAI ===
        ModelDefinition { ... },
        // === Ollama === (generic fallback)
        ModelDefinition { ... },
    ]
}

/// Look up a model definition from built-in defaults.
/// Uses LazyLock for static storage â€?no allocation per call.
pub fn resolve_builtin_model(provider: &str, model_name: &str) -> Option<&'static ModelDefinition> {
    BUILTIN_MODELS
        .iter()
        .find(|m| m.provider == provider && m.id == model_name)
}

/// Get all built-in provider defaults (uses LazyLock for static storage)
static BUILTIN_PROVIDER_DEFAULTS: LazyLock<Vec<ProviderDefaults>> = LazyLock::new(|| {
    vec![
        ProviderDefaults {
            name: "anthropic".into(),
            base_url: "https://api.anthropic.com/v1".into(),
            api_key_env: "ANTHROPIC_API_KEY".into(),
            env_vars: vec!["ANTHROPIC_API_KEY".into()],
            timeout_ms: 300000,
        },
        ProviderDefaults {
            name: "deepseek".into(),
            base_url: "https://api.deepseek.com/v1".into(),
            api_key_env: "DEEPSEEK_API_KEY".into(),
            env_vars: vec!["DEEPSEEK_API_KEY".into()],
            timeout_ms: 300000,
        },
        ProviderDefaults {
            name: "openai".into(),
            base_url: "https://api.openai.com/v1".into(),
            api_key_env: "OPENAI_API_KEY".into(),
            env_vars: vec!["OPENAI_API_KEY".into()],
            timeout_ms: 300000,
        },
        ProviderDefaults {
            name: "ollama".into(),
            // Ollama native API: http://localhost:11434/api/chat
            // Ollama also supports OpenAI-compat at /v1/chat/completions
            // Using native endpoint for backward compatibility
            base_url: "http://localhost:11434/api".into(),
            api_key_env: String::new(),
            env_vars: vec![],
            timeout_ms: 60000,
        },
    ]
});

pub fn builtin_provider_defaults() -> &'static [ProviderDefaults] {
    &BUILTIN_PROVIDER_DEFAULTS
}
```

- [ ] **Step 5: Add `pub mod models;` to `llm/mod.rs`**

- [ ] **Step 5: Build and test compilation**

Run: `cargo check -p feishu-agent-core`

---

### Task 2: ProviderConfig â€?YAML config structs

**Files:**
- Create: `crates/core/src/config/provider.rs`
- Modify: `crates/core/src/config/llm.rs` (restructure LlmConfig)
- Modify: `crates/core/src/config/mod.rs` (add `pub mod provider;`)

- [ ] **Step 1: Create `config/provider.rs`**

```rust
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Provider configuration â€?mirrors OpenCode's ProviderConfig schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    /// Display name for the provider
    pub name: Option<String>,
    /// AI SDK npm package (informational, not used in Rust)
    pub npm: Option<String>,
    /// Environment variable names for auto-detecting API keys
    #[serde(default)]
    pub env: Vec<String>,
    /// Only allow these models (whitelist)
    pub whitelist: Option<Vec<String>>,
    /// Block these models (blacklist)
    pub blacklist: Option<Vec<String>>,
    /// Provider-level options
    #[serde(default)]
    pub options: ProviderOptions,
    /// Per-model configuration overrides
    #[serde(default)]
    pub models: HashMap<String, ProviderModelConfig>,
}

/// Provider-level options â€?all fields are optional, defaults come from built-in
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderOptions {
    /// API key value or "{env:VAR}" reference
    pub api_key: Option<String>,
    /// Custom base URL
    pub base_url: Option<String>,
    /// Request timeout in milliseconds (set to 0 for no timeout)
    pub timeout: Option<u32>,
    /// SSE chunk timeout in milliseconds
    pub chunk_timeout: Option<u32>,
    /// Enable prompt caching
    pub set_cache_key: Option<bool>,
    /// Custom HTTP headers
    #[serde(default)]
    pub headers: HashMap<String, String>,
    /// GitHub Enterprise URL (for copilot provider). Reserved for future use.
    #[allow(dead_code)]
    pub enterprise_url: Option<String>,
}

impl Default for ProviderOptions {
    fn default() -> Self {
        Self {
            api_key: None,
            base_url: None,
            timeout: None,
            chunk_timeout: None,
            set_cache_key: None,
            headers: HashMap::new(),
            enterprise_url: None,
        }
    }
}

/// Per-model configuration overrides
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderModelConfig {
    /// Override model ID sent to API
    pub id: Option<String>,
    /// Display name
    pub name: Option<String>,
    /// Token cost overrides
    pub cost: Option<super::llm::ModelCostConfig>,
    /// Token limit overrides
    pub limit: Option<super::llm::ModelLimitConfig>,
    /// Per-model options (overrides provider-level options)
    #[serde(default)]
    pub options: HashMap<String, String>,
    /// Per-model custom headers
    #[serde(default)]
    pub headers: HashMap<String, String>,
}
```

- [ ] **Step 2: Restructure `config/llm.rs` to include provider config**

```rust
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    /// Provider configurations (keyed by provider ID)
    #[serde(default)]
    pub provider: HashMap<String, super::provider::ProviderConfig>,
    /// Legacy model pool (kept for backward compatibility, but provider is preferred)
    #[serde(default)]
    pub models: HashMap<String, super::agent::ModelConfig>,
}

/// Model cost config in YAML (per 1M tokens)
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ModelCostConfig {
    pub input: Option<f64>,
    pub output: Option<f64>,
    #[serde(default)]
    pub cache_read: Option<f64>,
    #[serde(default)]
    pub cache_write: Option<f64>,
}

/// Model limit config in YAML
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelLimitConfig {
    pub context: Option<u32>,
    pub input: Option<u32>,
    pub output: Option<u32>,
}
```

- [ ] **Step 3: Add `pub mod provider;` to `config/mod.rs`**

- [ ] **Step 4: Build and test**

Run: `cargo check -p feishu-agent-core`

---

### Task 3: ProviderResolver â€?Resolution Chain Logic

**Files:**
- Create: `crates/core/src/llm/provider.rs`
- Modify: `crates/core/src/llm/mod.rs` (add `pub mod provider;`)

- [ ] **Step 1: Define `ResolvedModel` â€?the fully resolved result**

```rust
use crate::config::agent::ModelConfig;
use crate::config::provider::{ProviderConfig, ProviderOptions};
use crate::llm::models::{ModelDefinition, ModelLimits, ModelCost, ModelCapabilities, ProviderDefaults};
use std::collections::HashMap;

/// A fully resolved model with all configuration merged:
/// built-in defaults â†?provider config â†?agent overrides
/// NOTE: All fields are FLAT â€?no nested model_config reference.
/// Use the ResolvedModel fields directly in API calls.
#[derive(Debug, Clone)]
pub struct ResolvedModel {
    /// Provider ID (e.g. "anthropic")
    pub provider: String,
    /// Model name (e.g. "claude-sonnet-4-20250514")
    pub model_name: String,
    /// Actual API model string to send
    pub api_model: String,
    /// Base URL for API calls
    pub base_url: String,
    /// API key env var name (or direct key value)
    pub api_key: String,
    /// Whether the api_key is a direct value vs env var name
    pub api_key_is_direct: bool,
    /// Max output tokens
    pub max_tokens: u32,
    /// Request timeout in seconds
    pub timeout_secs: u64,
    /// SSE chunk timeout in seconds
    pub chunk_timeout_secs: u64,
    /// Enable prompt caching
    pub set_cache_key: bool,
    /// Custom HTTP headers
    pub headers: Vec<(String, String)>,
    /// Model limits
    pub limits: ModelLimits,
    /// Model costs
    pub cost: ModelCost,
    /// Model capabilities
    pub capabilities: ModelCapabilities,
    /// Can reason (enable thinking)
    pub can_reason: bool,
    /// Default max tokens from model definition
    pub default_max_tokens: u32,
    /// Skip SSL verification
    pub skip_verify_ssl: bool,
    // === Agent-level optional params (flat on ResolvedModel) ===
    pub temperature: Option<f64>,
    pub top_p: Option<f64>,
    pub top_k: Option<u32>,
    pub stop: Option<Vec<String>>,
    pub presence_penalty: Option<f64>,
    pub frequency_penalty: Option<f64>,
    pub reasoning_effort: Option<String>,
    pub thinking: Option<bool>,
    pub max_retries: Option<u32>,
    /// Unique key for rate limiting â€?"{provider}/{model_name}"
    pub rate_limiter_key: String,
}
```

- [ ] **Step 2: Create `ProviderResolver` with resolution logic**

```rust
#[derive(Debug, Clone)]
pub struct ProviderResolver {
    provider_configs: HashMap<String, ProviderConfig>,
}

impl ProviderResolver {
    pub fn new(provider_configs: HashMap<String, ProviderConfig>) -> Self {
        Self {
            provider_configs,
        }
    }

    /// Resolve a ModelConfig into a fully resolved ResolvedModel.
    /// Resolution priority (highest wins):
    ///   1. Agent ModelConfig explicit fields
    ///   2. User provider config (from YAML)
    ///   3. Built-in defaults (from Rust code)
    ///   4. Generic fallback
    pub fn resolve(&self, config: &ModelConfig) -> ResolvedModel {
        let provider = config.provider();
        let model_name = config.model_name();

        // Layer 3: Built-in defaults
        let builtin_model = models::resolve_builtin_model(provider, model_name);
        let provider_default = models::builtin_provider_defaults().iter().find(|p| p.name == provider);

        // Layer 2: User provider config
        let user_provider = self.provider_configs.get(provider);
        let user_model = user_provider.and_then(|p| p.models.get(model_name));

        // Resolve: built-in defaults â†?user provider config â†?agent overrides
        //
        // base_url:
        //   agent config.base_url â†?user provider options.base_url â†?builtin default base_url
        let base_url = config.base_url.clone()
            .or_else(|| user_provider.and_then(|p| p.options.base_url.clone()))
            .or_else(|| provider_default.map(|p| p.base_url.clone()))
            .unwrap_or_else(|| format!("https://api.{}.com/v1", provider));

        // api_key_env:
        //   agent config.api_key_env â†?user provider options.api_key (direct) â†?builtin default env
        let (api_key, api_key_is_direct) = self.resolve_api_key(config, user_provider, provider_default);

        // max_tokens:
        //   agent config.max_tokens â†?builtin model default_max_tokens â†?4096
        let max_tokens = config.max_tokens.max(
            builtin_model.map(|m| m.default_max_tokens).unwrap_or(4096)
        );
        // Actually, agent config.max_tokens should take priority but also serve as a cap.
        // Let me reconsider: the agent sets max_tokens directly on ModelConfig,
        // and it's already mandatory. So just use config.max_tokens.
        // But wait â€?the agent config should simplify to NOT require max_tokens.
        // Actually looking at the current code, max_tokens is required (not Option).
        // For now, keep it required and use it directly.

        // timeout_secs:
        //   agent config.timeout_secs â†?user provider options.timeout (msâ†’s) â†?builtin default(msâ†’s)
        let timeout_secs = config.timeout_secs
            .or_else(|| user_provider
                .and_then(|p| p.options.timeout)
                .map(|ms| (ms / 1000).max(1) as u64)
            )
            .or_else(|| provider_default
                .map(|p| (p.timeout_ms / 1000).max(1) as u64)
            )
            .unwrap_or(300);

        // headers: merged user provider headers + user model headers
        let mut headers: Vec<(String, String)> = Vec::new();
        if let Some(up) = user_provider {
            for (k, v) in &up.options.headers {
                headers.push((k.clone(), v.clone()));
            }
            if let Some(um) = user_model {
                for (k, v) in &um.headers {
                    headers.push((k.clone(), v.clone()));
                }
            }
        }
        // Also merge builtin model headers
        if let Some(bm) = builtin_model {
            if let Some(bh) = &bm.headers {
                for (k, v) in bh {
                    // Don't override user-set headers
                    if !headers.iter().any(|(hk, _)| hk == k) {
                        headers.push((k.clone(), v.clone()));
                    }
                }
            }
        }

        // capabilities from builtin model
        let capabilities = builtin_model
            .map(|m| m.capabilities.clone())
            .unwrap_or_default();

        // limits from builtin model
        let limits = builtin_model
            .map(|m| m.limits.clone())
            .unwrap_or_default();

        // cost from builtin model (overridable by user)
        let cost = if let Some(uc) = user_model.and_then(|m| m.cost) {
            ModelCost {
                input: uc.input.unwrap_or_else(|| builtin_model.map(|m| m.cost.input).unwrap_or(0.0)),
                output: uc.output.unwrap_or_else(|| builtin_model.map(|m| m.cost.output).unwrap_or(0.0)),
                cache_read: uc.cache_read.unwrap_or_else(|| builtin_model.map(|m| m.cost.cache_read).unwrap_or(0.0)),
                cache_write: uc.cache_write.unwrap_or_else(|| builtin_model.map(|m| m.cost.cache_write).unwrap_or(0.0)),
            }
        } else {
            builtin_model.map(|m| m.cost).unwrap_or_default()
        };

        // set_cache_key
        let set_cache_key = user_provider
            .and_then(|p| p.options.set_cache_key)
            .unwrap_or(false);

        // chunk_timeout
        let chunk_timeout_secs = user_provider
            .and_then(|p| p.options.chunk_timeout)
            .map(|ms| (ms / 1000).max(1) as u64)
            .unwrap_or(30);

        let api_model = user_model
            .and_then(|m| m.id.clone())
            .or_else(|| builtin_model.map(|m| m.api_model.clone()))
            .unwrap_or_else(|| model_name.to_string());

        let default_max_tokens = builtin_model.map(|m| m.default_max_tokens).unwrap_or(4096);

        // Flat params from ModelConfig
        let temperature = config.temperature;
        let top_p = config.top_p;
        let top_k = config.top_k;
        let stop = config.stop.clone();
        let presence_penalty = config.presence_penalty;
        let frequency_penalty = config.frequency_penalty;
        let reasoning_effort = config.reasoning_effort.clone();
        let thinking = config.thinking;
        let max_retries = config.max_retries;
        let skip_verify_ssl = config.skip_verify_ssl.unwrap_or(false);

        // Unique key for rate limiting
        let rate_limiter_key = format!("{}/{}", provider, model_name);

        ResolvedModel {
            provider: provider.to_string(),
            model_name: model_name.to_string(),
            api_model,
            base_url,
            api_key,
            api_key_is_direct,
            max_tokens: config.max_tokens,
            timeout_secs,
            chunk_timeout_secs,
            set_cache_key,
            headers,
            limits,
            cost,
            capabilities,
            can_reason: capabilities.can_reason,
            default_max_tokens,
            skip_verify_ssl,
            temperature,
            top_p,
            top_k,
            stop,
            presence_penalty,
            frequency_penalty,
            reasoning_effort,
            thinking,
            max_retries,
            rate_limiter_key,
        }
    }

    fn resolve_api_key(
        &self,
        config: &ModelConfig,
        user_provider: Option<&ProviderConfig>,
        provider_default: Option<&ProviderDefaults>,
    ) -> (String, bool) {
        // Priority 1: Agent config's api_key_env
        if let Some(env) = &config.api_key_env {
            return (env.clone(), false);
        }

        // Priority 2: User provider config's options.api_key (direct value)
        if let Some(up) = user_provider {
            if let Some(api_key) = &up.options.api_key {
                // Check if it's an {env:VAR} reference
                if api_key.starts_with("{env:") && api_key.ends_with("}") {
                    let env_var = &api_key[5..api_key.len()-1];
                    return (env_var.to_string(), false);
                }
                // Direct API key value
                return (api_key.clone(), true);
            }
        }

        // Priority 3: Built-in provider default env var
        if let Some(pd) = provider_default {
            if !pd.api_key_env.is_empty() {
                return (pd.api_key_env.clone(), false);
            }
        }

        ("OPENAI_API_KEY".to_string(), false)
    }
}
```

- [ ] **Step 3: Add `pub mod provider;` to `llm/mod.rs`** (separate from `config::provider`)

- [ ] **Step 4: Build and test**

Run: `cargo check -p feishu-agent-core`

---

### Task 4: Integrate Resolution Chain into Core Startup

**Files:**
- Modify: `crates/core/src/lib.rs` (Core::new() â€?build ProviderResolver)
- Modify: `crates/core/src/llm/gateway.rs` (LlmGateway â€?use ResolvedModel)

- [ ] **Step 1: Add ProviderResolver to Core struct**

In `lib.rs`, add `provider_resolver` field to `Core`:
```rust
pub struct Core {
    // ... existing fields
    pub provider_resolver: llm::provider::ProviderResolver,
}
```

In `Core::new()`, after loading `llm_config`:
```rust
let provider_resolver = llm::provider::ProviderResolver::new(llm_config.provider.clone());
```

Add to `Ok(Self { ... })`:
```rust
provider_resolver,
```

- [ ] **Step 2: Update LlmGateway to store ProviderResolver + re-key rate limiters**

```rust
pub struct LlmGateway {
    client: reqwest::Client,
    models: HashMap<String, ModelConfig>,
    rate_limiters: HashMap<String, RateLimiter>,
    provider_resolver: crate::llm::provider::ProviderResolver,
}
```

Update `LlmGateway::new()` to accept `provider_resolver` and re-key rate limiters using `{provider}/{model_name}` format:

```rust
pub fn new(config: LlmConfig, provider_resolver: crate::llm::provider::ProviderResolver) -> Self {
    let skip_verify = config.models.values().any(|m| m.skip_verify_ssl.unwrap_or(false));

    let mut client_builder = reqwest::Client::builder()
        .timeout(Duration::from_secs(180));
    if skip_verify {
        client_builder = client_builder.danger_accept_invalid_certs(true);
    }
    let client = client_builder.build().unwrap();

    // Initialize rate limiters using {provider}/{model_name} keys
    // (matches the rate_limiter_key in ResolvedModel)
    let mut rate_limiters = HashMap::new();
    for (_, model_config) in &config.models {
        if let Some(rate) = &model_config.rate_limit {
            let key = format!("{}/{}", model_config.provider(), model_config.model_name());
            rate_limiters.insert(key, RateLimiter::new(rate.rpm));
        }
    }

    Self {
        client,
        models: config.models,
        rate_limiters,
        provider_resolver,
    }
}

/// Resolve a ModelConfig into a ResolvedModel using the provider registry
pub fn resolve_model(&self, config: &ModelConfig) -> crate::llm::provider::ResolvedModel {
    self.provider_resolver.resolve(config)
}
```

- [ ] **Step 3: Update Gateway API calls to use ResolvedModel â€?use flat fields**

In `chat()`, resolve the model first. Use `resolved.rate_limiter_key` for rate limiter lookup:

```rust
pub async fn chat(
    &self,
    model_config: &ModelConfig,
    request: &ChatRequest,
) -> Result<ChatResponse, CoreError> {
    let resolved = self.resolve_model(model_config);

    // Rate limit using resolved rate_limiter_key (format: "{provider}/{model_name}")
    if let Some(limiter) = self.rate_limiters.get(&resolved.rate_limiter_key) {
        limiter.acquire().await;
    }

    // Use resolved values
    let api_key = if resolved.api_key_is_direct {
        resolved.api_key.clone()
    } else {
        std::env::var(&resolved.api_key)
            .map_err(|_| CoreError::Llm(format!("{} not set", resolved.api_key)))?
    };

    // Pass resolved to provider-specific methods
    match resolved.provider.as_str() {
        "anthropic" => self.call_anthropic_resolved(&resolved, &api_key, request).await,
        "ollama" => self.call_ollama_resolved(&resolved, &api_key, request).await,
        "deepseek" | "openai" => self.call_openai_compat_resolved(&resolved, &api_key, request).await,
        provider => Err(CoreError::Llm(format!("Unsupported provider: {}", provider))),
    }
}
```

- [ ] **Step 4: Rewrite `call_anthropic()` to use ResolvedModel**

New method signature:
```rust
async fn call_anthropic_resolved(
    &self,
    resolved: &crate::llm::provider::ResolvedModel,
    api_key: &str,
    request: &ChatRequest,
) -> Result<ChatResponse, CoreError>
```

Changes from current `call_anthropic`:
- Use `resolved.api_model` instead of `config.model_name()`
- Use `resolved.base_url` for the endpoint URL (currently hardcoded to `https://api.anthropic.com/v1/messages`)
- Use `resolved.timeout_secs` for timeout
- Use `resolved.headers` for additional request headers
- Use `resolved.max_tokens` for `max_tokens`
- Pass `set_cache_key` â†?enable Anthropic cache control
- Pass `can_reason` â†?enable thinking mode
- Wire ALL the OpenCode params from `ResolvedModel` (flat fields: temperature, top_p, top_k, stop, etc.)

```rust
async fn call_anthropic_resolved(
    &self,
    resolved: &crate::llm::provider::ResolvedModel,
    api_key: &str,
    request: &ChatRequest,
) -> Result<ChatResponse, CoreError> {
    let endpoint = format!("{}/messages", resolved.base_url.trim_end_matches('/'));

    let mut body = serde_json::json!({
        "model": resolved.api_model,
        "max_tokens": resolved.max_tokens,
        "system": request.system_prompt,
        "messages": request.messages.iter().map(|m| {
            // ... same as current call_anthropic
        }).collect::<Vec<_>>(),
    });

    // Optional params from resolved model (flat fields)
    if let Some(temp) = resolved.temperature {
        body["temperature"] = serde_json::json!(temp);
    }
    if let Some(top_p) = resolved.top_p {
        body["top_p"] = serde_json::json!(top_p);
    }
    if let Some(top_k) = resolved.top_k {
        body["top_k"] = serde_json::json!(top_k);
    }
    if let Some(stop) = &resolved.stop {
        body["stop_sequences"] = serde_json::json!(stop);
    }

    // Reasoning/thinking from resolved capabilities
    if resolved.can_reason {
        if let Some(effort) = &resolved.reasoning_effort {
            let budget_tokens = match effort.as_str() {
                "low" => (resolved.max_tokens as f64 * 0.5) as u64,
                "medium" => (resolved.max_tokens as f64 * 0.7) as u64,
                "high" => (resolved.max_tokens as f64 * 0.9) as u64,
                _ => (resolved.max_tokens as f64 * 0.7) as u64,
            };
            body["thinking"] = serde_json::json!({
                "type": "enabled",
                "budget_tokens": budget_tokens,
            });
            body["temperature"] = serde_json::json!(1);
        }
    }

    // Prompt caching
    if resolved.set_cache_key && request.system_prompt.len() > 1000 {
        // Anthropic prompt caching: add cache control to system prompt
        if let Some(sys) = body["system"].as_str() {
            body["system"] = serde_json::json!([
                {"type": "text", "text": sys, "cache_control": {"type": "ephemeral"}}
            ]);
        }
    }

    // Tools
    if !request.tools.is_empty() {
        let tools: Vec<serde_json::Value> = request.tools.iter().map(|t| {
            serde_json::json!({
                "name": t.name,
                "description": t.description,
                "input_schema": t.input_schema,
            })
        }).collect();
        body["tools"] = serde_json::json!(tools);
    }

    let timeout = std::time::Duration::from_secs(resolved.timeout_secs);
    let mut req = self.client
        .post(&endpoint)
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01");

    // Add custom headers from provider/model config
    for (key, value) in &resolved.headers {
        req = req.header(key.as_str(), value.as_str());
    }

    let response = tokio::time::timeout(
        timeout,
        req.json(&body).send(),
    )
    .await
    .map_err(|e| CoreError::Llm(format!("Timeout or request error: {}", e)))??;

    // ... rest of response parsing same as current
}
```

- [ ] **Step 5: Rewrite `call_openai_compat_resolved()` to use ResolvedModel**

```rust
async fn call_openai_compat_resolved(
    &self,
    resolved: &crate::llm::provider::ResolvedModel,
    api_key: &str,
    request: &ChatRequest,
) -> Result<ChatResponse, CoreError> {
    let endpoint = format!("{}/chat/completions", resolved.base_url.trim_end_matches('/'));

    let mut all_messages: Vec<serde_json::Value> = vec![
        serde_json::json!({"role": "system", "content": request.system_prompt}),
    ];
    all_messages.extend(request.messages.iter().map(|m| {
        serde_json::json!({"role": m.role, "content": m.content})
    }));

    // Preserve reasoning_content from previous turns (DeepSeek thinking mode)
    for msg in &request.messages {
        if let Some(ref rc) = msg.reasoning_content {
            if let Some(arr) = body["messages"].as_array_mut() {
                for m in arr.iter_mut() {
                    if m["role"] == "assistant" && m["content"] == msg.content {
                        m["reasoning_content"] = serde_json::json!(rc);
                    }
                }
            }
        }
    }

    let mut body = serde_json::json!({
        "model": resolved.api_model,
        "max_tokens": resolved.max_tokens,
        "messages": all_messages,
    });

    // Optional params from resolved model (flat fields)
    if let Some(temp) = resolved.temperature {
        body["temperature"] = serde_json::json!(temp);
    }
    if let Some(top_p) = resolved.top_p {
        body["top_p"] = serde_json::json!(top_p);
    }
    if let Some(top_k) = resolved.top_k {
        // Only send top_k for non-OpenAI providers (OpenAI API doesn't support it)
        if resolved.provider != "openai" {
            body["top_k"] = serde_json::json!(top_k);
        }
    }
    if let Some(stop) = &resolved.stop {
        body["stop"] = serde_json::json!(stop);
    }
    if let Some(pp) = resolved.presence_penalty {
        body["presence_penalty"] = serde_json::json!(pp);
    }
    if let Some(fp) = resolved.frequency_penalty {
        body["frequency_penalty"] = serde_json::json!(fp);
    }

    // Reasoning/thinking for OpenAI-compat models
    if resolved.can_reason {
        if let Some(effort) = &resolved.reasoning_effort {
            body["reasoning_effort"] = serde_json::json!(effort);
        }
    }

    // DeepSeek thinking mode
    if let Some(thinking) = resolved.thinking {
        if thinking {
            body["temperature"] = serde_json::json!(1);
        }
    }

    // Tools
    if !request.tools.is_empty() {
        let tools: Vec<serde_json::Value> = request.tools.iter().map(|t| {
            serde_json::json!({
                "type": "function",
                "function": {
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.input_schema,
                }
            })
        }).collect();
        body["tools"] = serde_json::json!(tools);
        body["tool_choice"] = serde_json::json!("auto");
    }

    // Set cache key header for supported providers
    let mut req = self.client
        .post(&endpoint)
        .header("Authorization", format!("Bearer {}", api_key));

    if resolved.set_cache_key {
        req = req.header("x-cache-key", format!("{}:{}", resolved.provider, resolved.api_model));
    }

    // Custom headers
    for (key, value) in &resolved.headers {
        req = req.header(key.as_str(), value.as_str());
    }

    let timeout = std::time::Duration::from_secs(resolved.timeout_secs);
    let response = tokio::time::timeout(
        timeout,
        req.json(&body).send(),
    )
    .await
    .map_err(|e| CoreError::Llm(format!("Timeout or request error: {}", e)))??;

    // ... same response parsing as current
}
```

- [ ] **Step 6: Rewrite `call_ollama_resolved()` â€?uses native Ollama API format**

Ollama uses its native `/api/chat` endpoint, NOT the OpenAI-compatible format. The response format is `{"message": {"content": "..."}}`, not `{"choices": [{"message": {"content": "..."}}]}`.

```rust
async fn call_ollama_resolved(
    &self,
    resolved: &crate::llm::provider::ResolvedModel,
    _api_key: &str,  // Ollama doesn't need auth
    request: &ChatRequest,
) -> Result<ChatResponse, CoreError> {
    // Ollama native API: /api/chat (not OpenAI-compat /v1/chat/completions)
    let endpoint = format!("{}/chat", resolved.base_url.trim_end_matches('/'));

    let mut body = serde_json::json!({
        "model": resolved.api_model,
        "system": request.system_prompt,
        "messages": request.messages.iter().map(|m| {
            serde_json::json!({"role": m.role, "content": m.content})
        }).collect::<Vec<_>>(),
        "stream": false,
    });

    // Optional params
    if let Some(temp) = resolved.temperature {
        body["temperature"] = serde_json::json!(temp);
    }
    if let Some(top_p) = resolved.top_p {
        body["top_p"] = serde_json::json!(top_p);
    }
    if let Some(stop) = &resolved.stop {
        body["stop"] = serde_json::json!(stop);
    }

    // Ollama doesn't need auth headers
    let timeout = std::time::Duration::from_secs(resolved.timeout_secs);
    let response = tokio::time::timeout(
        timeout,
        self.client.post(&endpoint).json(&body).send(),
    )
    .await
    .map_err(|e| CoreError::Llm(format!("Ollama timeout: {}", e)))??;

    let json: serde_json::Value = response.json().await?;
    let content = json["message"]["content"]
        .as_str()
        .unwrap_or("")
        .to_string();

    Ok(ChatResponse {
        content,
        tool_calls: vec![],
        reasoning_content: None,
        usage: TokenUsage { input_tokens: 0, output_tokens: 0 },
    })
}
```

- [ ] **Step 7: Update LlmGateway::new() in lib.rs call sites**

Update `Core::new()` and any other LlmGateway construction:
```rust
// In Core::new():
let llm_config = config::load_llm_config(llm_config_path)?;
let provider_resolver = llm::provider::ProviderResolver::new(llm_config.provider.clone());
// ...
llm_gateway: llm::gateway::LlmGateway::new(llm_config, provider_resolver.clone()),
```

- [ ] **Step 8: Build and test**

Run: `cargo check -p feishu-agent-core`

---

### Task 5: Update llm_config.yaml + Tests

**Files:**
- Modify: `llm_config.yaml`
- Modify: `crates/core/src/config/mod.rs` (tests)
- Modify: `crates/core/src/tests/smoke_test.rs` (integration)

- [ ] **Step 1: Rewrite `llm_config.yaml` with provider section**

The existing `models:` section is kept for backward compatibility â€?it will parse but the new system primarily uses `provider:`. The `LlmConfig` struct keeps both:

```yaml
# Provider configurations â€?matches OpenCode schema
# (primary config format, recommended for all users)
provider:
  anthropic:
    name: "Anthropic"
    env: ["ANTHROPIC_API_KEY"]
    options:
      baseURL: https://api.anthropic.com/v1
      timeout: 300000
      setCacheKey: true
    models:
      claude-sonnet-4-20250514:
        name: "Claude Sonnet 4"
        limit: { context: 200000, output: 50000 }

  deepseek:
    name: "DeepSeek"
    env: ["DEEPSEEK_API_KEY"]
    options:
      baseURL: https://api.deepseek.com/v1
      timeout: 300000
    models:
      deepseek-v4-pro:
        name: "DeepSeek V4 Pro"
        limit: { context: 64000, output: 8192 }
      deepseek-v4-flash:
        name: "DeepSeek V4 Flash"
        limit: { context: 64000, output: 8192 }

  openai:
    name: "OpenAI"
    env: ["OPENAI_API_KEY"]
    options:
      baseURL: https://api.openai.com/v1
      timeout: 300000
    models:
      gpt-4o:
        name: "GPT 4o"
        limit: { context: 128000, output: 4096 }

  ollama:
    name: "Ollama (local)"
    options:
      baseURL: http://localhost:11434/api
      timeout: 60000
```

- [ ] **Step 2: Update config/mod.rs tests**

Update the test YAML to work with the new provider resolution. Add a test for `resolve_model()`:

```rust
#[test]
fn test_model_provider_resolution() {
    use crate::llm::models;
    use crate::llm::provider::ProviderResolver;

    let resolver = ProviderResolver::new(std::collections::HashMap::new());

    let yaml = r#"
name: "test"
role: "test"
llm:
  primary:
    model: deepseek/deepseek-v4-flash
    max_tokens: 8192
triggers: []
"#;
    let config: crate::config::agent::AgentConfig = serde_yaml::from_str(yaml).unwrap();
    let resolved = resolver.resolve(&config.llm.primary);

    assert_eq!(resolved.provider, "deepseek");
    assert_eq!(resolved.model_name, "deepseek-v4-flash");
    assert_eq!(resolved.api_model, "deepseek-v4-flash");  // from built-in
    assert_eq!(resolved.base_url, "https://api.deepseek.com/v1");
    assert_eq!(resolved.timeout_secs, 300);
    assert_eq!(resolved.max_tokens, 8192);
    assert!(resolved.can_reason);
}

#[test]
fn test_model_provider_resolution_with_agent_override() {
    use crate::llm::provider::ProviderResolver;
    use std::collections::HashMap;

    let resolver = ProviderResolver::new(HashMap::new());

    let yaml = r#"
name: "test"
role: "test"
llm:
  primary:
    model: anthropic/claude-sonnet-4-20250514
    max_tokens: 16384
    base_url: https://custom-proxy.com/v1
    api_key_env: MY_CUSTOM_KEY
triggers: []
"#;
    let config: crate::config::agent::AgentConfig = serde_yaml::from_str(yaml).unwrap();
    let resolved = resolver.resolve(&config.llm.primary);

    assert_eq!(resolved.provider, "anthropic");
    assert_eq!(resolved.base_url, "https://custom-proxy.com/v1");
    assert_eq!(resolved.max_tokens, 16384);
    // api_key_env should resolve to MY_CUSTOM_KEY
}

#[test]
fn test_model_provider_resolution_unknown_model() {
    use crate::llm::provider::ProviderResolver;
    use std::collections::HashMap;

    let resolver = ProviderResolver::new(HashMap::new());

    let yaml = r#"
name: "test"
role: "test"
llm:
  primary:
    model: unknown/custom-model-v1
    max_tokens: 4096
triggers: []
"#;
    let config: crate::config::agent::AgentConfig = serde_yaml::from_str(yaml).unwrap();
    let resolved = resolver.resolve(&config.llm.primary);

    assert_eq!(resolved.provider, "unknown");
    assert_eq!(resolved.api_model, "custom-model-v1");
    // Should fall back to generic defaults
    assert!(!resolved.can_reason);
}

#[test]
fn test_provider_config_overrides_builtin() {
    use crate::llm::provider::ProviderResolver;
    use crate::config::provider::*;
    use std::collections::HashMap;

    let mut provider_configs = HashMap::new();
    provider_configs.insert("deepseek".to_string(), ProviderConfig {
        name: Some("My DeepSeek".into()),
        npm: None,
        env: vec![],
        whitelist: None,
        blacklist: None,
        options: ProviderOptions {
            base_url: Some("https://my-deepseek-proxy.com/v1".into()),
            timeout: Some(60000),
            ..Default::default()
        },
        models: HashMap::new(),
    });

    let resolver = ProviderResolver::new(provider_configs);

    let yaml = r#"
name: "test"
role: "test"
llm:
  primary:
    model: deepseek/deepseek-v4-pro
    max_tokens: 8192
triggers: []
"#;
    let config: crate::config::agent::AgentConfig = serde_yaml::from_str(yaml).unwrap();
    let resolved = resolver.resolve(&config.llm.primary);

    assert_eq!(resolved.base_url, "https://my-deepseek-proxy.com/v1");
    assert_eq!(resolved.timeout_secs, 60);  // 60000ms â†?60s
}
```

- [ ] **Step 3: Ensure all smoke tests still pass or are updated**

The smoke tests need `FEISHU_CHAT_ID` and lark-cli. Mark as integration tests.

Run: `cargo test -p feishu-agent-core --lib` (52 unit tests should pass)

---

### Task 6: ResolvedModel Usage in Gateway â€?Full Implementation

**Files:**
- Modify: `crates/core/src/llm/gateway.rs`

- [ ] **Step 1: Full rewrites of all three provider call methods**

Replace the old `call_anthropic()`, `call_ollama()`, `call_openai_compat()` with the new `_resolved` versions.

**IMPORTANT**: These replace the old methods completely. The old methods are deleted. The `chat()` method calls the new `_resolved` versions.

The `chat()` method uses `resolved.rate_limiter_key` for rate limiting (after resolution, using the `{provider}/{model_name}` format). The retry loop wraps the provider dispatch and stays unchanged. After resolution, the provider-specific methods use `ResolvedModel` for all parameters.

**Key changes in each method:**

`call_anthropic_resolved()`:
- Endpoint: `{resolved.base_url}/messages` (was hardcoded `https://api.anthropic.com/v1/messages`)
- API key: passed as parameter (resolved from resolution chain)
- Model: `resolved.api_model`
- Timeout: `resolved.timeout_secs`
- Headers: `resolved.headers` merged into request
- Prompt caching: when `resolved.set_cache_key`, add cache_control to system prompt
- Thinking: when `resolved.can_reason` + `reasoning_effort`, enable thinking with budget_tokens
- All optional params from ResolvedModel flat fields (temperature, top_p, top_k, stop)
- Same response parsing as current

`call_openai_compat_resolved()`:
- Endpoint: `{resolved.base_url}/chat/completions`
- API key: passed as parameter
- Model: `resolved.api_model`
- Timeout: `resolved.timeout_secs`
- Headers: `resolved.headers` merged
- Cache key header: when `resolved.set_cache_key`
- Reasoning: when `resolved.can_reason`, pass `reasoning_effort` in body
- All optional params from ResolvedModel flat fields
- Same tool/response parsing as current

`call_ollama_resolved()`:
- Uses native Ollama endpoint `{resolved.base_url}/chat` (base_url = `http://localhost:11434/api`)
- No API key (Ollama doesn't need auth)
- Response format: `{"message": {"content": "..."}}` (native Ollama format)
- Wire temperature, top_p, stop from flat ResolvedModel fields

- [ ] **Step 2: Build and test**

Run: `cargo check -p feishu-agent-core`
Run: `cargo test -p feishu-agent-core --lib`

---

### Self-Review Checklist

After all tasks, verify:
1. [ ] Every method is fully implemented â€?no empty bodies, no TODO stubs, no `unimplemented!()` macros
2. [ ] All provider options (baseURL, apiKey, timeout, headers, setCacheKey, chunkTimeout) are used in actual API calls
3. [ ] All model definition fields (limits, costs, capabilities) are accessible but costs/limits don't need to be wired to anything yet (they're informational)
4. [ ] `can_reason` actually enables thinking/reasoning in API calls
5. [ ] Resolution chain is correctly ordered: agent > provider config > built-in
6. [ ] Unknown providers/models fall back to sensible defaults (not crash)
7. [ ] All 52 unit tests pass
8. [ ] Old `provider` field removal verified (no stale references)