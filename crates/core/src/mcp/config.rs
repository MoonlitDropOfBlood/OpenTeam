use std::collections::HashMap;
use std::path::Path;
use serde::Deserialize;
use crate::CoreError;

#[derive(Debug, Clone, Deserialize)]
pub struct McpServerConfig {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default)]
    pub tools: Vec<ToolDefinition>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// Re-export for gateway use
pub use ToolDefinition as McpToolDef;

/// Discover MCP server configs from a directory (each subdirectory contains mcp.json)
pub fn discover_mcp_configs(dir: &Path) -> Result<Vec<McpServerConfig>, CoreError> {
    let mut configs = Vec::new();
    if !dir.exists() {
        return Ok(configs);
    }

    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let config_path = path.join("mcp.json");
        if !config_path.exists() {
            continue;
        }

        let content = std::fs::read_to_string(&config_path)?;
        match serde_json::from_str::<McpServerConfig>(&content) {
            Ok(config) => {
                tracing::info!(
                    "Discovered MCP server: {} ({})",
                    config.name,
                    config.description
                );
                configs.push(config);
            }
            Err(e) => {
                tracing::warn!("Failed to load MCP config from {:?}: {e}", config_path);
            }
        }
    }
    Ok(configs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_discover_empty_dir() {
        let tmp = std::env::temp_dir().join("feishu_mcp_empty_test");
        let _ = std::fs::create_dir_all(&tmp);
        let configs = discover_mcp_configs(&tmp).unwrap();
        assert!(configs.is_empty());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_discover_mcp_config() {
        let tmp = std::env::temp_dir().join("feishu_mcp_discover_test");
        let server_dir = tmp.join("github");
        let _ = std::fs::create_dir_all(&server_dir);

        let json = r#"{
            "name": "github",
            "description": "GitHub API",
            "command": "node",
            "args": ["server.js"],
            "env": {"GITHUB_TOKEN": "${GITHUB_TOKEN}"},
            "tools": [
                {
                    "name": "create_issue",
                    "description": "Create a GitHub issue",
                    "input_schema": {
                        "type": "object",
                        "properties": {
                            "repo": {"type": "string"},
                            "title": {"type": "string"}
                        },
                        "required": ["repo", "title"]
                    }
                }
            ]
        }"#;
        let mut f = std::fs::File::create(&server_dir.join("mcp.json")).unwrap();
        f.write_all(json.as_bytes()).unwrap();

        let configs = discover_mcp_configs(&tmp).unwrap();
        assert_eq!(configs.len(), 1);
        assert_eq!(configs[0].name, "github");
        assert_eq!(configs[0].tools.len(), 1);
        assert_eq!(configs[0].tools[0].name, "create_issue");
        assert_eq!(configs[0].tools[0].input_schema["type"], "object");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_discover_invalid_json_warns() {
        let tmp = std::env::temp_dir().join("feishu_mcp_invalid_test");
        let server_dir = tmp.join("bad-server");
        let _ = std::fs::create_dir_all(&server_dir);

        let mut f = std::fs::File::create(&server_dir.join("mcp.json")).unwrap();
        f.write_all(b"not json").unwrap();

        let configs = discover_mcp_configs(&tmp).unwrap();
        assert!(configs.is_empty());

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
