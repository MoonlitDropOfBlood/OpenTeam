use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use crate::CoreError;
use super::config::*;

/// Manages all discovered MCP tool definitions
#[derive(Clone)]
pub struct McpRegistry {
    servers: HashMap<String, McpServerConfig>,
}

impl McpRegistry {
    pub fn new() -> Self {
        Self {
            servers: HashMap::new(),
        }
    }

    /// Load all MCP configs from multiple directories and merge
    pub fn discover_all(dirs: &[PathBuf]) -> Result<Self, CoreError> {
        let mut registry = Self::new();
        for dir in dirs {
            let configs = discover_mcp_configs(dir)?;
            for cfg in configs {
                registry.servers.entry(cfg.name.clone()).or_insert(cfg);
            }
        }
        Ok(registry)
    }

    /// Get all tool definitions for agents that have access to these MCP servers
    pub fn get_all_tools(&self) -> Vec<ToolDefinition> {
        self.servers
            .values()
            .flat_map(|s| s.tools.clone())
            .collect()
    }

    /// Get tool definitions for specific named MCP servers
    pub fn get_tools_for(&self, names: &[String]) -> Vec<ToolDefinition> {
        names
            .iter()
            .filter_map(|n| self.servers.get(n))
            .flat_map(|s| s.tools.clone())
            .collect()
    }

    pub fn list_servers(&self) -> Vec<&McpServerConfig> {
        self.servers.values().collect()
    }

    /// Start a file watcher for hot-reload (same pattern as SkillRegistry)
    pub fn start_watcher(
        registry: Arc<RwLock<Self>>,
        watch_dirs: Vec<PathBuf>,
    ) -> Result<tokio::task::JoinHandle<()>, CoreError> {
        use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
        use std::sync::mpsc;

        let (tx, rx) = mpsc::channel::<Result<notify::Event, notify::Error>>();
        let mut watcher = RecommendedWatcher::new(tx, Config::default())
            .map_err(|e| CoreError::Mcp(format!("Create watcher: {e}")))?;

        for dir in &watch_dirs {
            if dir.exists() {
                watcher
                    .watch(dir, RecursiveMode::Recursive)
                    .map_err(|e| CoreError::Mcp(format!("Watch {:?}: {e}", dir)))?;
                tracing::info!("[MCP Watcher] Watching: {:?}", dir);
            }
        }

        let handle = tokio::task::spawn_blocking(move || {
            let _watcher = watcher;
            for res in rx {
                match res {
                    Ok(event) => {
                        let is_config_change = matches!(
                            event.kind,
                            EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
                        ) && event.paths.iter().any(|p| {
                            p.file_name()
                                .and_then(|n| n.to_str())
                                == Some("mcp.json")
                        });

                        if is_config_change {
                            tracing::info!(
                                "[MCP Watcher] mcp.json changed, hot-reloading..."
                            );
                            let mut reg = registry.blocking_write();
                            reg.servers.clear();
                            for dir in &watch_dirs {
                                if dir.exists() {
                                    if let Ok(configs) = discover_mcp_configs(dir) {
                                        for cfg in configs {
                                            reg.servers
                                                .entry(cfg.name.clone())
                                                .or_insert(cfg);
                                        }
                                    }
                                }
                            }
                            tracing::info!(
                                "[MCP Watcher] Hot-reload complete — {} servers",
                                reg.servers.len()
                            );
                        }
                    }
                    Err(e) => tracing::error!("[MCP Watcher] Error: {e}"),
                }
            }
        });

        Ok(handle)
    }
}

fn home_dir() -> std::path::PathBuf {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
}

/// Get the global MCP configs directory path (~/.config/OpenTeam/mcps/)
pub fn global_mcp_dir() -> std::path::PathBuf {
    home_dir().join(".config/OpenTeam/mcps")
}

/// Get the assistant MCP configs directory path (~/.config/OpenTeam/assistant/mcps/)
pub fn assistant_mcp_dir() -> std::path::PathBuf {
    home_dir().join(".config/OpenTeam/assistant/mcps")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_discover_all_multiple_dirs() {
        let tmp1 = std::env::temp_dir().join("feishu_mcp_registry_test1");
        let tmp2 = std::env::temp_dir().join("feishu_mcp_registry_test2");
        let server_dir1 = tmp1.join("github");
        let server_dir2 = tmp2.join("postgres");
        let _ = std::fs::create_dir_all(&server_dir1);
        let _ = std::fs::create_dir_all(&server_dir2);

        let json1 = r#"{
            "name": "github",
            "description": "GitHub API",
            "command": "node",
            "args": [],
            "env": {},
            "tools": [{"name": "create_issue", "description": "Create issue", "input_schema": {}}]
        }"#;
        let json2 = r#"{
            "name": "postgres",
            "description": "PostgreSQL",
            "command": "python",
            "args": [],
            "env": {},
            "tools": [{"name": "query", "description": "Run query", "input_schema": {}}]
        }"#;

        let mut f1 = std::fs::File::create(&server_dir1.join("mcp.json")).unwrap();
        f1.write_all(json1.as_bytes()).unwrap();
        let mut f2 = std::fs::File::create(&server_dir2.join("mcp.json")).unwrap();
        f2.write_all(json2.as_bytes()).unwrap();

        let registry = McpRegistry::discover_all(&[tmp1.clone(), tmp2.clone()]).unwrap();
        assert_eq!(registry.list_servers().len(), 2);
        assert_eq!(registry.get_tools_for(&["github".to_string()]).len(), 1);
        assert_eq!(
            registry.get_tools_for(&["postgres".to_string()])[0].name,
            "query"
        );

        let _ = std::fs::remove_dir_all(&tmp1);
        let _ = std::fs::remove_dir_all(&tmp2);
    }

    #[test]
    fn test_get_all_tools() {
        let tmp = std::env::temp_dir().join("feishu_mcp_alltools_test");
        let server_dir = tmp.join("github");
        let _ = std::fs::create_dir_all(&server_dir);

        let json = r#"{
            "name": "github",
            "description": "",
            "command": "node",
            "args": [],
            "env": {},
            "tools": [
                {"name": "create_issue", "description": "a", "input_schema": {}},
                {"name": "search_code", "description": "b", "input_schema": {}}
            ]
        }"#;
        let mut f = std::fs::File::create(&server_dir.join("mcp.json")).unwrap();
        f.write_all(json.as_bytes()).unwrap();

        let registry = McpRegistry::discover_all(&[tmp.clone()]).unwrap();
        assert_eq!(registry.get_all_tools().len(), 2);

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
