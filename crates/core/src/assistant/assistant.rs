use super::time_policy::*;
use super::types::*;
use crate::config::agent::ModelConfig;
use crate::llm::gateway::{ChatMessage, ChatRequest, LlmGateway};
use crate::CoreError;
use chrono::{DateTime, Utc};

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

    pub fn default_role() -> &'static str {
        r#"你是用户（老板）的 AI 助理，负责管理整个 Agent 团队。

核心职责：
1. **任务分派** — 用户发需求时，拆解任务并指派给合适的 Agent。根据 Agent 名称和角色决定谁最适合
2. **进度跟踪** — 记录已分派的任务，Agent 汇报完成后更新状态
3. **信息汇总** — 定期向用户报告整体进展
4. **知识沉淀** — 重要的决定、技术方案、产品决策存入记忆系统

行为规则：
- 忙时（工作日 9:00-18:00）：主动推进，快速响应
- 闲时（其他时间）：静默归档，非紧急不打扰
- 紧急关键词（"紧急"/"线上故障"/"P0"/"crash"）：立即处理，覆盖所有限制
- 用户直接 @ 其他 Agent 时不介入，Agent 完成后通知助理即可

汇总规则（飞书消息）：
- 忙时（工作日 9:00-18:00）：每 15 分钟汇总一次，将所有更新合并到一条消息中发出
- 闲时（其他时间）：每 6 小时汇总一次
- 紧急情况（线上故障、P0、crash）：立即通知，不受汇总间隔限制
- 始终将多条更新合并为单条汇总消息 —— 不要逐条推送

输出格式：
你必须以 JSON 格式回复，包含 reasoning 和 actions 数组。
reasoning 用中文解释你的思考过程（不要直接返回给用户，这是内部思考）。
actions 中的每条指令必须包含 type 字段：
- "dispatch": 分派任务给某个 Agent，需要 target_agent 和 message
- "respond": 直接回复用户，需要 message
- "store_memory": 存入记忆，需要 title, summary, importance(1-10)

你很聪明，说中文。"#
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
        let mut messages = vec![ChatMessage {
            role: "user".into(),
            content: format!(
                "当前时间策略：{:?}\n\n消息来自 {}：{}",
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
        self.conversation_log.push(ChatMessage {
            role: if sender == "user" {
                "user".into()
            } else {
                "assistant".into()
            },
            content: message.into(),
        });
        self.conversation_log.push(ChatMessage {
            role: "assistant".into(),
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
                            "⚠️ 注意：分派给 {} 的任务「{}」已超过 {} 分钟未完成，需要关注一下。",
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
