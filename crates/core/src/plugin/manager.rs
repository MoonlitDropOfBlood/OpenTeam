use std::collections::HashMap;
use tokio::sync::RwLock;

#[derive(Debug, Clone)]
pub struct HookRegistration {
    pub plugin_name: String,
    pub hook_point: String,
    pub handler_id: String,
}

/// Manages plugin lifecycle and hook registrations
pub struct PluginManager {
    hooks: RwLock<HashMap<String, Vec<HookRegistration>>>,
    running: RwLock<bool>,
}

impl PluginManager {
    pub fn new() -> Self {
        Self {
            hooks: RwLock::new(HashMap::new()),
            running: RwLock::new(false),
        }
    }

    /// Start the plugin system
    pub async fn start(&self) {
        let mut running = self.running.write().await;
        *running = true;
        tracing::info!("Plugin system started (Phase 3 V1: stub)");
    }

    /// Stop the plugin system
    pub async fn stop(&self) {
        let mut running = self.running.write().await;
        *running = false;
        tracing::info!("Plugin system stopped");
    }

    /// Trigger a hook point — run all registered handlers
    pub async fn trigger_hook(&self, hook_point: &str, _payload: &serde_json::Value) -> Vec<serde_json::Value> {
        let hooks = self.hooks.read().await;
        let mut results = Vec::new();
        if let Some(handlers) = hooks.get(hook_point) {
            for reg in handlers {
                tracing::debug!("Triggering hook {hook_point} for plugin {}", reg.plugin_name);
                results.push(serde_json::json!({"handled": true, "plugin": reg.plugin_name}));
            }
        }
        results
    }

    /// Register a hook handler
    pub async fn register_hook(&self, reg: HookRegistration) {
        let mut hooks = self.hooks.write().await;
        hooks.entry(reg.hook_point.clone())
            .or_default()
            .push(reg);
    }

    /// Check if plugin system is running
    pub async fn is_running(&self) -> bool {
        *self.running.read().await
    }
}
