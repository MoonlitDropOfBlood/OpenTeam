use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use crate::llm::gateway::ToolCall;
use crate::CoreError;
use super::config::McpServerConfig;

/// MCP process cache with 30s reuse window
static PROCESS_CACHE: once_cell::sync::Lazy<Mutex<HashMap<String, CachedProcess>>> =
    once_cell::sync::Lazy::new(|| Mutex::new(HashMap::new()));

struct CachedProcess {
    #[allow(dead_code)]
    child: Option<tokio::process::Child>,
    last_used: Instant,
}

/// Check if we can reuse a cached process (within 30s since last use)
pub fn can_reuse_process(server_name: &str) -> bool {
    let cache = PROCESS_CACHE.lock().unwrap();
    if let Some(entry) = cache.get(server_name) {
        entry.last_used.elapsed() < Duration::from_secs(30)
    } else {
        false
    }
}

/// Track that a process was used for this server; store child handle for reuse
pub fn track_process_spawn(server_name: &str, child: Option<tokio::process::Child>) {
    let mut cache = PROCESS_CACHE.lock().unwrap();
    // Drop previous child if it exists (will wait for exit)
    if let Some(prev) = cache.remove(server_name) {
        drop(prev);
    }
    cache.insert(
        server_name.to_string(),
        CachedProcess {
            child,
            last_used: Instant::now(),
        },
    );
}

/// Execute a tool call. Checks built-in tools first, then external MCP servers.
pub async fn execute_tool(
    tool_call: &ToolCall,
    server_name: &str,
    servers: &HashMap<String, McpServerConfig>,
) -> Result<String, CoreError> {
    // Check if this is a built-in tool (runs in-process, no subprocess)
    if super::builtin::is_builtin(&tool_call.name) {
        return super::builtin::execute_builtin(&tool_call.name, &tool_call.arguments).await;
    }

    // Otherwise, route to external MCP server
    let server = servers
        .get(server_name)
        .ok_or_else(|| CoreError::Mcp(format!("Unknown MCP server: {server_name}")))?;

    // Check process pool: reuse if within 30s window
    let _cached = {
        let cache = PROCESS_CACHE.lock().unwrap();
        cache.get(server_name).and_then(|c| {
            if c.last_used.elapsed() < Duration::from_secs(30) {
                tracing::debug!(
                    "[MCP] Reusing cached process for {} (last used {:?}s ago)",
                    server_name,
                    c.last_used.elapsed().as_secs(),
                );
                // Phase 3 V3: send next JSON-RPC request via stored stdin handle
                Some(())
            } else {
                None
            }
        })
    };

    tracing::info!(
        "[MCP] Executing {}.{} with args: {:?}",
        server_name, tool_call.name, tool_call.arguments,
    );

    // Track this spawn in the process pool (child=None for now; Phase 3 V4: store handle)
    track_process_spawn(server_name, None);

    // Send JSON-RPC tools/call via the shared transport (stdio or HTTP)
    let resp = super::config::send_jsonrpc(
        server,
        "tools/call",
        serde_json::json!({
            "name": tool_call.name,
            "arguments": tool_call.arguments,
        }),
    ).await?;

    // Extract result content — try multiple known formats
    let result = resp["result"]["content"]
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|c| c["text"].as_str())
        .or_else(|| resp["result"].as_str())
        .or_else(|| resp["result"]["content"].as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| serde_json::to_string(&resp).unwrap_or_default());

    tracing::info!(
        "[MCP] {}.{} result ({} chars): {}",
        server_name, tool_call.name, result.len(),
        &result[..result.len().min(100)],
    );

    Ok(result)
}
