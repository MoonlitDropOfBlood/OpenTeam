use super::time_policy::*;
use super::types::*;
use crate::config::agent::ModelConfig;
use crate::llm::gateway::{ChatMessage, ChatRequest, LlmGateway};
use crate::CoreError;
use chrono::{DateTime, Utc};
use std::collections::HashMap;

/// Get current UTC time without chrono `clock` feature (uses std::time)
fn utc_now() -> DateTime<Utc> {
    let since_epoch = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    // nanos are always < 1_999_999_999 so unwrap is safe
    DateTime::from_timestamp(since_epoch.as_secs() as i64, since_epoch.subsec_nanos())
        .expect("valid Unix timestamp")
}

pub struct AssistantAgent {
    pub time_policy_config: TimePolicyConfig,
    pub current_mode: WakeMode,
    pub role: String,
    pub conversation_log: Vec<ChatMessage>,
    pub active_tasks: Vec<TaskTracking>,
    /// Number of conversations processed since last summary LLM call
    pub pending_conversation_count: u32,
    /// Assistant responses queued for the scheduler to send via Feishu
    pub pending_responses: Vec<String>,
    /// Maps task_id → thread_id for tracking which thread each task is in
    pub active_threads: HashMap<String, String>,
}

impl AssistantAgent {
    pub fn new() -> Self {
        Self {
            time_policy_config: TimePolicyConfig::default(),
            current_mode: WakeMode::Proactive,
            role: Self::default_role().into(),
            conversation_log: Vec::new(),
            active_tasks: Vec::new(),
            pending_conversation_count: 0,
            pending_responses: Vec::new(),
            active_threads: HashMap::new(),
        }
    }

    /// Returns true if there are unsummarized conversations worth a summary LLM call
    pub fn has_pending_summaries(&self) -> bool {
        self.pending_conversation_count >= 3
    }

    /// Get and clear all pending assistant responses (for scheduler to send via Feishu)
    pub fn drain_responses(&mut self) -> Vec<String> {
        self.pending_responses.drain(..).collect()
    }

    /// Generate a dispatch action that includes thread context
    pub fn create_dispatch_action(
        &mut self,
        target_agent: &str,
        message: &str,
        thread_id: Option<&str>,
    ) -> AssistantAction {
        let full_message = match thread_id {
            Some(tid) => {
                // Track the task-thread mapping
                self.active_threads
                    .insert(format!("{target_agent}-{}", uuid::Uuid::now_v7()), tid.to_string());
                format!("[Thread: {}] {}", tid, message)
            }
            None => message.to_string(),
        };
        AssistantAction::Dispatch {
            target_agent: target_agent.to_string(),
            message: full_message,
        }
    }

