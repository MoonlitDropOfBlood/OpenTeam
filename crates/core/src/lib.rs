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
pub mod mcp;

use std::path::Path;
use std::sync::Arc;
use tokio::io::AsyncBufReadExt;
use tokio::sync::{Mutex, RwLock};
use tokio::task::JoinHandle;

pub use error::CoreError;

pub struct Core {
    pub registry: registry::AgentRegistry,
    pub llm_gateway: llm::gateway::LlmGateway,
    pub feishu_bridge: feishu::bridge::FeishuBridge,
    pub feishu_chat_id: String,
    pub default_model_config: Option<config::agent::ModelConfig>,
    pub memory_store: Arc<memory::store::MemoryStore>,
    pub agent_manager: agent::manager::AgentManager,
    pub router: router::router::MessageRouter,
    pub assistant: Arc<Mutex<assistant::assistant::AssistantAgent>>,
    pub plugin_manager: plugin::manager::PluginManager,
    pub skill_registry: Arc<RwLock<skill::registry::SkillRegistry>>,
    pub mcp_registry: Arc<RwLock<mcp::registry::McpRegistry>>,
    scheduler_handle: Option<JoinHandle<()>>,
    watcher_handle: Option<JoinHandle<()>>,
    mcp_watcher_handle: Option<JoinHandle<()>>,
    agent_watcher_handle: Option<JoinHandle<()>>,
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

        // Read Feishu chat_id from env var
        let feishu_chat_id = std::env::var("FEISHU_CHAT_ID").unwrap_or_default();
        tracing::info!("Feishu chat_id: {}", if feishu_chat_id.is_empty() { "(not set)" } else { &feishu_chat_id });

        // Pick first available model config as default for summary generation
        let default_model_config = registry.all().first()
            .map(|r| r.config.llm.primary.clone());

        // Discover global skills
        let global_skills_dir = skill::registry::global_skills_dir();
        let mut skill_registry = skill::registry::SkillRegistry::discover(&global_skills_dir)?;

        // Discover per-agent skills
        for record in registry.all() {
            let agent_skills_dir = agents_dir.join(&record.config.name).join("skills");
            if agent_skills_dir.exists() {
                tracing::info!("Discovering per-agent skills for {} from {:?}", record.config.name, agent_skills_dir);
                if let Ok(agent_skills) = skill::registry::SkillRegistry::discover(&agent_skills_dir) {
                    skill_registry.merge(agent_skills);
                }
            }
        }

        // Discover assistant skills
        let asst_skills_dir = skill::registry::assistant_skills_dir();
        if asst_skills_dir.exists() {
            tracing::info!("Discovering assistant skills from {:?}", asst_skills_dir);
            if let Ok(asst_skills) = skill::registry::SkillRegistry::discover(&asst_skills_dir) {
                skill_registry.merge(asst_skills);
            }
        }

        let skill_registry = Arc::new(RwLock::new(skill_registry));

        // Collect all skill directories for file watching
        let mut skill_watch_dirs = vec![global_skills_dir];
        if asst_skills_dir.exists() {
            skill_watch_dirs.push(asst_skills_dir);
        }
        for record in registry.all() {
            let agent_skills_dir = agents_dir.join(&record.config.name).join("skills");
            if agent_skills_dir.exists() {
                skill_watch_dirs.push(agent_skills_dir);
            }
        }

        // Discover MCP servers: global + assistant + per-agent mcp.json files
        let global_mcp_path = mcp::registry::global_mcp_path();
        let asst_mcp_path = mcp::registry::assistant_mcp_path();
        let mut mcp_paths = vec![global_mcp_path.clone()];
        if asst_mcp_path.exists() {
            mcp_paths.push(asst_mcp_path.clone());
        }
        for record in registry.all() {
            let agent_mcp_path = agents_dir.join(&record.config.name).join("mcp.json");
            if agent_mcp_path.exists() {
                mcp_paths.push(agent_mcp_path);
            }
        }
        let mut mcp_registry =
            mcp::registry::McpRegistry::discover_all(&mcp_paths)?;
        // Probe servers for tools
        mcp_registry.probe_all().await;
        let mcp_registry = Arc::new(RwLock::new(mcp_registry));

