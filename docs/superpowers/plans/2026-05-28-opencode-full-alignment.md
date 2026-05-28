# OpenCode Full Alignment Plan �?LLM HTTP Layer

> **Goal:** Full parity with OpenCode's LLM HTTP request/response handling �?correct token counting, complete cache handling, structured errors, additional providers, and streaming support.

**Architecture:** Incremental alignment in 3 phases. Phase 1 fixes correctness issues (max_completion_tokens, Anthropic input_tokens counting, retry jitter). Phase 2 adds structural parity (extended usage tracking, thinking blocks, cache control, additional providers). Phase 3 adds streaming (major architectural change).

---

## Gaps Summary (from OpenCode audit)

| # | Gap | Severity | OpenCode Reference |
|---|-----|----------|-------------------|
| G1 | `max_completion_tokens` vs `max_tokens` | 🟡 High | `openai.go:170-185`: reasoning �?`MaxCompletionTokens` |
| G2 | Anthropic `input_tokens` = non-cached only | 🟡 High | `anthropic-messages.ts:485-506`: sums with cache_* |
| G3 | No retry jitter + no Retry-After header | 🟡 High | `openai.go:330-365`: 20% jitter, `retryAfter` header |
| G4 | No extended Usage (cache/reasoning breakdown) | 🟡 Medium | `events.ts:43-65`: `Usage` with 8 fields |
| G5 | Anthropic thinking blocks silently dropped | 🟡 Medium | `anthropic-messages.ts`: `thinking_delta`/`ReasoningDelta` |
| G6 | Anthropic prompt cache only covers system prompt | 🟡 Medium | `anthropic.go:71-103`: last 3 messages + last tool |
| G7 | Error handling via string matching | 🟢 Low | `anthropic-messages.ts:709-718`: typed `error.type` |
| G8 | Missing GROQ / OpenRouter / xAI providers | 🟢 Low | `provider.go:130-154` |
| G9 | Finish reason not captured | 🟢 Low | `anthropic-messages.ts`: `stop_reason` map |
| G10 | No streaming | 🔴 Future | Entire SSE state machine |

---

## Phase 1: Correctness Fixes

### Task 1.1: `max_completion_tokens` for reasoning models

**Files:** `crates/core/src/llm/gateway.rs:385-389`

**Change:** In `call_openai_compat_resolved`, when `resolved.provider == "openai" && resolved.can_reason`, send `max_completion_tokens` instead of `max_tokens`. This only applies to OpenAI o1/o3/o4 series �?DeepSeek and other OpenAI-compat providers always use `max_tokens`.

Current:
```rust
let mut body = serde_json::json!({
    "model": resolved.api_model,
    "max_tokens": resolved.max_tokens,
    "messages": all_messages,
});
```

New:
```rust
let use_completion_tokens = resolved.provider == "openai" && resolved.can_reason;
let mut body = if use_completion_tokens {
    serde_json::json!({
        "model": resolved.api_model,
        "max_completion_tokens": resolved.max_tokens,
        "messages": all_messages,
    })
} else {
    serde_json::json!({
        "model": resolved.api_model,
        "max_tokens": resolved.max_tokens,
        "messages": all_messages,
    })
};
```

**QA:** `cargo test -p feishu-agent-core --lib` passes.

### Task 1.2: Fix Anthropic input_tokens counting

**Files:** `crates/core/src/llm/gateway.rs:307-308`

**Change:** Anthropic's `input_tokens` is the non-cached portion. The inclusive total is `input_tokens + cache_read_input_tokens + cache_creation_input_tokens`.

Current:
```rust
let input_tokens = json["usage"]["input_tokens"].as_u64().unwrap_or(0) as u32;
let output_tokens = json["usage"]["output_tokens"].as_u64().unwrap_or(0) as u32;
```

New:
```rust
let raw_input = json["usage"]["input_tokens"].as_u64().unwrap_or(0) as u32;
let cache_read = json["usage"]["cache_read_input_tokens"].as_u64().unwrap_or(0) as u32;
let cache_create = json["usage"]["cache_creation_input_tokens"].as_u64().unwrap_or(0) as u32;
let input_tokens = raw_input + cache_read + cache_create;
let output_tokens = json["usage"]["output_tokens"].as_u64().unwrap_or(0) as u32;
```

**Also update `TokenUsage` struct** to hold cache breakdown:
```rust
#[derive(Debug, Clone)]
pub struct TokenUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub cache_read_input_tokens: Option<u32>,
    pub cache_write_input_tokens: Option<u32>,
    pub reasoning_tokens: Option<u32>,
}
```

