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
pub mod skill;

use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

pub use error::CoreError;

pub struct Core {
    pub registry: registry::AgentRegistry,
    pub llm_gateway: llm::gateway::LlmGateway,
    pub feishu_bridge: feishu::bridge::FeishuBridge,
    pub memory_store: Arc<memory::store::MemoryStore>,
    pub agent_manager: agent::manager::AgentManager,
    pub router: router::router::MessageRouter,
    pub assistant: Arc<Mutex<assistant::assistant::AssistantAgent>>,
    pub plugin_manager: plugin::manager::PluginManager,
    pub skill_registry: Arc<skill::registry::SkillRegistry>,
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
        let memory_store = Arc::new(memory::store::MemoryStore::new(memory_db_path, memory_config).await?);

        let agent_manager = agent::manager::AgentManager::new();
        let router = router::router::MessageRouter::new();
        let assistant = Arc::new(Mutex::new(assistant::assistant::AssistantAgent::new()));
        let plugin_manager = plugin::manager::PluginManager::new();

        // Discover skills
        let skills_dir = Path::new("skills");
        let skill_registry = Arc::new(skill::registry::SkillRegistry::discover(skills_dir)?);

        Ok(Self {
            registry,
            llm_gateway: llm::gateway::LlmGateway::new(llm_config),
            feishu_bridge: feishu::bridge::FeishuBridge::new(),
            memory_store,
            agent_manager,
            router,
            assistant,
            plugin_manager,
            skill_registry,
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

    /// Build the system prompt for an agent by injecting relevant skill instructions
    pub fn build_agent_prompt(&self, role: &str, skill_names: &[String]) -> String {
        self.skill_registry.build_system_prompt(role, skill_names)
    }

    /// Start the background scheduler that drives assistant periodic tasks
    pub fn start_scheduler(&mut self) {
        let assistant = self.assistant.clone();
        let handle = tokio::spawn(async move {
            let mut secs_since_last_summary: u64 = 0;

            loop {
                tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                secs_since_last_summary += 30;

                // Check assistant escalations (pure Rust — no LLM cost)
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

                // Periodic summary: interval depends on current mode
                let summary_interval = {
                    let asst = assistant.lock().await;
                    let policy = asst.time_policy_config.resolve(
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs(),
                        "",
                    );
                    policy.summary_interval_secs
                };

                if secs_since_last_summary >= summary_interval {
                    let has_pending = {
                        let asst = assistant.lock().await;
                        asst.has_pending_summaries()
                    };
                    if has_pending {
                        let count = {
                            let asst = assistant.lock().await;
                            asst.pending_conversation_count
                        };
                        tracing::info!(
                            "[Scheduler] Summary — {} pending conversations, mode: {:?}, interval: {}s",
                            count, 
                            {
                                let asst = assistant.lock().await;
                                asst.current_mode.clone()
                            },
                            summary_interval,
                        );
                        // Phase 3 V3: call LLM to generate summary
                    } else {
                        tracing::debug!("[Scheduler] Summary skipped — no pending conversations");
                    }
                    secs_since_last_summary = 0;
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
