use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub name: String,
    pub role: String,
    pub personality: Option<String>,
    pub llm: LlmAgentConfig,
    pub triggers: Vec<TriggerConfig>,
    #[serde(default)]
    pub skills: Vec<String>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerConfig {
    pub pattern: String,
    #[serde(default = "default_auto_respond")]
    pub auto_respond: bool,
}

fn default_auto_respond() -> bool { true }
