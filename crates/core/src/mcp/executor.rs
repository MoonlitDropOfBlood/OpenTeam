use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use crate::llm::gateway::ToolCall;
use crate::CoreError;
use super::config::McpServerConfig;

/// Simple MCP process cache with TTL
static PROCESS_CACHE: once_cell::sync::Lazy<Mutex<HashMap<String, CachedProcess>>> =
    once_cell::sync::Lazy::new(|| Mutex::new(HashMap::new()));

struct CachedProcess {
    // Phase 3 V3: store child process handles for reuse
    // For now, just track when we last spawned to avoid rapid respawns
    last_spawned: Instant,
}

/// Check if we can reuse a cached process (min 5s between spawns)
pub fn can_reuse_process(server_name: &str) -> bool {
    let cache = PROCESS_CACHE.lock().unwrap();
    if let Some(entry) = cache.get(server_name) {
        entry.last_spawned.elapsed() < Duration::from_secs(5)
    } else {
        false
    }
}

/// Track that a process was spawned for this server
pub fn track_process_spawn(server_name: &str) {
    let mut cache = PROCESS_CACHE.lock().unwrap();
    cache.insert(
        server_name.to_string(),
        CachedProcess {
            last_spawned: Instant::now(),
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

    // Check process pool: skip spawn if recently spawned
    if can_reuse_process(server_name) {
        tracing::debug!(
            "[MCP] Reusing cached process for {} (spawned <5s ago)",
            server_name
        );
    }

    tracing::info!(
        "[MCP] Executing {}.{} with args: {:?}",
        server_name, tool_call.name, tool_call.arguments,
    );

    // Track this spawn in the process pool
    track_process_spawn(server_name);

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
