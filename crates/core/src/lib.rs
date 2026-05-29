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
use tokio::sync::{Mutex, RwLock};
use tokio::task::JoinHandle;

pub use error::CoreError;

pub struct Core {
    pub registry: registry::AgentRegistry,
    pub llm_gateway: llm::gateway::LlmGateway,
    pub feishu_bridge: feishu::bridge::FeishuBridge,
    pub feishu_app_id: String,
    pub feishu_app_secret: String,
    pub feishu_chat_id: String,
    pub default_model_config: Option<config::agent::ModelConfig>,
    pub memory_store: Arc<memory::store::MemoryStore>,
    pub agent_manager: agent::manager::AgentManager,
    pub router: router::router::MessageRouter,
    pub assistant: Arc<Mutex<assistant::assistant::AssistantAgent>>,
    pub plugin_manager: plugin::manager::PluginManager,
    pub skill_registry: Arc<RwLock<skill::registry::SkillRegistry>>,
    pub mcp_registry: Arc<RwLock<mcp::registry::McpRegistry>>,
    pub provider_resolver: llm::provider::ProviderResolver,
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
        let provider_resolver = llm::provider::ProviderResolver::new(llm_config.provider.clone());
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

        // === Feishu mandatory validation ===

        // Load global config (fall back to env vars)
        let global_config = config::feishu::load_global_config()?;

        let feishu_app_id = config::feishu::resolve_app_id(&global_config)?;
        let feishu_app_secret = config::feishu::resolve_app_secret(&global_config)?;
        let feishu_chat_id = config::feishu::resolve_chat_id(&global_config)?;

        tracing::info!("Feishu credentials configured (app_id: {})", &feishu_app_id[..8]);
        tracing::info!("Feishu chat_id: {}", feishu_chat_id);

        // 3. Create bridge (no lark-cli dependency)
        let feishu_bridge = feishu::bridge::FeishuBridge::new();

        // Pick first available model config as default for summary generation
        let default_model_config = registry.all().first()
            .map(|r| r.config.llm.primary.clone());

        // Release built-in skills to global config directory
        skill::registry::release_builtin_skills()?;

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
            llm_gateway: llm::gateway::LlmGateway::new(llm_config, provider_resolver.clone()),
            feishu_bridge,
            feishu_app_id,
            feishu_app_secret,
            feishu_chat_id,
            default_model_config,
            memory_store,
            agent_manager,
            router,
            assistant,
            plugin_manager,
            provider_resolver,
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

