use std::collections::HashMap;
use tokio::process::Command;
use crate::llm::gateway::ToolCall;
use crate::CoreError;
use super::config::McpServerConfig;

/// Execute an MCP tool call by spawning the corresponding server process.
///
/// Sends a JSON-RPC `tools/call` request via stdin and reads the result from stdout.
/// Uses a 30-second timeout to prevent stalled processes from blocking the agent loop.
pub async fn execute_tool(
    tool_call: &ToolCall,
    server_name: &str,
    servers: &HashMap<String, McpServerConfig>,
) -> Result<String, CoreError> {
    let server = servers
        .get(server_name)
        .ok_or_else(|| CoreError::Mcp(format!("Unknown MCP server: {server_name}")))?;

    tracing::info!(
        "[MCP] Executing {}.{} with args: {:?}",
        server_name,
        tool_call.name,
        tool_call.arguments,
    );

    // Build JSON-RPC request for the tool call
    // MCP protocol: send a request with method "tools/call"
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": tool_call.name,
            "arguments": tool_call.arguments,
        },
    });

    // Spawn the MCP server process
    let mut cmd = Command::new(&server.command);
    for arg in &server.args {
        cmd.arg(arg);
    }
    for (key, val) in &server.env {
        let resolved = if val.starts_with("${") && val.ends_with('}') {
            let var_name = &val[2..val.len() - 1];
            std::env::var(var_name).unwrap_or_default()
        } else {
            val.clone()
        };
        cmd.env(key, resolved);
    }

    let mut child = cmd
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::inherit())
        .spawn()
        .map_err(|e| CoreError::Mcp(format!("Failed to spawn {}: {e}", server_name)))?;

    // Write request to stdin
    if let Some(stdin) = child.stdin.as_mut() {
        use tokio::io::AsyncWriteExt;
        let request_str = serde_json::to_string(&request)
            .map_err(|e| CoreError::Mcp(format!("Serialize JSON-RPC request: {e}")))?;
        stdin
            .write_all(request_str.as_bytes())
            .await
            .map_err(|e| CoreError::Mcp(format!("Write to stdin of {}: {e}", server_name)))?;
        // Send a newline to signal end of request
        stdin.write_all(b"\n").await.ok();
    }

    // Read response from stdout with timeout
    use tokio::io::AsyncBufReadExt;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| CoreError::Mcp(format!("No stdout for {}", server_name)))?;
    let mut reader = tokio::io::BufReader::new(stdout);
    let mut response_line = String::new();

    tokio::time::timeout(
        std::time::Duration::from_secs(30),
        reader.read_line(&mut response_line),
    )
    .await
    .map_err(|_| CoreError::Mcp(format!("{server_name} tool execution timed out after 30s")))?
    .map_err(|e| CoreError::Mcp(format!("Read stdout from {server_name}: {e}")))?;

    // Wait for process to exit (clean up)
    let _ = child.wait().await;

    // Parse JSON-RPC response
    let resp: serde_json::Value = serde_json::from_str(&response_line)
        .map_err(|e| CoreError::Mcp(format!("Parse response from {server_name}: {e}\nRaw: {response_line}")))?;

    // Extract result content — try multiple known formats
    let result = resp["result"]["content"]
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|c| c["text"].as_str())
        .or_else(|| resp["result"].as_str())
        .or_else(|| resp["result"]["content"].as_str())
        .unwrap_or(&response_line)
        .to_string();

    tracing::info!(
        "[MCP] {}.{} result ({} chars): {}",
        server_name,
        tool_call.name,
        result.len(),
        &result[..result.len().min(100)],
    );

    Ok(result)
}