    pub fn default_role() -> &'static str {
        r#"You are the user's (boss) AI assistant, responsible for managing the entire Agent team.

Core Responsibilities:
1. **Task Dispatch** — When the user makes a request, break it down and assign tasks to the right Agent. Decide who is best suited based on Agent name and role.
2. **Progress Tracking** — Track all dispatched tasks. Update status when Agents report completion.
3. **Progress Summaries** — Periodically report overall progress to the user.
4. **Knowledge Retention** — Store important decisions, technical solutions, and product decisions into the memory system.

Behavior Rules:
- During busy hours (weekdays 9:00-18:00): proactive, respond quickly
- During idle hours (other times): silent archiving, do not disturb unless urgent
- Urgent keywords ("urgent", "production outage", "P0", "crash"): handle immediately, overrides all limits
- When the user directly @mentions another Agent, do not intervene. The Agent notifies the assistant when done.

Summary Rules (Feishu messages):
- During busy hours (weekdays 9:00-18:00): summarize every 15 minutes, batch all updates into one message
- During idle hours: summarize every 6 hours
- Urgent situations (production outage, P0, crash): notify immediately, bypass summary interval
- Always batch multiple updates into a single summary message — do not push one at a time

Output Format:
You MUST reply in JSON format with a reasoning field and an actions array.
reasoning: explain your thought process in English (this is internal thinking, not shown to the user).
Each action in the actions array must have a type field:
- "dispatch": assign a task to an Agent. Requires target_agent and message.
- "respond": reply directly to the user. Requires message.
- "store_memory": store in memory. Requires title, summary, importance(1-10)."#
    }

    /// Process an incoming message through LLM and return actions
    pub async fn process_message(
        &mut self,
        message: &str,
        sender: &str,
        llm_gateway: &LlmGateway,
        model_config: &ModelConfig,
    ) -> Result<Vec<AssistantAction>, CoreError> {
        // Build LLM messages before updating the conversation log
        let mut messages = vec![ChatMessage { reasoning_content: None, role: "user".into(),
            content: format!(
                "Current time policy: {:?}\n\nMessage from {}: {}",
                self.current_mode, sender, message
            ),
        }];

        // Add relevant conversation history (last 10 turns from existing log)
        for msg in self.conversation_log.iter().rev().take(10).rev() {
            messages.push(msg.clone());
        }

        let request = ChatRequest {
            model: model_config.model.clone(),
            system_prompt: self.role.clone(),
            messages,
            tools: vec![],
        };

        // Call LLM
        let response = llm_gateway.chat(model_config, &request).await?;

        // Add message + response to conversation log AFTER LLM call
        self.conversation_log.push(ChatMessage { reasoning_content: None, role: if sender == "user" {
                "user".into()
            } else {
                "assistant".into()
            },
            content: message.into(),
        });
        self.conversation_log.push(ChatMessage { reasoning_content: None, role: "assistant".into(),
            content: response.content.clone(),
        });

        // Mark this conversation as needing summarization
        self.pending_conversation_count += 1;

        // Parse JSON response
        let content = response.content.trim();
        // Find JSON block in response (handle markdown-wrapped JSON)
        let json_str = if let Some(start) = content.find('{') {
            if let Some(end) = content.rfind('}') {
                &content[start..=end]
            } else {
                content
            }
        } else {
            content
        };

        let assistant_resp: AssistantResponse = serde_json::from_str(json_str).map_err(|e| {
            CoreError::Assistant(format!("Failed to parse LLM response as JSON: {e}\nRaw content: {content}"))
        })?;

        // Log reasoning
        tracing::info!("[Assistant] Reasoning: {}", assistant_resp.reasoning);

        // Track dispatched tasks and log memory actions
        for action in &assistant_resp.actions {
            match action {
                AssistantAction::Dispatch {
                    target_agent,
                    message: task_msg,
                } => {
                    let task = TaskTracking {
                        id: uuid::Uuid::now_v7().to_string(),
                        description: task_msg.clone(),
                        assigned_to: target_agent.clone(),
                        created_at: utc_now(),
                        status: TaskStatus::Pending,
                    };
                    tracing::info!(
                        "[Assistant] Dispatched task {} to {target_agent}: {task_msg}",
                        task.id
                    );
                    self.active_tasks.push(task);
                }
                AssistantAction::StoreMemory {
                    title,
                    summary,
                    importance,
                } => {
                    tracing::info!(
                        "[Assistant] Memory stored: {title} (importance={importance}) — {summary}"
                    );
                }
                AssistantAction::Respond { message: resp_msg } => {
                    tracing::info!("[Assistant] Response queued: {resp_msg}");
                    self.pending_responses.push(resp_msg.clone());
                }
            }
        }

        Ok(assistant_resp.actions)
    }

    /// Check for task timeouts and return escalation actions
    pub fn check_escalations(&self) -> Vec<AssistantAction> {
        let mut escalations = Vec::new();
        let now = utc_now();

        for task in &self.active_tasks {
            if task.status == TaskStatus::Pending || task.status == TaskStatus::InProgress {
                let elapsed = now - task.created_at;
                let timeout_minutes: i64 = if self.current_mode == WakeMode::Proactive {
                    10
                } else {
                    120
                };

                if elapsed > chrono::Duration::minutes(timeout_minutes) {
                    let preview: String = task
                        .description
                        .chars()
                        .take(50)
                        .collect();
                    escalations.push(AssistantAction::Respond {
                        message: format!(
                            "⚠️ Task for {} — '{}' — has exceeded {} minutes without completion. Please check.",
                            task.assigned_to, preview, timeout_minutes,
                        ),
                    });
                }
            }
        }

        escalations
    }

    /// Evaluate the current time policy based on wall clock and message content
    pub fn evaluate(&mut self, message: &str) -> &TimePolicyConfig {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let policy = self.time_policy_config.resolve(now, message);
        self.current_mode = policy.wake_mode.clone();
        &self.time_policy_config
    }

    /// Check if an urgent keyword is present (convenience method)
    pub fn is_urgent(&self, message: &str) -> bool {
        self.time_policy_config
            .urgent_keywords
            .iter()
            .any(|k| message.contains(k.as_str()))
    }
}
