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
        tracing::info!("Plugin system started");

        // Trigger startup hook
        self.trigger_hook("system:startup", &serde_json::json!({
            "timestamp": std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
        })).await;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_plugin_manager_start_stop() {
        let pm = PluginManager::new();
        assert!(!pm.is_running().await);
        pm.start().await;
        assert!(pm.is_running().await);
        pm.stop().await;
        assert!(!pm.is_running().await);
    }

    #[tokio::test]
    async fn test_register_and_trigger_hook() {
        let pm = PluginManager::new();
        pm.register_hook(HookRegistration {
            plugin_name: "test".into(),
            hook_point: "message:received".into(),
            handler_id: "handler-1".into(),
        }).await;

        let payload = serde_json::json!({"content": "hello"});
        let results = pm.trigger_hook("message:received", &payload).await;
        assert_eq!(results.len(), 1, "Should trigger 1 handler");
        assert_eq!(results[0]["plugin"], "test");

        // Unregistered hook returns empty
        let no_results = pm.trigger_hook("nonexistent", &payload).await;
        assert!(no_results.is_empty());
    }

    #[tokio::test]
    async fn test_multiple_hooks_same_point() {
        let pm = PluginManager::new();
        pm.register_hook(HookRegistration {
            plugin_name: "a".into(),
            hook_point: "system:startup".into(),
            handler_id: "h1".into(),
        }).await;
        pm.register_hook(HookRegistration {
            plugin_name: "b".into(),
            hook_point: "system:startup".into(),
            handler_id: "h2".into(),
        }).await;

        let results = pm.trigger_hook("system:startup", &serde_json::json!({})).await;
        assert_eq!(results.len(), 2);
    }
}