Update `ChatResponse` in `call_anthropic_resolved`:
```rust
usage: TokenUsage {
    input_tokens,
    output_tokens,
    cache_read_input_tokens: Some(cache_read),
    cache_write_input_tokens: Some(cache_create),
    reasoning_tokens: None,
}
```

Update `call_openai_compat_resolved`:
```rust
let cached = json["usage"]["prompt_tokens_details"]["cached_tokens"].as_u64().unwrap_or(0) as u32;
let reasoning = json["usage"]["completion_tokens_details"]["reasoning_tokens"].as_u64().unwrap_or(0) as u32;
// ...
usage: TokenUsage {
    input_tokens,
    output_tokens,
    cache_read_input_tokens: if cached > 0 { Some(cached) } else { None },
    cache_write_input_tokens: None,
    reasoning_tokens: if reasoning > 0 { Some(reasoning) } else { None },
}
```

Update `call_ollama_resolved` (no change needed �?Ollama returns 0/0).

### Task 1.3: Retry with jitter + Retry-After header + structured error types

**Files:** `crates/core/src/llm/gateway.rs:139-176`, `crates/core/src/error.rs`, `Cargo.toml`

**Change:** Replace fixed exponential backoff with jitter + Retry-After header parsing. Also add structured error types alongside existing `Llm(String)` (NOT rename �?add new variants without removing old).

#### Step 1: Add structured error variants to `error.rs`

DO NOT rename `Llm(String)` �?add new typed variants alongside it:

```rust
#[derive(Error, Debug)]
pub enum CoreError {
    // ... existing variants unchanged ...
    
    /// LLM authentication error (401/403) �?not retryable
    #[error("LLM auth error ({provider}): {message}")]
    LlmAuth { provider: String, message: String },
    
    /// LLM rate limit error (429) �?retryable with optional Retry-After
    #[error("LLM rate limited ({provider}){retry_msg}: {message}")]
    LlmRateLimit { provider: String, message: String, retry_after_secs: Option<u32> },
    
    /// LLM API error (4xx/5xx) �?may be retryable
    #[error("LLM API error ({provider}): {message}")]
    LlmApi { provider: String, message: String, status_code: u16, retryable: bool },
}
```

The existing `Llm(String)` variant stays unchanged �?zero breaking change across the codebase.

#### Step 2: Retry with jitter + Retry-After

Add `rand` to `Cargo.toml`:
```toml
rand = "0.8"
```

New retry logic in `chat()`:

```rust
let max_retries = model_config.max_retries.unwrap_or(0);
let mut last_error = None;

for attempt in 0..=max_retries {
    if attempt > 0 {
        // Exponential backoff: 1s, 2s, 4s, 8s... (matching OpenCode's 2^(attempt-1)*1000ms)
        let base_ms = 1000u64 * (1u64 << (attempt - 1)); // 1000, 2000, 4000, ...
        let jitter_ms = rand::thread_rng().gen_range(0..=(base_ms / 5)); // 20% jitter
        let delay = Duration::from_millis(base_ms + jitter_ms);
        tokio::time::sleep(delay).await;
    }

    let result = match resolved.provider.as_str() {
        "anthropic" => self.call_anthropic_resolved(&resolved, &api_key, request).await,
        "ollama" => self.call_ollama_resolved(&resolved, &api_key, request).await,
        "deepseek" | "openai" | "groq" | "openrouter" | "xai"
            => self.call_openai_compat_resolved(&resolved, &api_key, request).await,
        provider => Err(CoreError::Llm(format!("Unsupported provider: {}", provider))),
    };

    match result {
        Ok(resp) => return Ok(resp),
        Err(CoreError::LlmAuth { .. }) => return Err(e),             // never retry auth
        Err(CoreError::LlmRateLimit { retry_after_secs: Some(after), .. }) => {
            tokio::time::sleep(Duration::from_secs(after as u64)).await;
            last_error = Some(e);
        }
        Err(CoreError::LlmApi { retryable: false, .. }) => return Err(e),
        Err(e @ CoreError::LlmApi { .. }) => last_error = Some(e),
        Err(e) => {
            // Fallback: string-based error matching for old Llm(String) variant
            let err_str = e.to_string();
            if err_str.contains("401") || err_str.contains("403")
                || err_str.contains("400") || err_str.contains("Unsupported provider")
            {
                return Err(e);
            }
            last_error = Some(e);
        }
    }
}
```

#### Step 3: Extract Retry-After in provider methods

In `call_openai_compat_resolved`, after HTTP response, capture `Retry-After` before error construction:

```rust
let status = response.status();
if !status.is_success() {
    let body_text = response.text().await.unwrap_or_default();
    let retry_after = response_headers
        .get("Retry-After")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u32>().ok());
    
    let err = match status.as_u16() {
        401 | 403 => CoreError::LlmAuth {
            provider: resolved.provider.clone(),
            message: body_text,
        },
        429 => CoreError::LlmRateLimit {
            provider: resolved.provider.clone(),
            message: body_text,
            retry_after_secs: retry_after,
        },
        code => CoreError::LlmApi {
            provider: resolved.provider.clone(),
            message: body_text,
            status_code: code,
            retryable: code >= 500,
        },
    };
    return Err(err);
}
```

---

## Phase 2: Structural Parity

### Task 2.1: Extended TokenUsage across all callers

**Files:** `crates/core/src/llm/gateway.rs` + `crates/core/src/llm/provider.rs` (ResolvedModel)

**Change:** The `TokenUsage` struct from Task 1.2 needs all call sites updated:
- `agent/manager.rs` �?builds ChatResponse from gateway response
- `assistant/assistant.rs` �?builds ChatResponse from gateway response

For now, the TokenUsage fields are populated only in gateway methods; callers just pass through the values. No changes needed in callers if they use the struct directly.

### Task 2.2: Parse Anthropic thinking blocks

**Files:** `crates/core/src/llm/gateway.rs:276-305`

**Change:** Add `thinking` block parsing to `call_anthropic_resolved`:

Current code drops `_ => {}` for unknown block types. Add:
```rust
Some("thinking") => {
    if let Some(text) = block["thinking"].as_str() {
        // Accumulate thinking content for reasoning_content
        reasoning_content.push_str(text);
    }
}
```

Also capture top-level `reasoning_content` from message:
```rust
// After parsing all blocks, if we found thinking blocks, set reasoning_content
let reasoning_content = if reasoning_content.is_empty() {
    None
} else {
    Some(reasoning_content)
};
```

Update response construction:
```rust
Ok(ChatResponse {
    content: text_content,
    tool_calls,
    reasoning_content,
    usage: TokenUsage { ... },
})
```

### Task 2.3: Extended prompt caching (Anthropic)

**Files:** `crates/core/src/llm/gateway.rs:250-257`

**Change:** Extend cache control to last 3 messages + last tool, matching OpenCode.

Current: Only system prompt gets cache_control (when `set_cache_key && system_prompt.len() > 1000`).
New: Always apply cache_control to system prompt + last 3 messages + last tool when `set_cache_key` is true:

```rust
if resolved.set_cache_key {
    // System prompt as array with cache_control
    body["system"] = serde_json::json!([{
        "type": "text",
        "text": request.system_prompt,
        "cache_control": { "type": "ephemeral" }
    }]);
    
    // Apply cache_control to last 3 messages
    if let Some(arr) = body["messages"].as_array_mut() {
        let msg_count = arr.len();
        for (i, msg) in arr.iter_mut().enumerate() {
            if i >= msg_count.saturating_sub(3) {
                if let Some(obj) = msg.as_object_mut() {
                    obj.insert("cache_control".into(), serde_json::json!({"type": "ephemeral"}));
                }
            }
        }
    }
    
    // Apply cache_control to last tool (tools already use Anthropic format)
    if let Some(tools) = body["tools"].as_array_mut() {
        if let Some(last_tool) = tools.last_mut() {
            if let Some(obj) = last_tool.as_object_mut() {
                obj.insert("cache_control".into(), serde_json::json!({"type": "ephemeral"}));
            }
        }
    }
}
```

Note: No tool format changes needed �?the current Anthropic tool objects (`{"name":..., "description":..., "input_schema":...}`) already support adding `cache_control` as an additional field.

### Task 2.4: Parse finish reason

**Files:** `crates/core/src/llm/gateway.rs:58-63` (ChatResponse), plus response parsing in all 3 methods

**Change:** Add `stop_reason` to `ChatResponse` struct and parse it after each API call.

In `ChatResponse` struct (gateway.rs line 58-63), add:
```rust
#[derive(Debug, Clone)]
pub struct ChatResponse {
    pub content: String,
    pub tool_calls: Vec<ToolCall>,
    pub usage: TokenUsage,
    pub reasoning_content: Option<String>,
    pub stop_reason: Option<String>,  // NEW
}
```

In `call_anthropic_resolved` after response parsing:
```rust
// Anthropic: "end_turn" | "tool_use" | "max_tokens" | "stop_sequence"
let stop_reason = json["stop_reason"].as_str().map(|s| s.to_string());
```

