use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::{mpsc, RwLock};
use tokio_util::sync::CancellationToken;
use crate::config::agent::AgentConfig;
use crate::llm::gateway::{LlmGateway, ChatRequest, ChatMessage};
use crate::mcp::config::McpServerConfig;
use crate::mcp::registry::McpRegistry;
use crate::registry::{AgentId, AgentRegistry, AgentStatus};
use crate::skill::registry::SkillRegistry;
use crate::memory::store::MemoryStore;
use crate::memory::types::{MemoryEntry, MemoryType};
use super::handle::*;
use super::inbox::*;

pub struct AgentManager {
    agents: RwLock<HashMap<AgentId, AgentHandle>>,
    shutdown_token: CancellationToken,
}

impl AgentManager {
    pub fn new() -> Self {
        Self {
            agents: RwLock::new(HashMap::new()),
            shutdown_token: CancellationToken::new(),
        }
    }

    pub async fn spawn_agent(
        &self,
        config: AgentConfig,
        agent_registry: Arc<RwLock<AgentRegistry>>,
        llm_gateway: Arc<LlmGateway>,
        skill_registry: Arc<RwLock<SkillRegistry>>,
        memory_store: Arc<MemoryStore>,
        mcp_registry: Arc<RwLock<McpRegistry>>,
    ) -> AgentId {
        let id = uuid::Uuid::now_v7();
        let (control_tx, mut control_rx) = mpsc::channel::<AgentCommand>(64);
        let cancel_token = self.shutdown_token.child_token();
        let cancel_clone = cancel_token.clone();
        let config_clone = config.clone();
        let id_clone = id;
        let llm_gateway_clone = llm_gateway;
        let skills_clone = skill_registry;
        let memory_clone = memory_store;
        let mcp_clone = mcp_registry;
        let registry_clone = agent_registry.clone();

        let handle = tokio::spawn(async move {
            agent_main_loop(
                &config_clone, id_clone, &mut control_rx, cancel_clone,
                llm_gateway_clone, skills_clone, memory_clone, mcp_clone, registry_clone,
            ).await;
        });

        let agent_handle = AgentHandle {
            id,
            config,
            join_handle: handle,
            control_tx,
            cancel_token,
        };

        self.agents.write().await.insert(id, agent_handle);
        id
    }

    pub async fn send_command(&self, id: &AgentId, cmd: AgentCommand) -> Result<(), String> {
        let agents = self.agents.read().await;
        if let Some(handle) = agents.get(id) {
            handle.control_tx.send(cmd).await
                .map_err(|_| format!("Agent {} channel closed", id))
        } else {
            Err(format!("Agent {} not found", id))
        }
    }

    pub async fn shutdown_all(&self) {
        self.shutdown_token.cancel();
        // Give agents 30s to gracefully stop
        tokio::time::sleep(std::time::Duration::from_secs(30)).await;
    }

    pub async fn agent_count(&self) -> usize {
        self.agents.read().await.len()
    }

    /// Stop a specific agent by sending Stop command
    pub async fn stop_agent(&self, id: &AgentId) -> Result<(), String> {
        self.send_command(id, AgentCommand::Stop).await
    }

    /// Kill a specific agent by cancelling its token (hard stop, bypasses graceful shutdown)
    pub async fn kill_agent(&self, id: &AgentId) -> Result<(), String> {
        let mut agents = self.agents.write().await;
        if let Some(handle) = agents.remove(id) {
            handle.cancel_token.cancel();
            Ok(())
        } else {
            Err(format!("Agent {id} not found"))
        }
    }
}

