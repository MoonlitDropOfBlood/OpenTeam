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

    /// Load MCP configs from mcp.json files (sync, no probing)
    pub fn discover_all(config_paths: &[PathBuf]) -> Result<Self, CoreError> {
        let mut registry = Self::new();
        for path in config_paths {
            let configs = load_mcp_configs(path)?;
            for cfg in configs {
                registry.servers.insert(cfg.name.clone(), cfg);
            }
        }
        Ok(registry)
    }

    /// Probe all servers for their tool lists (async, spawns processes / HTTP calls)
    pub async fn probe_all(&mut self) {
        let names: Vec<String> = self.servers.keys().cloned().collect();
        for name in names {
            if let Some(server) = self.servers.get_mut(&name) {
                match probe_server_tools(server).await {
                    Ok(tools) => {
                        server.tools = tools;
                    }
                    Err(e) => {
                        tracing::warn!("Failed to probe MCP server '{}': {e}", name);
                    }
                }
            }
        }
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

    /// Start a file watcher for hot-reload (watches parent dirs of mcp.json files)
    pub fn start_watcher(
        registry: Arc<RwLock<Self>>,
        watch_files: Vec<PathBuf>,
    ) -> Result<tokio::task::JoinHandle<()>, CoreError> {
        use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
        use std::sync::mpsc;

        let (tx, rx) = mpsc::channel::<Result<notify::Event, notify::Error>>();
        let mut watcher = RecommendedWatcher::new(tx, Config::default())
            .map_err(|e| CoreError::Mcp(format!("Create watcher: {e}")))?;

        // Watch the parent directory of each mcp.json file
        let mut watched_parents: Vec<PathBuf> = Vec::new();
        for f in &watch_files {
            if let Some(parent) = f.parent() {
                if parent.exists() && !watched_parents.iter().any(|p| p == parent) {
                    watcher
                        .watch(parent, RecursiveMode::NonRecursive)
                        .map_err(|e| CoreError::Mcp(format!("Watch {:?}: {e}", parent)))?;
                    watched_parents.push(parent.to_path_buf());
                    tracing::info!("[MCP Watcher] Watching: {:?} for mcp.json changes", parent);
                }
            }
        }

        let watch_files_clone = watch_files.clone();
        let handle = tokio::task::spawn_blocking(move || {
            let _watcher = watcher;
            for res in rx {
                match res {
                    Ok(event) => {
                        let is_mcp_change = matches!(
                            event.kind,
                            EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
                        ) && event.paths.iter().any(|p| {
                            p.file_name()
                                .and_then(|n| n.to_str())
                                == Some("mcp.json")
                        });

                        if is_mcp_change {
                            tracing::info!("[MCP Watcher] mcp.json changed, hot-reloading...");

                            // Use the current tokio runtime handle to run async probing
                            let handle = tokio::runtime::Handle::current();
                            let mut reg = registry.blocking_write();
                            reg.servers.clear();

                            for path in &watch_files_clone {
                                if path.exists() {
                                    if let Ok(configs) = load_mcp_configs(path) {
                                        for mut cfg in configs {
                                            match handle.block_on(probe_server_tools(&cfg)) {
                                                Ok(tools) => {
                                                    cfg.tools = tools;
                                                }
                                                Err(e) => {
                                                    tracing::warn!(
                                                        "[MCP Watcher] Probe failed for '{}': {e}",
                                                        cfg.name
                                                    );
                                                }
                                            }
                                            reg.servers.insert(cfg.name.clone(), cfg);
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

/// Path to the global MCP config file (~/.config/OpenTeam/mcp.json)
pub fn global_mcp_path() -> PathBuf {
    home_dir().join(".config/OpenTeam/mcp.json")
}

/// Path to the assistant MCP config file (~/.config/OpenTeam/assistant/mcp.json)
pub fn assistant_mcp_path() -> PathBuf {
    home_dir().join(".config/OpenTeam/assistant/mcp.json")
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

        // Standard mcp.json format with tools defined inline for testing
        // (real tools come from probing, but we test config parsing here)
        let json1 = r#"{
            "mcpServers": {
                "github": {
                    "command": "node",
                    "args": [],
                    "env": {}
                }
            }
        }"#;
        let json2 = r#"{
            "mcpServers": {
                "postgres": {
                    "command": "python",
                    "args": [],
                    "env": {}
                }
            }
        }"#;

        let p1 = tmp1.join("mcp.json");
        let p2 = tmp2.join("mcp.json");
        let mut f1 = std::fs::File::create(&p1).unwrap();
        f1.write_all(json1.as_bytes()).unwrap();
        let mut f2 = std::fs::File::create(&p2).unwrap();
        f2.write_all(json2.as_bytes()).unwrap();

        // discover_all is now sync (no probing)
        let registry = McpRegistry::discover_all(&[p1, p2]).unwrap();
        assert_eq!(registry.list_servers().len(), 2);

        let _ = std::fs::remove_dir_all(&tmp1);
        let _ = std::fs::remove_dir_all(&tmp2);
    }

    #[test]
    fn test_get_all_tools() {
        let tmp = std::env::temp_dir().join("feishu_mcp_alltools_test");
        let _ = std::fs::create_dir_all(&tmp);
        let path = tmp.join("mcp.json");
        let json = r#"{
            "mcpServers": {
                "github": {
                    "command": "node",
                    "args": [],
                    "env": {}
                }
            }
        }"#;
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(json.as_bytes()).unwrap();

        let registry = McpRegistry::discover_all(&[path]).unwrap();
        // 8 built-in + 0 probed (no real server) = 8 total
        let all_tools = registry.get_all_tools();
        assert_eq!(all_tools.len(), 8);
        // Verify built-in tools are present
        let tool_names: Vec<&str> = all_tools.iter().map(|t| t.name.as_str()).collect();
        assert!(tool_names.contains(&"read_file"));
        assert!(tool_names.contains(&"write_file"));
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
