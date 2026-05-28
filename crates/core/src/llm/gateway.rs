use std::collections::HashMap;
use std::time::Duration;
use crate::config::agent::ModelConfig;
use crate::config::llm::LlmConfig;
use crate::CoreError;
use super::provider::ProviderResolver;
use super::rate_limiter::RateLimiter;

#[derive(Clone)]
pub struct LlmGateway {
    client: reqwest::Client,
    models: HashMap<String, ModelConfig>,
    rate_limiters: HashMap<String, RateLimiter>,
    provider_resolver: ProviderResolver,
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
    pub fn new(config: LlmConfig, provider_resolver: ProviderResolver) -> Self {
        // Check skip_verify_ssl from both legacy models and provider configs
        let skip_verify_legacy = config.models.values().any(|m| m.skip_verify_ssl.unwrap_or(false));
        let skip_verify_provider = config.provider.values().any(|p| {
            p.options.headers.contains_key("skip_verify_ssl")
                || p.options.base_url.as_deref().map_or(false, |url| url.starts_with("http://"))
        });
        let skip_verify = skip_verify_legacy || skip_verify_provider;

        let mut client_builder = reqwest::Client::builder()
            .timeout(Duration::from_secs(180));

        if skip_verify {
            client_builder = client_builder.danger_accept_invalid_certs(true);
        }

        let client = client_builder.build().unwrap();

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

    /// Look up model config from global pool by name
    pub fn get_model(&self, name: &str) -> Option<&ModelConfig> {
        self.models.get(name)
    }

    /// Resolve a ModelConfig through the provider resolution chain
    pub fn resolve_model(&self, config: &ModelConfig) -> super::provider::ResolvedModel {
        self.provider_resolver.resolve(config)
    }

    /// Chat using a ModelConfig directly (from agent's embedded config or global pool).
    /// Uses ProviderResolver to resolve the full endpoint, keys, headers, and params.
    pub async fn chat(
        &self,
        model_config: &ModelConfig,
        request: &ChatRequest,
    ) -> Result<ChatResponse, CoreError> {
        let resolved = self.resolve_model(model_config);

        // Rate limit using resolved rate_limiter_key
        if let Some(limiter) = self.rate_limiters.get(&resolved.rate_limiter_key) {
            limiter.acquire().await;
        }

        // Resolve API key
        let api_key = if resolved.api_key_is_direct {
            resolved.api_key.clone()
        } else {
            let env_val = std::env::var(&resolved.api_key)
                .map_err(|_| CoreError::Llm(format!("{} not set", resolved.api_key)))?;
            env_val
        };

        let max_retries = model_config.max_retries.unwrap_or(0);
        let mut last_error = None;

        for attempt in 0..=max_retries {
            if attempt > 0 {
                tracing::warn!(
                    "Retry attempt {}/{} for {}",
                    attempt,
                    max_retries,
                    resolved.api_model
                );
                tokio::time::sleep(std::time::Duration::from_secs(2u64.pow(attempt))).await;
            }

            let result = match resolved.provider.as_str() {
                "anthropic" => self.call_anthropic_resolved(&resolved, &api_key, request).await,
                "ollama" => self.call_ollama_resolved(&resolved, &api_key, request).await,
                "deepseek" | "openai" => self.call_openai_compat_resolved(&resolved, &api_key, request).await,
                provider => Err(CoreError::Llm(format!("Unsupported provider: {}", provider))),
            };

            match result {
                Ok(resp) => return Ok(resp),
                Err(e) => {
                    let err_str = e.to_string();
                    if err_str.contains("401")
                        || err_str.contains("403")
                        || err_str.contains("400")
                        || err_str.contains("Unsupported provider")
                    {
                        return Err(e);
                    }
                    last_error = Some(e);
                }
            }
        }

        Err(last_error.unwrap_or_else(|| CoreError::Llm("Max retries exceeded".into())))
    }

    async fn call_anthropic_resolved(
        &self,
        resolved: &super::provider::ResolvedModel,
        api_key: &str,
        request: &ChatRequest,
    ) -> Result<ChatResponse, CoreError> {
        let endpoint = format!("{}/messages", resolved.base_url.trim_end_matches('/'));

        let messages: Vec<serde_json::Value> = request.messages.iter().map(|m| {
            match serde_json::from_str::<serde_json::Value>(&m.content) {
                Ok(serde_json::Value::Array(_)) => {
                    serde_json::json!({"role": m.role, "content": serde_json::from_str::<serde_json::Value>(&m.content).unwrap()})
                }
                _ => {
                    serde_json::json!({"role": m.role, "content": m.content})
                }
            }
        }).collect();

        let mut body = serde_json::json!({
            "model": resolved.api_model,
            "max_tokens": resolved.max_tokens,
            "system": request.system_prompt,
            "messages": messages,
        });

        // Tools (only if model supports tool calling)
        if resolved.capabilities.supports_tools && !request.tools.is_empty() {
            let tools: Vec<serde_json::Value> = request.tools.iter().map(|t| {
                serde_json::json!({
                    "name": t.name,
                    "description": t.description,
                    "input_schema": t.input_schema,
                })
            }).collect();
            body["tools"] = serde_json::json!(tools);
        }

        // Optional params (gated by capabilities)
        if resolved.capabilities.supports_temperature {
            if let Some(temp) = resolved.temperature {
                body["temperature"] = serde_json::json!(temp);
            }
            if let Some(top_p) = resolved.top_p {
                body["top_p"] = serde_json::json!(top_p);
            }
        }
        if let Some(top_k) = resolved.top_k {
            body["top_k"] = serde_json::json!(top_k);
        }
        if let Some(stop) = &resolved.stop {
            body["stop_sequences"] = serde_json::json!(stop);
        }

        // Reasoning/thinking
        if resolved.can_reason {
            if let Some(effort) = &resolved.reasoning_effort {
                let budget_tokens = (resolved.max_tokens as f64 * match effort.as_str() {
                    "low" => 0.5,
                    "medium" => 0.7,
                    "high" => 0.9,
                    _ => 0.7,
                }) as u64;
                body["thinking"] = serde_json::json!({
                    "type": "enabled",
                    "budget_tokens": budget_tokens,
                });
                body["temperature"] = serde_json::json!(1);
            }
        }

        // Prompt caching
        if resolved.set_cache_key && request.system_prompt.len() > 1000 {
            if let Some(sys) = body["system"].as_str() {
                body["system"] = serde_json::json!([
                    {"type": "text", "text": sys, "cache_control": {"type": "ephemeral"}}
                ]);
            }
        }

        let timeout = std::time::Duration::from_secs(resolved.timeout_secs);
        let mut req = self.client
            .post(&endpoint)
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01");

        // Custom headers
        for (key, value) in &resolved.headers {
            req = req.header(key.as_str(), value.as_str());
        }

        let response = tokio::time::timeout(timeout, req.json(&body).send())
            .await
            .map_err(|e| CoreError::Llm(format!("Timeout or request error: {}", e)))??;

        let json: serde_json::Value = response.json().await?;

        // Parse content blocks
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
            usage: TokenUsage { input_tokens, output_tokens },
        })
    }

    async fn call_ollama_resolved(
        &self,
        resolved: &super::provider::ResolvedModel,
        _api_key: &str,
        request: &ChatRequest,
    ) -> Result<ChatResponse, CoreError> {
        let endpoint = format!("{}/chat", resolved.base_url.trim_end_matches('/'));

        let mut body = serde_json::json!({
            "model": resolved.api_model,
            "system": request.system_prompt,
            "messages": request.messages.iter().map(|m| {
                serde_json::json!({"role": m.role, "content": m.content})
            }).collect::<Vec<_>>(),
            "stream": false,
        });

        // Optional params (gated by capabilities)
        if resolved.capabilities.supports_temperature {
            if let Some(temp) = resolved.temperature {
                body["temperature"] = serde_json::json!(temp);
            }
            if let Some(top_p) = resolved.top_p {
                body["top_p"] = serde_json::json!(top_p);
            }
        }
        if let Some(stop) = &resolved.stop {
            body["stop"] = serde_json::json!(stop);
        }

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

    async fn call_openai_compat_resolved(
        &self,
        resolved: &super::provider::ResolvedModel,
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

        let mut body = serde_json::json!({
            "model": resolved.api_model,
            "max_tokens": resolved.max_tokens,
            "messages": all_messages,
        });

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

        // Tools (only if model supports tool calling)
        if resolved.capabilities.supports_tools && !request.tools.is_empty() {
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

        // Optional params (gated by capabilities)
        if resolved.capabilities.supports_temperature {
            if let Some(temp) = resolved.temperature {
                body["temperature"] = serde_json::json!(temp);
            }
            if let Some(top_p) = resolved.top_p {
                body["top_p"] = serde_json::json!(top_p);
            }
        }
        if let Some(top_k) = resolved.top_k {
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

        // Reasoning/thinking
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

        let timeout = std::time::Duration::from_secs(resolved.timeout_secs);
        let mut req = self.client
            .post(&endpoint)
            .header("Authorization", format!("Bearer {}", api_key));

        // Cache key header
        if resolved.set_cache_key {
            req = req.header("x-cache-key", format!("{}:{}", resolved.provider, resolved.api_model));
        }

        // Custom headers
        for (key, value) in &resolved.headers {
            req = req.header(key.as_str(), value.as_str());
        }

        let response = tokio::time::timeout(timeout, req.json(&body).send())
            .await
            .map_err(|e| CoreError::Llm(format!("Timeout or request error: {}", e)))??;

        let json: serde_json::Value = response.json().await?;

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
            usage: TokenUsage { input_tokens, output_tokens },
        })
    }
}
