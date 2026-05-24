use std::collections::HashMap;
use std::time::Duration;
use crate::config::agent::ModelConfig;
use crate::config::llm::LlmConfig;
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
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone)]
pub struct ChatResponse {
    pub content: String,
    pub usage: TokenUsage,
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

    /// Look up model config from global pool by name
    pub fn get_model(&self, name: &str) -> Option<&ModelConfig> {
        self.models.get(name)
    }

    /// Chat using a ModelConfig directly (from agent's embedded config or global pool)
    pub async fn chat(
        &self,
        model_config: &ModelConfig,
        request: &ChatRequest,
    ) -> Result<ChatResponse, CoreError> {
        // Rate limit
        if let Some(limiter) = self.rate_limiters.get(&request.model) {
            limiter.acquire().await;
        }

        match model_config.provider.as_str() {
            "anthropic" => self.call_anthropic(model_config, request).await,
            "ollama" => self.call_ollama(model_config, request).await,
            provider => Err(CoreError::Llm(format!("Unsupported provider: {}", provider))),
        }
    }

    async fn call_anthropic(
        &self,
        config: &ModelConfig,
        request: &ChatRequest,
    ) -> Result<ChatResponse, CoreError> {
        let api_key = std::env::var(
            config.api_key_env.as_deref().unwrap_or("ANTHROPIC_API_KEY"),
        )
        .map_err(|_| CoreError::Llm("ANTHROPIC_API_KEY not set".into()))?;

        let body = serde_json::json!({
            "model": config.model,
            "max_tokens": config.max_tokens,
            "system": request.system_prompt,
            "messages": request.messages.iter().map(|m| {
                serde_json::json!({"role": m.role, "content": m.content})
            }).collect::<Vec<_>>(),
        });

        let timeout = Duration::from_secs(config.timeout_secs.unwrap_or(120));
        let response = tokio::time::timeout(
            timeout,
            self.client
                .post("https://api.anthropic.com/v1/messages")
                .header("x-api-key", &api_key)
                .header("anthropic-version", "2023-06-01")
                .json(&body)
                .send(),
        )
        .await
        .map_err(|e| CoreError::Llm(format!("Timeout or request error: {}", e)))??;

        let json: serde_json::Value = response.json().await?;

        let content = json["content"][0]["text"]
            .as_str()
            .unwrap_or("")
            .to_string();
        let input_tokens = json["usage"]["input_tokens"].as_u64().unwrap_or(0) as u32;
        let output_tokens = json["usage"]["output_tokens"].as_u64().unwrap_or(0) as u32;

        Ok(ChatResponse {
            content,
            usage: TokenUsage {
                input_tokens,
                output_tokens,
            },
        })
    }

    async fn call_ollama(
        &self,
        config: &ModelConfig,
        request: &ChatRequest,
    ) -> Result<ChatResponse, CoreError> {
        let body = serde_json::json!({
            "model": config.model,
            "system": request.system_prompt,
            "messages": request.messages.iter().map(|m| {
                serde_json::json!({"role": m.role, "content": m.content})
            }).collect::<Vec<_>>(),
            "stream": false,
        });

        let timeout = Duration::from_secs(config.timeout_secs.unwrap_or(60));

        let response = tokio::time::timeout(
            timeout,
            self.client
                .post("http://localhost:11434/api/chat")
                .json(&body)
                .send(),
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
            usage: TokenUsage {
                input_tokens: 0,
                output_tokens: 0,
            },
        })
    }
}