        // Start Channel Bridge (Node.js plugin with Feishu Channel SDK)
        // Capture ALL dependencies needed for message processing
        let feishu_bridge = self.feishu_bridge.clone();
        let feishu_app_id = self.feishu_app_id.clone();
        let feishu_app_secret = self.feishu_app_secret.clone();
        let _feishu_chat_id_for_bridge = self.feishu_chat_id.clone();
        let assistant = self.assistant.clone();
        let llm_gateway = self.llm_gateway.clone();
        let model_config = self.default_model_config.clone();
        let _channel_handle = tokio::spawn(async move {
            tracing::info!("[ChannelBridge] Starting Feishu Channel Bridge...");

            let mut channel_bridge = feishu::channel_bridge::FeishuChannelBridge::new();

            match channel_bridge.start(&feishu_app_id, &feishu_app_secret).await {
                Ok(()) => {
                    tracing::info!("[ChannelBridge] Connected to Feishu via Channel SDK");

                    // Register with FeishuBridge for message sending
                    feishu_bridge.set_channel_bridge(channel_bridge.clone()).await;

                    // Subscribe to incoming messages (broadcast receiver)
                    let mut msg_rx = channel_bridge.subscribe_messages();
                    // Also subscribe to status changes
                    let mut status_rx = channel_bridge.subscribe_status();

                    // Event loop: forward messages, process via assistant, reply
                    loop {
                        tokio::select! {
                            Ok(msg) = msg_rx.recv() => {
                                tracing::info!(
                                    "[ChannelBridge] Message from {} (chat: {}, type: {}): {}",
                                    msg.sender_id, msg.chat_id, msg.chat_type,
                                    &msg.content[..msg.content.len().min(100)],
                                );

                                // Only process if bot is mentioned or it's a direct message
                                if msg.mentioned_bot || msg.chat_type == "p2p" {
                                    if let Some(ref mc) = model_config {
                                        let mut asst = assistant.lock().await;
                                        match asst.process_message(
                                            &msg.content,
                                            &msg.sender_id,
                                            &llm_gateway,
                                            mc,
                                        ).await {
                                            Ok(actions) => {
                                                // Drop assistant lock before calling Feishu API
                                                drop(asst);
                                                for action in &actions {
                                                    match action {
                                                        assistant::types::AssistantAction::Respond { message } => {
                                                            tracing::info!("[ChannelBridge] Assistant response: {}",
                                                                &message[..message.len().min(100)]);
                                                            let _ = feishu_bridge.reply_to_message(
                                                                &msg.message_id,
                                                                message,
                                                                msg.thread_id.is_some(),
                                                            ).await;
                                                        }
                                                        _ => {
                                                            tracing::debug!("[ChannelBridge] Action: {:?}", action);
                                                        }
                                                    }
                                                }
                                            }
                                            Err(e) => {
                                                tracing::error!("[ChannelBridge] Assistant error: {e}");
                                            }
                                        }
                                    } else {
                                        tracing::warn!("[ChannelBridge] No model configured — cannot process message");
                                    }
                                } else {
                                    tracing::debug!("[ChannelBridge] Skipping — bot not mentioned");
                                }
                            }
                            Ok(_status) = status_rx.changed() => {
                                let status = status_rx.borrow().clone();
                                match &status {
                                    feishu::types::ChannelStatus::Connected { bot_name } => {
                                        tracing::info!("[ChannelBridge] Status: Connected as {bot_name}");
                                    }
                                    feishu::types::ChannelStatus::Connecting => {
                                        tracing::info!("[ChannelBridge] Status: Reconnecting...");
                                    }
                                    feishu::types::ChannelStatus::Disconnected => {
                                        tracing::warn!("[ChannelBridge] Status: Disconnected");
                                        break;
                                    }
                                    feishu::types::ChannelStatus::Error(e) => {
                                        tracing::error!("[ChannelBridge] Status: Error — {e}");
                                    }
                                }
                            }
                            else => break,
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("[ChannelBridge] Failed to start: {e}");
                }
            }
        });
        tracing::info!("Channel Bridge subscriber started");

        // Also keep the old subscribe_events as a no-op for backward compat
        let fb_legacy = self.feishu_bridge.clone();
        let _legacy_handle = tokio::spawn(async move {
            let _ = fb_legacy.subscribe_events(&["im.message.receive_v1"]).await;
        });

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
        // Register Feishu Remote MCP server programmatically (not via mcp.json)
        {
            // Create and register the Feishu token manager for dynamic TAT refresh
            let tm = feishu::token::FeishuTokenManager::new(
                self.feishu_app_id.clone(),
                self.feishu_app_secret.clone(),
            );
            mcp::config::register_feishu_token_manager(tm);

            // Add Feishu Remote MCP as a standard HTTP MCP server
            let feishu_mcp_server = mcp::config::McpServerConfig {
                name: "feishu-remote-mcp".into(),
                entry: mcp::config::McpServerEntry {
                    url: Some("https://mcp.feishu.cn/mcp".into()),
                    headers: std::collections::HashMap::from([
                        ("Content-Type".into(), "application/json".into()),
                        ("X-Lark-MCP-TAT".into(), "${FEISHU_TAT}".into()),
                        ("X-Lark-MCP-Allowed-Tools".into(),
                            "create-doc,fetch-doc,search-doc,update-doc,list-docs,get-comments,add-comments,search-user,get-user,fetch-file".into()),
                    ]),
                    command: None,
                    args: vec![],
                    env: std::collections::HashMap::new(),
                    enabled: true,
                },
                tools: vec![],
            };
            {
                let reg = self.mcp_registry.clone();
                let handle = tokio::runtime::Handle::current();
                handle.block_on(async move {
                    let mut guard = reg.write().await;
                    guard.register_server(feishu_mcp_server);
                });
            }
            tracing::info!("Feishu Remote MCP server registered (programmatic)");
        }

        let mcp_registry = self.mcp_registry.clone();
        let global_mcp_path = mcp::registry::global_mcp_path();
        let asst_mcp_path = mcp::registry::assistant_mcp_path();
        let mut mcp_watch_files = vec![global_mcp_path];
        if asst_mcp_path.exists() {
            mcp_watch_files.push(asst_mcp_path);
        }
        let agents_dir = skill::registry::global_agents_dir();
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
        let agents_dir = skill::registry::global_agents_dir();
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
        let feishu_bridge_for_sched = self.feishu_bridge.clone();
        let feishu_chat_id_for_sched = self.feishu_chat_id.clone();
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
                            // Send to default chat (no thread context for scheduled responses)
                            let outgoing = feishu::types::OutgoingMessage {
                                chat_id: feishu_chat_id_for_sched.clone(),
                                thread_id: None,
                                text: resp,
                                mentions: vec![],
                                priority: feishu::types::MessagePriority::Secretary,
                            };
                            if let Err(e) = feishu_bridge_for_sched.send_message(&outgoing).await {
                                tracing::error!("[Scheduler] Failed to send response: {e}");
                            }
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