        Ok(Self {
            registry,
            llm_gateway: llm::gateway::LlmGateway::new(llm_config),
            feishu_bridge: feishu::bridge::FeishuBridge::new(),
            feishu_chat_id,
            default_model_config,
            memory_store,
            agent_manager,
            router,
            assistant,
            plugin_manager,
            skill_registry,
            mcp_registry,
            scheduler_handle: None,
            watcher_handle: None,
            mcp_watcher_handle: None,
            agent_watcher_handle: None,
        })
    }

    pub async fn check_feishu_auth(&self) -> bool {
        self.feishu_bridge.check_auth().await.unwrap_or(false)
    }

    /// Spawn all registered agents with full dependencies
    pub async fn spawn_all_agents(&self) {
        let agent_registry = Arc::new(RwLock::new(self.registry.clone()));
        for record in self.registry.all() {
            self.agent_manager.spawn_agent(
                record.config.clone(),
                agent_registry.clone(),
                Arc::new(self.llm_gateway.clone()),
                self.skill_registry.clone(),
                self.memory_store.clone(),
                self.mcp_registry.clone(),
            ).await;
        }
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
    pub async fn build_agent_prompt(&self, role: &str) -> String {
        self.skill_registry.read().await.build_system_prompt(role)
    }

    /// Start the background scheduler and skill file watcher
    pub fn start_scheduler(&mut self) {
        // Register FeishuBridge for built-in tools (send_feishu_message)
        mcp::builtin::register_feishu_bridge(self.feishu_bridge.clone(), self.feishu_chat_id.clone());

        // Start send queue consumer with bridge reference (5 QPS rate-limited dispatcher)
        let send_queue = std::sync::Arc::new(self.feishu_bridge.queue().clone());
        let fb_clone = self.feishu_bridge.clone();
        let consumer_bridge = std::sync::Arc::new(std::sync::Mutex::new(Some(fb_clone)));
        let _consumer_handle = feishu::message_queue::SendQueue::start_consumer(send_queue, consumer_bridge);
        tracing::info!("Send queue consumer started (5 QPS)");

        // Start WebSocket event subscriber (Phase 3 V3: real lark-cli subscription)
        let feishu_bridge_clone = self.feishu_bridge.clone();
        let _ws_handle = tokio::spawn(async move {
            tracing::info!("[WS] Attempting to subscribe to Feishu events...");

            // Check if lark-cli is available
            match tokio::process::Command::new("lark-cli")
                .arg("--version")
                .output()
                .await
            {
                Ok(output) if output.status.success() => {
                    let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    tracing::info!("[WS] lark-cli available: {}", version);

                    // Subscribe to message events
                    match feishu_bridge_clone.subscribe_events(&["im.message.receive_v1"]).await {
                        Ok(mut child) => {
                            tracing::info!("[WS] Subscribed to im.message.receive_v1 (pid: {:?})", child.id());

                            // Read events from stdout
                            if let Some(stdout) = child.stdout.take() {
                                let mut reader = tokio::io::BufReader::new(stdout);
                                let mut line = String::new();
                                loop {
                                    line.clear();
                                    match tokio::time::timeout(
                                        std::time::Duration::from_secs(300),
                                        reader.read_line(&mut line),
                                    ).await {
                                        Ok(Ok(_)) if !line.trim().is_empty() => {
                                            tracing::info!("[WS] Event received: {}", line.trim());
                                            // Phase 3 V3: parse and route event
                                        }
                                        Ok(Ok(_)) => {} // empty line, skip
                                        Ok(Err(e)) => {
                                            tracing::error!("[WS] Read error: {e}");
                                            break;
                                        }
                                        Err(_) => {
                                            tracing::debug!("[WS] No events for 5min, still alive");
                                        }
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            tracing::warn!("[WS] Failed to subscribe: {e}");
                        }
                    }
                }
                _ => {
                    tracing::warn!("[WS] lark-cli not available — WebSocket events disabled");
                }
            }
        });
        tracing::info!("WebSocket event subscriber started");

        // Start skill file watcher (needs multi-threaded runtime)
        let skill_registry = self.skill_registry.clone();
        let global_dir = skill::registry::global_skills_dir();
        let asst_dir = skill::registry::assistant_skills_dir();
        let mut watch_dirs = vec![global_dir];
        if asst_dir.exists() {
            watch_dirs.push(asst_dir);
        }
        if let Ok(watcher) = skill::registry::SkillRegistry::start_watcher(
            skill_registry,
            watch_dirs,
        ) {
            self.watcher_handle = Some(watcher);
            tracing::info!("Skill file watcher started");
        }

        // Start MCP file watcher
        let mcp_registry = self.mcp_registry.clone();
        let global_mcp_path = mcp::registry::global_mcp_path();
        let asst_mcp_path = mcp::registry::assistant_mcp_path();
        let mut mcp_watch_files = vec![global_mcp_path];
        if asst_mcp_path.exists() {
            mcp_watch_files.push(asst_mcp_path);
        }
        let agents_dir = std::path::PathBuf::from("agents");
        for record in self.registry.all() {
            let agent_mcp_path = agents_dir.join(&record.config.name).join("mcp.json");
            if agent_mcp_path.exists() {
                mcp_watch_files.push(agent_mcp_path);
            }
        }
        if let Ok(watcher) = mcp::registry::McpRegistry::start_watcher(
            mcp_registry,
            mcp_watch_files,
        ) {
            self.mcp_watcher_handle = Some(watcher);
            tracing::info!("MCP file watcher started");
        }

        // Start agent YAML file watcher (Phase 3 V3: full lifecycle management)
        let agents_dir = std::path::PathBuf::from("agents");
        if agents_dir.exists() {
            use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
            use std::sync::mpsc;

            let (tx, rx) = mpsc::channel::<Result<notify::Event, notify::Error>>();
            match RecommendedWatcher::new(tx, Config::default()) {
                Ok(mut watcher) => {
                    if let Err(e) = watcher.watch(&agents_dir, RecursiveMode::NonRecursive) {
                        tracing::warn!("Failed to watch agents dir: {e}");
                    } else {
                        let agent_watch_dir = agents_dir.clone();
                        let handle = tokio::task::spawn_blocking(move || {
                            let _watcher = watcher;
                            for res in rx {
                                match res {
                                    Ok(event) => {
                                        let is_yaml_change = matches!(
                                            event.kind,
                                            EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
                                        ) && event.paths.iter().any(|p| {
                                            p.extension().and_then(|e| e.to_str()) == Some("yaml")
                                        });
                                        if is_yaml_change {
                                            tracing::info!(
                                                "[Agent Watcher] Agent YAML changed: {:?} (dir: {:?})",
                                                event.paths,
                                                agent_watch_dir,
                                            );
                                            // Phase 3 V3: re-load config, diff registry, spawn/stop agents
                                        }
                                    }
                                    Err(e) => tracing::error!("[Agent Watcher] Error: {e}"),
                                }
                            }
                        });
                        self.agent_watcher_handle = Some(handle);
                        tracing::info!("Agent file watcher started for: {:?}", agents_dir);
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to create agent watcher: {e}");
                }
            }
        } else {
            tracing::debug!("Agent directory 'agents/' does not exist — skipping agent watcher");
        }

        // Start scheduler task
        let assistant = self.assistant.clone();
        let default_model_config = self.default_model_config.clone();
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

                // Drain and send pending assistant responses via Feishu
                {
                    let mut asst = assistant.lock().await;
                    let responses = asst.drain_responses();
                    if !responses.is_empty() {
                        for resp in responses {
                            tracing::info!(
                                "[Scheduler] Sending assistant response: {}",
                                &resp[..resp.len().min(60)]
                            );
                            // Phase 3 V3: actual Feishu send requires chat_id config
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
                            "[Scheduler] Generating summary for {} pending conversations",
                            count,
                        );
                        let _summary_prompt = format!(
                            "Summarize the work progress for the past period. There are {} unprocessed conversations to summarize. Output in the following format:\n\n1. Completed tasks\n2. In-progress tasks\n3. Blocked issues\n4. Action items",
                            count,
                        );
                        // Phase 3 V3: call assistant.process_message() with summary prompt
                        // Model config is available for future LLM summary generation
                        tracing::info!(
                            "[Scheduler] Summary requested ({} conversations, model config {}available)",
                            count,
                            if default_model_config.is_some() { "" } else { "not " },
                        );
                        // Reset counter to avoid infinite buildup
                        {
                            let mut asst = assistant.lock().await;
                            asst.pending_conversation_count = 0;
                        }
                        tracing::info!("[Scheduler] Summary generated (pending count reset)");
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
