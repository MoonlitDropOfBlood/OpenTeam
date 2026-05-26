use std::collections::HashMap;
use std::path::Path;
use serde::{Deserialize, Serialize};
use crate::CoreError;

/// Top-level structure of an mcp.json file
#[derive(Debug, Clone, Deserialize)]
struct McpConfigFile {
    #[serde(default, rename = "mcpServers")]
    pub mcp_servers: HashMap<String, McpServerEntry>,
}

/// A single server entry in the mcp.json config
#[derive(Debug, Clone, Deserialize)]
pub struct McpServerEntry {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    /// Default: enabled. Set `"enabled": false` to skip.
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_enabled() -> bool {
    true
}

/// Full server config: name + entry + probed tools
#[derive(Debug, Clone)]
pub struct McpServerConfig {
    pub name: String,
    pub entry: McpServerEntry,
    pub tools: Vec<ToolDefinition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// Re-export for gateway use
pub use ToolDefinition as McpToolDef;

/// Load MCP server configs from a standard mcp.json file
pub fn load_mcp_configs(path: &Path) -> Result<Vec<McpServerConfig>, CoreError> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = std::fs::read_to_string(path)?;
    let file: McpConfigFile = serde_json::from_str(&content)
        .map_err(|e| CoreError::Mcp(format!("Parse {:?}: {e}", path)))?;

    let mut configs = Vec::new();
    for (name, entry) in file.mcp_servers {
        if !entry.enabled {
            tracing::info!("MCP server '{}' is disabled, skipping", name);
            continue;
        }
        configs.push(McpServerConfig {
            name: name.clone(),
            entry,
            tools: Vec::new(), // populated later via probe
        });
        tracing::info!("Discovered MCP server: {}", name);
    }
    Ok(configs)
}

/// Probe an MCP server for its tool list by spawning it and calling tools/list
pub async fn probe_server_tools(config: &McpServerConfig) -> Result<Vec<ToolDefinition>, CoreError> {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::process::Command;

    let mut cmd = Command::new(&config.entry.command);
    for arg in &config.entry.args {
        cmd.arg(arg);
    }
    for (key, val) in &config.entry.env {
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
        .map_err(|e| CoreError::Mcp(format!("Spawn {}: {e}", config.name)))?;

    // Send tools/list request
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/list",
        "params": {}
    });
    if let Some(stdin) = child.stdin.as_mut() {
        let req_str = serde_json::to_string(&request)
            .map_err(|e| CoreError::Mcp(format!("Serialize: {e}")))?;
        stdin
            .write_all(req_str.as_bytes())
            .await
            .map_err(|e| CoreError::Mcp(format!("Write stdin: {e}")))?;
        stdin.write_all(b"\n").await.ok();
    }

    // Read response with timeout
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| CoreError::Mcp("No stdout".into()))?;
    let mut reader = BufReader::new(stdout);
    let mut line = String::new();

    tokio::time::timeout(
        std::time::Duration::from_secs(15),
        reader.read_line(&mut line),
    )
    .await
    .map_err(|_| CoreError::Mcp(format!("{} tools/list timed out", config.name)))?
    .map_err(|e| CoreError::Mcp(format!("Read stdout: {e}")))?;

    let _ = child.wait().await;

    let resp: serde_json::Value = serde_json::from_str(&line)
        .map_err(|e| CoreError::Mcp(format!("Parse: {e}\nRaw: {line}")))?;

    let tools_raw = resp["result"]["tools"]
        .as_array()
        .ok_or_else(|| CoreError::Mcp(format!("No tools in response from {}", config.name)))?;

    let tools: Vec<ToolDefinition> = tools_raw
        .iter()
        .map(|t| ToolDefinition {
            name: t["name"].as_str().unwrap_or("unknown").to_string(),
            description: t["description"].as_str().unwrap_or("").to_string(),
            input_schema: t["input_schema"].clone(),
        })
        .collect();

    tracing::info!(
        "Probed MCP server '{}' — found {} tool(s)",
        config.name,
        tools.len()
    );
    Ok(tools)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_parse_standard_format() {
        let tmp = std::env::temp_dir().join("feishu_mcp_std_test");
        let _ = std::fs::create_dir_all(&tmp);
        let path = tmp.join("mcp.json");

        let json = r#"{
            "mcpServers": {
                "github": {
                    "command": "node",
                    "args": ["server.js"],
                    "env": {"TOKEN": "abc"},
                    "enabled": true
                },
                "postgres": {
                    "command": "python",
                    "args": ["pg.py"],
                    "enabled": false
                }
            }
        }"#;
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(json.as_bytes()).unwrap();

        let configs = load_mcp_configs(&path).unwrap();
        assert_eq!(configs.len(), 1, "Only enabled servers should load");
        assert_eq!(configs[0].name, "github");
        assert_eq!(configs[0].entry.command, "node");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_empty_config() {
        let tmp = std::env::temp_dir().join("feishu_mcp_empty_std_test");
        let _ = std::fs::create_dir_all(&tmp);
        let path = tmp.join("mcp.json");
        let configs = load_mcp_configs(&path).unwrap();
        assert!(configs.is_empty());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_all_enabled_by_default() {
        let tmp = std::env::temp_dir().join("feishu_mcp_default_enabled_test");
        let _ = std::fs::create_dir_all(&tmp);
        let path = tmp.join("mcp.json");

        let json = r#"{
            "mcpServers": {
                "github": {
                    "command": "node",
                    "args": []
                }
            }
        }"#;
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(json.as_bytes()).unwrap();

        let configs = load_mcp_configs(&path).unwrap();
        assert_eq!(configs.len(), 1, "Server should be enabled by default");
        assert!(configs[0].entry.enabled);

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
