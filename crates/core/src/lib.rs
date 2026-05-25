pub mod config;
pub mod registry;
pub mod llm;
pub mod feishu;
pub mod error;
pub mod memory;
pub mod agent;
pub mod router;
pub mod assistant;
pub mod plugin;

use std::path::Path;

pub use error::CoreError;

pub struct Core {
    pub registry: registry::AgentRegistry,
    pub llm_gateway: llm::gateway::LlmGateway,
    pub feishu_bridge: feishu::bridge::FeishuBridge,
    pub memory_store: memory::store::MemoryStore,
    pub agent_manager: agent::manager::AgentManager,
    pub router: router::router::MessageRouter,
    pub assistant: assistant::assistant::AssistantAgent,
    pub plugin_manager: plugin::manager::PluginManager,
}

impl Core {
    pub async fn new(
        agents_dir: &Path,
        llm_config_path: &Path,
        memory_db_path: &str,
    ) -> Result<Self, CoreError> {
        let llm_config = config::load_llm_config(llm_config_path)?;
        let mut registry = registry::AgentRegistry::new();
        let configs = config::load_all_agents(agents_dir)?;
        for cfg in configs {
            registry.register(cfg);
        }

        let memory_config = memory::types::MemoryConfig::default();
        let memory_store = memory::store::MemoryStore::new(memory_db_path, memory_config).await?;

        let agent_manager = agent::manager::AgentManager::new();
        let router = router::router::MessageRouter::new();
        let assistant = assistant::assistant::AssistantAgent::new();
        let plugin_manager = plugin::manager::PluginManager::new();

        Ok(Self {
            registry,
            llm_gateway: llm::gateway::LlmGateway::new(llm_config),
            feishu_bridge: feishu::bridge::FeishuBridge::new(),
            memory_store,
            agent_manager,
            router,
            assistant,
            plugin_manager,
        })
    }

    pub async fn check_feishu_auth(&self) -> bool {
        self.feishu_bridge.check_auth().await.unwrap_or(false)
    }

    pub fn list_agents(&self) -> Vec<&registry::registry::AgentRecord> {
        self.registry.all()
    }

    /// Process a message through the assistant (convenience method)
    /// Phase 3 V2: proper async mutex on assistant for interior mutability
    pub async fn process_with_assistant(
        &self,
        _message: &str,
        _sender: &str,
        _model: &config::agent::ModelConfig,
    ) -> Result<Vec<assistant::types::AssistantAction>, CoreError> {
        tracing::info!("[Core] Assistant message processing (stub — Phase 3 V2)");
        // Phase 3 V2: wrap assistant in Arc<Mutex<>> for interior mutability
        Ok(Vec::new())
    }

    pub async fn shutdown(&self) {
        tracing::info!("Core shutting down...");
        self.plugin_manager.trigger_hook("system:shutdown", &serde_json::json!({
            "timestamp": std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
        })).await;
        self.agent_manager.shutdown_all().await;
        self.plugin_manager.stop().await;
        tracing::info!("Core shutdown complete");
    }
}
