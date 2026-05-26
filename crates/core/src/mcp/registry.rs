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

    /// Load MCP configs from multiple mcps.json file paths and merge
    pub fn discover_all(config_paths: &[PathBuf]) -> Result<Self, CoreError> {
        let mut registry = Self::new();
        for path in config_paths {
            let configs = load_mcp_configs(path)?;
            for cfg in configs {
                registry.servers.entry(cfg.name.clone()).or_insert(cfg);
            }
        }
        Ok(registry)
    }

    /// Get all tool definitions — built-in tools + all discovered MCP servers
    pub fn get_all_tools(&self) -> Vec<ToolDefinition> {
        let mut tools = super::builtin::builtin_tool_definitions();
        for server in self.servers.values() {
            tools.extend(server.tools.clone());
        }
        tools
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

    /// Start a file watcher for hot-reload (watches parent dirs of mcps.json files)
    pub fn start_watcher(
        registry: Arc<RwLock<Self>>,
        watch_files: Vec<PathBuf>,
    ) -> Result<tokio::task::JoinHandle<()>, CoreError> {
        use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
        use std::sync::mpsc;

        let (tx, rx) = mpsc::channel::<Result<notify::Event, notify::Error>>();
        let mut watcher = RecommendedWatcher::new(tx, Config::default())
            .map_err(|e| CoreError::Mcp(format!("Create watcher: {e}")))?;

        // Watch the parent directory of each mcps.json file
        let mut watched_parents: Vec<PathBuf> = Vec::new();
        for f in &watch_files {
            if let Some(parent) = f.parent() {
                if parent.exists() && !watched_parents.iter().any(|p| p == parent) {
                    watcher
                        .watch(parent, RecursiveMode::NonRecursive)
                        .map_err(|e| CoreError::Mcp(format!("Watch {:?}: {e}", parent)))?;
                    watched_parents.push(parent.to_path_buf());
                    tracing::info!("[MCP Watcher] Watching: {:?} for mcps.json changes", parent);
                }
            }
        }

        let watch_files_clone = watch_files.clone();
        let handle = tokio::task::spawn_blocking(move || {
            let _watcher = watcher;
            for res in rx {
                match res {
                    Ok(event) => {
                        let is_mcps_change = matches!(
                            event.kind,
                            EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
                        ) && event.paths.iter().any(|p| {
                            p.file_name().and_then(|n| n.to_str()) == Some("mcps.json")
                        });

                        if is_mcps_change {
                            tracing::info!("[MCP Watcher] mcps.json changed, hot-reloading...");
                            let mut reg = registry.blocking_write();
                            reg.servers.clear();
                            for path in &watch_files_clone {
                                if path.exists() {
                                    if let Ok(configs) = load_mcp_configs(path) {
                                        for cfg in configs {
                                            reg.servers.entry(cfg.name.clone()).or_insert(cfg);
                                        }
                                    }
                                }
                            }
                            tracing::info!("[MCP Watcher] Hot-reload complete — {} servers", reg.servers.len());
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

/// Path to the global MCP config file (~/.config/OpenTeam/mcps.json)
pub fn global_mcp_path() -> PathBuf {
    home_dir().join(".config/OpenTeam/mcps.json")
}

/// Path to the assistant MCP config file (~/.config/OpenTeam/assistant/mcps.json)
pub fn assistant_mcp_path() -> PathBuf {
    home_dir().join(".config/OpenTeam/assistant/mcps.json")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_discover_all_multiple_files() {
        let tmp1 = std::env::temp_dir().join("feishu_mcp_reg_test1");
        let tmp2 = std::env::temp_dir().join("feishu_mcp_reg_test2");
        let _ = std::fs::create_dir_all(&tmp1);
        let _ = std::fs::create_dir_all(&tmp2);

        let json1 = r#"[
            {"name": "github", "description": "GitHub API", "command": "node", "args": [], "env": {}, "tools": [{"name": "create_issue", "description": "Create issue", "input_schema": {}}]}
        ]"#;
        let json2 = r#"[
            {"name": "postgres", "description": "PostgreSQL", "command": "python", "args": [], "env": {}, "tools": [{"name": "query", "description": "Run query", "input_schema": {}}]}
        ]"#;

        let p1 = tmp1.join("mcps.json");
        let p2 = tmp2.join("mcps.json");
        let mut f1 = std::fs::File::create(&p1).unwrap();
        f1.write_all(json1.as_bytes()).unwrap();
        let mut f2 = std::fs::File::create(&p2).unwrap();
        f2.write_all(json2.as_bytes()).unwrap();

        let registry = McpRegistry::discover_all(&[p1, p2]).unwrap();
        assert_eq!(registry.list_servers().len(), 2);
        assert_eq!(registry.get_tools_for(&["github".to_string()]).len(), 1);
        assert_eq!(registry.get_tools_for(&["postgres".to_string()])[0].name, "query");

        let _ = std::fs::remove_dir_all(&tmp1);
        let _ = std::fs::remove_dir_all(&tmp2);
    }

    #[test]
    fn test_get_all_tools() {
        let tmp = std::env::temp_dir().join("feishu_mcp_alltools_test");
        let _ = std::fs::create_dir_all(&tmp);
        let path = tmp.join("mcps.json");
        let json = r#"[
            {"name": "github", "description": "", "command": "node", "args": [], "env": {}, "tools": [
                {"name": "create_issue", "description": "a", "input_schema": {}},
                {"name": "search_code", "description": "b", "input_schema": {}}
            ]}
        ]"#;
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(json.as_bytes()).unwrap();
        let registry = McpRegistry::discover_all(&[path]).unwrap();
        // 8 built-in + 2 from MCP config = 10 total
        let all_tools = registry.get_all_tools();
        assert_eq!(all_tools.len(), 10);
        // Verify built-in tools are present
        let tool_names: Vec<&str> = all_tools.iter().map(|t| t.name.as_str()).collect();
        assert!(tool_names.contains(&"read_file"));
        assert!(tool_names.contains(&"write_file"));
        assert!(tool_names.contains(&"create_issue"));
        assert!(tool_names.contains(&"search_code"));
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
