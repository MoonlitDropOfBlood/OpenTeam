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
    pub tools: Vec<ToolDefinition>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
}

impl ChatMessage {
    pub fn new(role: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: role.into(),
            content: content.into(),
            reasoning_content: None,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct ChatResponse {
    pub content: String,
    pub tool_calls: Vec<ToolCall>,
    pub usage: TokenUsage,
    pub reasoning_content: Option<String>,
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
            "deepseek" => self.call_openai(model_config, request).await,
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

        // Build messages array — support both plain text and structured content blocks
        let messages: Vec<serde_json::Value> = request.messages.iter().map(|m| {
            match serde_json::from_str::<serde_json::Value>(&m.content) {
                Ok(serde_json::Value::Array(_)) => {
                    // Already structured content blocks (tool_result, tool_use)
                    serde_json::json!({"role": m.role, "content": serde_json::from_str::<serde_json::Value>(&m.content).unwrap()})
                }
                _ => {
                    // Plain text message
                    serde_json::json!({"role": m.role, "content": m.content})
                }
            }
        }).collect();

        let mut body = serde_json::json!({
            "model": config.model,
            "max_tokens": config.max_tokens,
            "system": request.system_prompt,
            "messages": messages,
        });

        // Add tools if present
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

        // Parse content blocks — extract text and tool_use
        let mut text_content = String::new();
        let mut tool_calls = Vec::new();

        if let Some(content_blocks) = json["content"].as_array() {
            for block in content_blocks {
                match block["type"].as_str() {
                    Some("text") => {
                        if let Some(text) = block["text"].as_str() {
                            text_content.push_str(text);
                        }
                    }
                    Some("tool_use") => {
                        tool_calls.push(ToolCall {
                            id: block["id"].as_str().unwrap_or("").to_string(),
                            name: block["name"].as_str().unwrap_or("").to_string(),
                            arguments: block["input"].clone(),
                        });
                    }
                    _ => {}
                }
            }
        }

        // Fallback for older API format or simple text responses
        if text_content.is_empty() && tool_calls.is_empty() {
            text_content = json["content"][0]["text"]
                .as_str()
                .unwrap_or("")
                .to_string();
        }

        let input_tokens = json["usage"]["input_tokens"].as_u64().unwrap_or(0) as u32;
        let output_tokens = json["usage"]["output_tokens"].as_u64().unwrap_or(0) as u32;

        Ok(ChatResponse {
            content: text_content,
            tool_calls,
            reasoning_content: None,
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
            tool_calls: vec![],
            reasoning_content: None,
            usage: TokenUsage {
                input_tokens: 0,
                output_tokens: 0,
            },
        })
    }

    async fn call_openai(
        &self,
        config: &ModelConfig,
        request: &ChatRequest,
    ) -> Result<ChatResponse, CoreError> {
        let api_key = std::env::var(
            config.api_key_env.as_deref().unwrap_or("OPENAI_API_KEY"),
        )
        .map_err(|_| CoreError::Llm(format!(
            "{} not set",
            config.api_key_env.as_deref().unwrap_or("OPENAI_API_KEY"),
        )))?;

        // Build messages array with system prompt
        let mut all_messages: Vec<serde_json::Value> = vec![
            serde_json::json!({"role": "system", "content": request.system_prompt}),
        ];
        all_messages.extend(request.messages.iter().map(|m| {
            serde_json::json!({"role": m.role, "content": m.content})
        }));

        let mut body = serde_json::json!({
            "model": config.model,
            "max_tokens": config.max_tokens,
            "messages": all_messages,
        });

        // Add tools if present
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

        // Determine API base URL based on provider
        let api_base = match config.provider.as_str() {
            "deepseek" => "https://api.deepseek.com/v1/chat/completions",
            provider => {
                return Err(CoreError::Llm(format!(
                    "Unknown OpenAI-compatible provider: {provider}"
                )))
            }
        };

        let timeout = Duration::from_secs(config.timeout_secs.unwrap_or(120));
        let response = tokio::time::timeout(
            timeout,
            self.client
                .post(api_base)
                .header("Authorization", format!("Bearer {}", api_key))
                .json(&body)
                .send(),
        )
        .await
        .map_err(|e| CoreError::Llm(format!("Timeout or request error: {}", e)))??;

        let json: serde_json::Value = response.json().await?;

        // Parse OpenAI-compatible response
        let choice = &json["choices"][0];
        let message = &choice["message"];
        let content = message["content"].as_str().unwrap_or("").to_string();

        let mut tool_calls = Vec::new();
        if let Some(tcs) = message["tool_calls"].as_array() {
            for tc in tcs {
                tool_calls.push(ToolCall {
                    id: tc["id"].as_str().unwrap_or("").to_string(),
                    name: tc["function"]["name"].as_str().unwrap_or("").to_string(),
                    arguments: tc["function"]["arguments"]
                        .as_str()
                        .and_then(|s| serde_json::from_str(s).ok())
                        .unwrap_or(serde_json::Value::Null),
                });
            }
        }

        let input_tokens = json["usage"]["prompt_tokens"].as_u64().unwrap_or(0) as u32;
        let output_tokens = json["usage"]["completion_tokens"].as_u64().unwrap_or(0) as u32;

        Ok(ChatResponse {
            content,
            tool_calls,
            reasoning_content: message["reasoning_content"].as_str().map(|s| s.to_string()),
            usage: TokenUsage {
                input_tokens,
                output_tokens,
            },
        })
    }
}
