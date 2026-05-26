use std::collections::HashMap;
use crate::llm::gateway::ToolCall;
use crate::CoreError;
use super::config::McpServerConfig;

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

    tracing::info!(
        "[MCP] Executing {}.{} with args: {:?}",
        server_name, tool_call.name, tool_call.arguments,
    );

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
