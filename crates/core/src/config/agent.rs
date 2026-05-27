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
    /// Skip SSL certificate verification. Applies to ALL models sharing the same
    /// reqwest client — if any model has this enabled, TLS verification is disabled globally.
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