async fn agent_main_loop(
    config: &AgentConfig,
    id: AgentId,
    control_rx: &mut mpsc::Receiver<AgentCommand>,
    cancel: CancellationToken,
    llm_gateway: Arc<LlmGateway>,
    skill_registry: Arc<RwLock<SkillRegistry>>,
    memory_store: Arc<MemoryStore>,
    mcp_registry: Arc<RwLock<McpRegistry>>,
    agent_registry: Arc<RwLock<AgentRegistry>>,
) {
    let mut inbox = PriorityInbox::new();
    let mut is_paused = false;
    let mut retry_count: u32 = 0;  // Per-agent escalation tracking
    // Per-thread conversation history — key is thread_id (None = main channel)
    let mut thread_histories: HashMap<Option<String>, Vec<ChatMessage>> = HashMap::new();

    // Build base system prompt: role + skill instructions (constant across messages)
    let base_prompt = skill_registry.read().await.build_system_prompt(&config.role);

    loop {
        tokio::select! {
            Some(cmd) = control_rx.recv() => {
                match cmd {
                    AgentCommand::Stop => {
                        tracing::info!("Agent {} stopping", config.name);
                        break;
                    }
                    AgentCommand::Pause => {
                        is_paused = true;
                        tracing::info!("Agent {} paused", config.name);
                    }
                    AgentCommand::Resume => {
                        is_paused = false;
                        tracing::info!("Agent {} resumed", config.name);
                    }
                    AgentCommand::InjectMessage { content, thread_id } => {
                        inbox.push(InboxMessage {
                            priority: 3,
                            content,
                            thread_id,
                            received_at: SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_secs() as i64,
                        });
                    }
                    AgentCommand::OverrideContext(ctx) => {
                        tracing::info!("Context override for {}: {}", config.name, ctx);
                    }
                }
            }
            _ = cancel.cancelled() => {
                tracing::info!("Agent {} cancelled, shutting down", config.name);
                break;
            }
        }

        // Process inbox if not paused — actual LLM processing with fallback
        if !is_paused {
            while let Some(msg) = inbox.pop() {
                tracing::info!("Agent {} processing message (pri={})", config.name, msg.priority);

                // Mark agent as busy
                {
                    let mut reg = agent_registry.write().await;
                    reg.update_status(&id, AgentStatus::Busy).ok();
                }

                // Inject tools from MCP registry (built-in + discovered MCP servers)
                let tools: Vec<crate::llm::gateway::ToolDefinition> = mcp_registry.read().await
                    .get_all_tools()
                    .into_iter()
                    .map(|t| crate::llm::gateway::ToolDefinition {
                        name: t.name,
                        description: t.description,
                        input_schema: t.input_schema,
                    })
                    .collect();
                // Build server configs map for tool execution
                let servers: HashMap<String, McpServerConfig> = mcp_registry.read().await
                    .list_servers()
                    .into_iter()
                    .map(|s| (s.name.clone(), s.clone()))
                    .collect();

                // Build full system prompt with memory context for this message
                let full_prompt = {
                    // Hash-based embedding from message content
                    let query_emb = crate::memory::vectorizer::hash_embed(&msg.content);
                    let memories = memory_store.search_semantic(
                        &config.name,
                        &query_emb,
                        3,
                    ).await.unwrap_or_default();

                    let memory_prompt = if memories.is_empty() {
                        base_prompt.clone()
                    } else {
                        let mut prompt = base_prompt.clone();
                        prompt.push_str("\n\n## Related Memories\n");
                        for (i, mem) in memories.iter().enumerate() {
                            prompt.push_str(&format!(
                                "{}. {} (importance: {}, similarity: {:.2})\n   {}\n",
                                i + 1,
                                mem.entry.title,
                                mem.entry.importance,
                                mem.semantic_score,
                                mem.entry.summary,
                            ));
                        }
                        prompt
                    };

                    // Inject thread context if the message came from a thread
                    if let Some(ref tid) = msg.thread_id {
                        format!(
                            "{}\n\n## Thread Context\nYou are currently working in thread: {}. All task-related messages should be sent in this thread via the send_feishu_message tool with thread_id=\"{}\". Keep all task collaboration within the thread to maintain context isolation.",
                            memory_prompt, tid, tid
                        )
                    } else {
                        memory_prompt
                    }
                };

                // Build LLM request with enhanced prompt + thread history
                let mut thread_msgs: Vec<ChatMessage> = Vec::new();
                // Add previous conversation history for this thread
                if let Some(history) = thread_histories.get(&msg.thread_id) {
                    thread_msgs.extend(history.clone());
                }
                // Add current message
                thread_msgs.push(ChatMessage { reasoning_content: None, role: "user".into(),
                    content: msg.content.clone(),
                });

                let request = ChatRequest {
                    model: config.llm.primary.model.clone(),
                    system_prompt: full_prompt.clone(),
                    messages: thread_msgs.clone(),
                    tools: tools.clone(),
                };

                // Track user message for thread history
                let user_msg = ChatMessage { reasoning_content: None, role: "user".into(),
                    content: msg.content.clone(),
                };

                match llm_gateway.chat(&config.llm.primary, &request).await {
                    Ok(mut response) => {
                        // Tool execution loop: if LLM wants to use tools, execute and feed back
                        while !response.tool_calls.is_empty() {
                            tracing::info!(
                                "Agent {} executing {} tool(s)",
                                config.name,
                                response.tool_calls.len(),
                            );

                            // Build assistant message with text + tool_use content blocks
                            let mut assistant_content: Vec<serde_json::Value> = Vec::new();
                            if !response.content.is_empty() {
                                assistant_content.push(serde_json::json!({
                                    "type": "text",
                                    "text": response.content,
                                }));
                            }
                            for tc in &response.tool_calls {
                                assistant_content.push(serde_json::json!({
                                    "type": "tool_use",
                                    "id": tc.id,
                                    "name": tc.name,
                                    "input": tc.arguments,
                                }));
                            }

                            thread_msgs.push(ChatMessage { reasoning_content: None, role: "assistant".into(),
                                content: serde_json::to_string(&assistant_content)
                                    .unwrap_or_default(),
                            });

                            // Execute each tool and build tool_result messages
                            for tc in &response.tool_calls {
                                // Phase 3 V3: integrate McpRegistry for server lookup.
                                // For now, tool name doubles as server name.
                                let result = crate::mcp::executor::execute_tool(
                                    tc,
                                    &tc.name,
                                    &servers,
                                )
                                .await
                                .unwrap_or_else(|e| format!("Error: {e}"));

                                let tool_result_content = serde_json::json!([{
                                    "type": "tool_result",
                                    "tool_use_id": tc.id,
                                    "content": result,
                                }]);

thread_msgs.push(ChatMessage { reasoning_content: None, role: "user".into(),
                                    content: serde_json::to_string(&tool_result_content)
                                        .unwrap_or_default(),
                                });
                            }

                            // Call LLM again with tool results (no tools needed this round)
                            let tool_request = ChatRequest {
                                model: config.llm.primary.model.clone(),
                                system_prompt: full_prompt.clone(),
                                messages: thread_msgs.clone(),
                                tools: vec![],
                            };

                            match llm_gateway.chat(&config.llm.primary, &tool_request).await {
                                Ok(next_resp) => {
                                    response = next_resp;
                                }
                                Err(e) => {
                                    tracing::error!(
                                        "Agent {} LLM error during tool loop: {e}",
                                        config.name,
                                    );
                                    break;
                                }
                            }
                        }

                        // Save to thread history
                        let history = thread_histories.entry(msg.thread_id.clone()).or_default();
                        history.push(user_msg.clone());
                        history.push(ChatMessage { role: "assistant".into(),
                            content: response.content.clone(),
                            reasoning_content: response.reasoning_content.clone(),
                        });
                        // Smart compression: estimate token count and compress when approaching limit
                        let max_context = config.llm.primary.max_tokens as f64 * 0.8; // 80% of max
                        let estimated_tokens: usize = history.iter()
                            .map(|m| m.content.len() / 2) // rough estimate: ~2 chars per token for English, ~1 for Chinese
                            .sum();
                        if estimated_tokens as f64 > max_context {
                            // Keep the last 10 turns (most recent), compress the rest into a summary
                            let keep = if history.len() > 20 { 10 } else { history.len() / 2 };
                            let compress_count = history.len().saturating_sub(keep);
                            if compress_count > 2 {
                                let _to_compress: Vec<String> = history.drain(0..compress_count)
                                    .map(|m| format!("{}: {}", m.role, &m.content[..m.content.len().min(100)])).collect();
                                let summary = format!("[Earlier conversation: {} turns compressed]", compress_count / 2);
                                tracing::info!("Agent {} compressed {} history turns (est. {} tokens)", config.name, compress_count, estimated_tokens);
                                history.insert(0, ChatMessage { reasoning_content: None, role: "system".into(),
                                    content: summary,
                                });
                            }
                        }

                        tracing::info!(
                            "Agent {} final response ({} tokens): {}",
                            config.name,
                            response.usage.output_tokens,
                            &response.content[..response.content.len().min(100)],
                        );

                        // Store important responses as memories
                        if response.usage.output_tokens > 100 {
                            let _ = memory_store.insert(&MemoryEntry {
                                id: uuid::Uuid::now_v7(),
                                agent_id: config.name.clone(),
                                memory_type: MemoryType::ShortTerm,
                                title: format!(
                                    "Response to: {}",
                                    &msg.content[..msg.content.len().min(60)],
                                ),
                                summary: response.content.clone(),
                                decisions: vec![],
                                artifacts: vec![],
                                pending_todos: vec![],
                                importance: 5,
                                embedding: Some(crate::memory::vectorizer::hash_embed(&response.content)),
                                turn_indices: vec![],
                                created_at: chrono::DateTime::<chrono::Utc>::from(
                                    SystemTime::now(),
                                ),
                                last_accessed: chrono::DateTime::<chrono::Utc>::from(
                                    SystemTime::now(),
                                ),
                                access_count: 0,
                            }).await;
                        }
                    }
                    Err(e) => {
                        retry_count += 1;
                        let escalation_level = if retry_count <= 2 {
                            1  // Initial retry
                        } else if retry_count <= 5 {
                            2  // Escalated: consider fallback or supervisor alert
                        } else {
                            3  // Critical: agent may be stuck, needs human intervention
                        };
                        tracing::warn!(
                            "Agent {} escalation level {} (retry {}): {e}",
                            config.name,
                            escalation_level,
                            retry_count,
                        );

                        // Try fallback model if primary fails
                        if let Some(ref fallback) = config.llm.fallback {
                            tracing::warn!(
                                "Agent {} primary LLM failed, trying fallback: {e}",
                                config.name,
                            );
                            let fb_request = ChatRequest {
                                model: fallback.model.clone(),
                                system_prompt: base_prompt.clone(),
                                messages: vec![
                                    ChatMessage { reasoning_content: None, role: "user".into(),
                                        content: msg.content.clone(),
                                    },
                                ],
                                tools: vec![],
                            };
                            match llm_gateway.chat(fallback, &fb_request).await {
                                Ok(resp) => {
                                    retry_count = 0;  // Reset on success
                                    // Save fallback response to thread history
                                    let history = thread_histories.entry(msg.thread_id.clone()).or_default();
                                    history.push(user_msg.clone());
                                    history.push(ChatMessage { role: "assistant".into(),
                                        content: resp.content.clone(),
                                        reasoning_content: resp.reasoning_content.clone(),
                                    });
                                    // Smart compression: estimate token count and compress when approaching limit
                        let max_context = config.llm.primary.max_tokens as f64 * 0.8; // 80% of max
                        let estimated_tokens: usize = history.iter()
                            .map(|m| m.content.len() / 2) // rough estimate: ~2 chars per token for English, ~1 for Chinese
                            .sum();
                        if estimated_tokens as f64 > max_context {
                            // Keep the last 10 turns (most recent), compress the rest into a summary
                            let keep = if history.len() > 20 { 10 } else { history.len() / 2 };
                            let compress_count = history.len().saturating_sub(keep);
                            if compress_count > 2 {
                                let _to_compress: Vec<String> = history.drain(0..compress_count)
                                    .map(|m| format!("{}: {}", m.role, &m.content[..m.content.len().min(100)])).collect();
                                let summary = format!("[Earlier conversation: {} turns compressed]", compress_count / 2);
                                tracing::info!("Agent {} compressed {} history turns (est. {} tokens)", config.name, compress_count, estimated_tokens);
                                history.insert(0, ChatMessage { reasoning_content: None, role: "system".into(),
                                    content: summary,
                                });
                            }
                        }

                                    tracing::info!(
                                        "Agent {} fallback response: {}",
                                        config.name,
                                        &resp.content[..resp.content.len().min(100)],
                                    );
                                }
                                Err(e2) => tracing::error!(
                                    "Agent {} both LLMs failed: primary={e}, fallback={e2}",
                                    config.name,
                                ),
                            }
                        } else {
                            tracing::error!("Agent {} LLM call failed: {e}", config.name);
                        }
                    }
                }

                // Mark agent as idle after message processing
                {
                    let mut reg = agent_registry.write().await;
                    reg.update_status(&id, AgentStatus::Idle).ok();
                }
            }
        }
    }
}