In `call_openai_compat_resolved` after response parsing:
```rust
// OpenAI: "stop" | "length" | "tool_calls" | "content_filter"
let stop_reason = json["choices"][0]["finish_reason"].as_str().map(|s| s.to_string());
```

In `call_ollama_resolved` after response parsing:
```rust
// Ollama: "stop" | "length"
let stop_reason = json["done_reason"].as_str().map(|s| s.to_string());
```

### Task 2.5: Additional OpenAI-compatible providers

**Files:** `crates/core/src/llm/models.rs` + `crates/core/src/llm/provider.rs` (resolve method)

**Change:** Add GROQ, OpenRouter, xAI as built-in provider defaults.

In `models.rs`, add to `builtin_provider_defaults()`:
```rust
ProviderDefaults {
    name: "groq".into(),
    base_url: "https://api.groq.com/openai/v1".into(),
    api_key_env: "GROQ_API_KEY".into(),
    env_vars: vec!["GROQ_API_KEY".into()],
    timeout_ms: 300000,
},
ProviderDefaults {
    name: "openrouter".into(),
    base_url: "https://openrouter.ai/api/v1".into(),
    api_key_env: "OPENROUTER_API_KEY".into(),
    env_vars: vec!["OPENROUTER_API_KEY".into()],
    timeout_ms: 300000,
},
ProviderDefaults {
    name: "xai".into(),
    base_url: "https://api.x.ai/v1".into(),
    api_key_env: "XAI_API_KEY".into(),
    env_vars: vec!["XAI_API_KEY".into()],
    timeout_ms: 300000,
},
```

Update `chat()` dispatch to add them all to the OpenAI-compat path:
```rust
"deepseek" | "openai" | "groq" | "openrouter" | "xai" => self.call_openai_compat_resolved(...).await,
```

In `call_openai_compat_resolved`, add OpenRouter-specific headers:
```rust
if resolved.provider == "openrouter" {
    req = req.header("HTTP-Referer", "https://github.com/MoonlitDropOfBlood/OpenTeam");
    req = req.header("X-Title", "OpenTeam");
}
```

---

## Phase 3: Streaming (Major Architecture)

> **Note:** Streaming is a significant architectural change. This plan provides the high-level design only.

### Task 3.0: Caller Adaptation — consuming StreamEvent

**Files:** `crates/core/src/agent/manager.rs:255-331`, `crates/core/src/assistant/assistant.rs:162`

**Change:** Adapt callers to consume streaming events instead of a single ChatResponse.

**agent/manager.rs** currently has a synchronous tool execution loop:
```rust
let request = ChatRequest { ... };
let response = llm_gateway.chat(model_config, &request).await?;
while !response.tool_calls.is_empty() {
    // execute tools, build new request, chat again...
}
```

With streaming, this becomes:
```rust
let request = ChatRequest { ... };
let mut stream = llm_gateway.chat_stream(model_config, &request).await?;
let mut content = String::new();
let mut reasoning = String::new();
let mut tool_calls = Vec::new();
let mut usage = None;

while let Some(event) = stream.recv().await {
    match event {
        StreamEvent::TextDelta { content: c } => content.push_str(&c),
        StreamEvent::ReasoningDelta { content: c } => reasoning.push_str(&c),
        StreamEvent::ToolCallStart { id, name } => tool_calls.push(ToolCall { id, name, arguments: Value::Null }),
        StreamEvent::ToolCallDelta { id, arguments: a } => { /* accumulate partial JSON */ }
        StreamEvent::ToolCallStop { id } => { /* finalize tool call */ }
        StreamEvent::Usage { input_tokens, output_tokens } => usage = Some(TokenUsage { ... }),
        StreamEvent::Stop { .. } => break,
        StreamEvent::Error { message, retryable } => return Err(...),
        StreamEvent::Done => break,
    }
}
```

Then `tool_calls` iteration and execution stays the same — the loop processes the assembled `Vec<ToolCall>`.

**assistant/assistant.rs** currently uses `?` propagation on `chat()`:
```rust
asst.process_message(message, sender, &self.llm_gateway, model).await
// Inside: let response = llm_gateway.chat(...).await?;
```

For streaming, change to:
```rust
let mut stream = llm_gateway.chat_stream(...).await?;
// ... consume events, build ChatResponse ...
// Then use ChatResponse as before
```

Or keep a `chat_async()` wrapper that internally consumes the stream and returns `ChatResponse`.

### Task 3.1: SSE streaming event types

**Create:** `crates/core/src/llm/stream.rs`

Define streaming events matching OpenCode's ProviderEvent:

```rust
#[derive(Debug, Clone)]
pub enum StreamEvent {
    TextDelta { content: String },
    ReasoningDelta { content: String },
    ToolCallStart { id: String, name: String },
    ToolCallDelta { id: String, arguments: String },
    ToolCallStop { id: String },
    Usage { input_tokens: u32, output_tokens: u32 },
    Stop { stop_reason: String },
    Error { message: String, retryable: bool },
    Done,
}
```

### Task 3.2: SSE streaming for OpenAI-compat

**Files:** `crates/core/src/llm/gateway.rs`

Add method:
```rust
pub async fn chat_stream(
    &self,
    model_config: &ModelConfig,
    request: &ChatRequest,
) -> Result<tokio::sync::mpsc::Receiver<StreamEvent>, CoreError> {
    let (tx, rx) = tokio::sync::mpsc::channel(1024);
    let resolved = self.resolve_model(model_config);
    // ... build request with stream: true ...
    // ... spawn tokio task to read SSE lines and emit events ...
    Ok(rx)
}
```

SSE line parsing:
```rust
// For OpenAI-compat streaming:
// data: {"choices":[{"delta":{"content":"Hello"},"index":0}]}
// data: [DONE]
let line = String::from_utf8_lossy(&chunk);
if line.starts_with("data: ") {
    let json_str = line.trim_start_matches("data: ");
    if json_str == "[DONE]" {
        let _ = tx.send(StreamEvent::Done).await;
        break;
    }
    // Parse delta...
}
```

### Task 3.3: SSE streaming for Anthropic

Same pattern as OpenAI but with Anthropic event format:
```
event: content_block_delta
data: {"type":"text_delta","text":"Hello"}
event: content_block_stop
data: {"index":0}
```

### Task 3.4: SSE streaming for Ollama

Ollama streaming JSON lines:
```json
{"message":{"content":"Hello"},"done":false}
{"message":{"content":""},"done":true,"total_duration":...}
```

---

## Test Plan

### Phase 1 Tests

Add to existing test files:

| Test | File | What it verifies |
|------|------|-----------------|
| `test_max_completion_tokens_for_reasoning` | `config/mod.rs` | Reasoning model sends `max_completion_tokens` |
| `test_max_tokens_for_non_reasoning` | `config/mod.rs` | Non-reasoning model sends `max_tokens` |
| `test_anthropic_input_tokens_with_cache` | `gateway.rs` | input_tokens = raw + cache_read + cache_create |
| `test_openai_usage_with_details` | `gateway.rs` | cached_tokens + reasoning_tokens extracted |
| `test_retry_with_jitter` | `gateway.rs` | Different delays on each retry |

### Phase 2 Tests

| Test | File | What it verifies |
|------|------|-----------------|
| `test_anthropic_thinking_blocks_parsed` | `gateway.rs` | thinking blocks become reasoning_content |
| `test_anthropic_cache_control_last_messages` | `gateway.rs` | Last 3 messages get cache_control |
| `test_anthropic_cache_control_last_tool` | `gateway.rs` | Last tool gets cache_control |
| `test_finish_reason_anthropic` | `gateway.rs` | `stop_reason` parsed from response |
| `test_finish_reason_openai` | `gateway.rs` | `finish_reason` parsed |
| `test_groq_provider_defaults` | `models.rs` | GROQ baseURL and api_key_env |
| `test_openrouter_provider_defaults` | `models.rs` | OpenRouter baseURL + headers |
| `test_structured_error_variants` | `error.rs` | Error type assignment by HTTP status |

---

## Execution Order

```
Phase 1: Correctness
├── Task 1.1: max_completion_tokens (1 file, ~10 lines)
├── Task 1.2: Anthropic input_tokens + TokenUsage (2 files, ~30 lines)
└── Task 1.3: Retry jitter + Retry-After + structured error types (3 files, ~90 lines)

Phase 2: Structural Parity  
├── Task 2.1: Extended TokenUsage callers (2 files, ~10 lines)
├── Task 2.2: Anthropic thinking blocks (1 file, ~15 lines)
├── Task 2.3: Extended prompt caching (1 file, ~30 lines)
├── Task 2.4: Finish reason (1 file, ~10 lines)
└── Task 2.5: Additional providers (2 files, ~40 lines)

Phase 3: Streaming (Future)
├── Task 3.0: Caller adaptation (2 files, ~100 lines)
├── Task 3.2: OpenAI SSE parser (gateway.rs, ~80 lines)
├── Task 3.3: Anthropic SSE parser (gateway.rs, ~60 lines)
└── Task 3.4: Ollama SSE parser (gateway.rs, ~30 lines)
```