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
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

pub use error::CoreError;

pub struct Core {
    pub registry: registry::AgentRegistry,
    pub llm_gateway: llm::gateway::LlmGateway,
    pub feishu_bridge: feishu::bridge::FeishuBridge,
    pub memory_store: memory::store::MemoryStore,
    pub agent_manager: agent::manager::AgentManager,
    pub router: router::router::MessageRouter,
    pub assistant: Arc<Mutex<assistant::assistant::AssistantAgent>>,
    pub plugin_manager: plugin::manager::PluginManager,
    scheduler_handle: Option<JoinHandle<()>>,
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
        let assistant = Arc::new(Mutex::new(assistant::assistant::AssistantAgent::new()));
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
            scheduler_handle: None,
        })
    }

    pub async fn check_feishu_auth(&self) -> bool {
        self.feishu_bridge.check_auth().await.unwrap_or(false)
    }

    pub fn list_agents(&self) -> Vec<&registry::registry::AgentRecord> {
        self.registry.all()
    }

    /// Process a message through the assistant (convenience method)
    pub async fn process_with_assistant(
        &self,
        message: &str,
        sender: &str,
        model: &config::agent::ModelConfig,
    ) -> Result<Vec<assistant::types::AssistantAction>, CoreError> {
        let mut asst = self.assistant.lock().await;
        asst.process_message(message, sender, &self.llm_gateway, model).await
    }

    /// Start the background scheduler that drives assistant periodic tasks
    pub fn start_scheduler(&mut self) {
        let assistant = self.assistant.clone();
        let handle = tokio::spawn(async move {
            let mut tick_count: u64 = 0;

            loop {
                tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                tick_count += 1;

                // Check assistant escalations
                let escalations = {
                    let asst = assistant.lock().await;
                    asst.check_escalations()
                };

                for action in &escalations {
                    match action {
                        assistant::types::AssistantAction::Respond { message } => {
                            tracing::info!("[Scheduler] Escalation: {message}");
                        }
                        _ => {
                            tracing::debug!("[Scheduler] Other action: {:?}", action);
                        }
                    }
                }

                // Periodic summary: every 30 ticks (15 min in busy mode)
                if tick_count % 30 == 0 {
                    tracing::info!("[Scheduler] Summary tick — checking overall status (tick {tick_count})");
                }
            }
        });

        self.scheduler_handle = Some(handle);
        tracing::info!("Background scheduler started (30s interval)");
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
        // Drop the scheduler handle (task will be cancelled on drop)
        tracing::info!("Core shutdown complete");
    }
}
