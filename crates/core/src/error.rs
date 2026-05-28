use thiserror::Error;

#[derive(Error, Debug)]
pub enum CoreError {
    #[error("Config error: {0}")]
    Config(String),

    #[error("LLM error: {0}")]
    Llm(String),

    /// LLM authentication error (401/403) — not retryable
    #[error("LLM auth error ({provider}): {message}")]
    LlmAuth { provider: String, message: String },

    /// LLM rate limit error (429) — retryable with optional Retry-After
    #[error("LLM rate limited ({provider}): {message}")]
    LlmRateLimit { provider: String, message: String, retry_after_secs: Option<u32> },

    /// LLM API error (4xx/5xx) — may be retryable
    #[error("LLM API error ({provider}): {message}")]
    LlmApi { provider: String, message: String, status_code: u16, retryable: bool },

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

    #[error("Memory error: {0}")]
    Memory(String),

    #[error("Plugin error: {0}")]
    Plugin(String),

    #[error("Assistant error: {0}")]
    Assistant(String),

    #[error("Skill error: {0}")]
    Skill(String),

    #[error("MCP error: {0}")]
    Mcp(String),
}
