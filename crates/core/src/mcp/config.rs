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

/// Load MCP server configs from a single mcps.json file (JSON array)
pub fn load_mcp_configs(path: &Path) -> Result<Vec<McpServerConfig>, CoreError> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = std::fs::read_to_string(path)?;
    let configs: Vec<McpServerConfig> = serde_json::from_str(&content)
        .map_err(|e| CoreError::Mcp(format!("Parse {:?}: {e}", path)))?;
    for cfg in &configs {
        tracing::info!("Discovered MCP server: {} ({})", cfg.name, cfg.description);
    }
    Ok(configs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_load_empty_file() {
        let tmp = std::env::temp_dir().join("feishu_mcp_empty_test");
        let _ = std::fs::create_dir_all(&tmp);
        let path = tmp.join("mcps.json");
        let configs = load_mcp_configs(&path).unwrap();
        assert!(configs.is_empty());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_load_mcp_configs() {
        let tmp = std::env::temp_dir().join("feishu_mcp_load_test");
        let _ = std::fs::create_dir_all(&tmp);
        let path = tmp.join("mcps.json");

        let json = r#"[
            {
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
            },
            {
                "name": "postgres",
                "description": "PostgreSQL",
                "command": "python",
                "args": [],
                "env": {},
                "tools": [
                    {
                        "name": "query",
                        "description": "Run SQL query",
                        "input_schema": {}
                    }
                ]
            }
        ]"#;
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(json.as_bytes()).unwrap();

        let configs = load_mcp_configs(&path).unwrap();
        assert_eq!(configs.len(), 2);
        assert_eq!(configs[0].name, "github");
        assert_eq!(configs[0].tools.len(), 1);
        assert_eq!(configs[1].name, "postgres");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_load_invalid_json_warns() {
        let tmp = std::env::temp_dir().join("feishu_mcp_invalid_test");
        let _ = std::fs::create_dir_all(&tmp);
        let path = tmp.join("mcps.json");
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(b"not json").unwrap();
        let result = load_mcp_configs(&path);
        assert!(result.is_err());
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
