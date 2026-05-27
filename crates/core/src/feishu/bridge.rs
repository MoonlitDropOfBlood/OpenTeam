use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use super::message_queue::SendQueue;
use super::types::*;
use crate::CoreError;

#[derive(Clone)]
pub struct FeishuBridge {
    queue: SendQueue,
}

impl FeishuBridge {
    pub fn new() -> Self {
        Self { queue: SendQueue::new() }
    }

    pub fn queue(&self) -> &SendQueue {
        &self.queue
    }

    /// Format @mention text for Feishu message
    pub fn format_mention(target: &MentionTarget) -> String {
        format!(r#"<at user_id="{}">{}</at>"#, target.user_id, target.name)
    }

    /// Send a text message to a Feishu group chat
    pub async fn send_message(&self, msg: &OutgoingMessage) -> Result<MessageId, CoreError> {
        let mut text = msg.text.clone();

        // Prepend @mentions
        for mention in &msg.mentions {
            text = format!("{} {}",
                Self::format_mention(mention),
                text
            );
        }

        let mut cmd = Command::new("lark-cli");
        cmd.arg("im")
            .arg("+messages-send")
            .arg("--chat-id")
            .arg(&msg.chat_id)
            .arg("--text")
            .arg(&text);

        let output = tokio::time::timeout(
            Duration::from_secs(30),
            cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).output(),
        )
        .await
        .map_err(|_| CoreError::Feishu("send_message timed out after 30s".into()))?
        .map_err(|e| CoreError::Feishu(format!("Failed to run lark-cli: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(CoreError::Feishu(format!("lark-cli error: {}", stderr)));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let message_id = stdout.trim().to_string();
        if message_id.is_empty() {
            return Err(CoreError::Feishu("lark-cli returned empty message ID".into()));
        }
        Ok(message_id)
    }

    /// Reply to a message in a thread
    pub async fn reply_to_message(
        &self,
        message_id: &str,
        text: &str,
        reply_in_thread: bool,
    ) -> Result<MessageId, CoreError> {
        let mut cmd = Command::new("lark-cli");
        cmd.arg("im")
            .arg("+messages-reply")
            .arg("--message-id")
            .arg(message_id)
            .arg("--text")
            .arg(text);

        if reply_in_thread {
            cmd.arg("--reply-in-thread");
        }

        let output = tokio::time::timeout(
            Duration::from_secs(30),
            cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).output(),
        )
        .await
        .map_err(|_| CoreError::Feishu("reply_to_message timed out after 30s".into()))?
        .map_err(|e| CoreError::Feishu(format!("Failed to reply: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(CoreError::Feishu(format!("lark-cli reply error: {}", stderr)));
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    /// Check Feishu CLI auth status
    pub async fn check_auth(&self) -> Result<bool, CoreError> {
        let output = tokio::time::timeout(
            Duration::from_secs(10),
            Command::new("lark-cli")
                .arg("auth")
                .arg("check")
                .output(),
        )
        .await
        .map_err(|_| CoreError::Feishu("check_auth timed out after 10s".into()))?
        .map_err(|e| CoreError::Feishu(format!("Auth check failed: {}", e)))?;

        Ok(output.status.success())
    }

    /// Start consuming events from Feishu WebSocket
    /// Returns the child process handle (caller must manage lifecycle)
    pub async fn subscribe_events(
        &self,
        event_types: &[&str],
    ) -> Result<Child, CoreError> {
        let mut cmd = Command::new("lark-cli");
        cmd.arg("event")
            .arg("+subscribe")
            .arg("--event-types")
            .arg(event_types.join(","))
            .arg("--compact")
            .arg("--quiet")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let child = cmd.spawn()
            .map_err(|e| CoreError::Feishu(format!("Failed to subscribe events: {}", e)))?;

        Ok(child)
    }

    /// Read next NDJSON line from event subscription output
    pub async fn read_event(
        reader: &mut BufReader<tokio::process::ChildStdout>,
    ) -> Option<String> {
        let mut line = String::new();
        reader.read_line(&mut line).await.ok()?;
        if line.trim().is_empty() {
            None
        } else {
            Some(line)
        }
    }

    /// Parse event JSON to extract message content
    pub fn parse_message_event(json: &str) -> Result<FeishuMessage, CoreError> {
        let value: serde_json::Value = serde_json::from_str(json)
            .map_err(|e| CoreError::Feishu(format!("Event parse error: {}", e)))?;

        let event = &value["event"];
        let message = &event["message"];

        Ok(FeishuMessage {
            message_id: message["message_id"].as_str().unwrap_or("").to_string(),
            chat_id: message["chat_id"].as_str().unwrap_or("").to_string(),
            thread_id: message["thread_id"].as_str().map(|s| s.to_string()),
            sender: SenderInfo {
                id: event["sender"]["sender_id"]["user_id"].as_str().unwrap_or("").to_string(),
                name: event["sender"]["name"].as_str().unwrap_or("").to_string(),
            },
            content: message["body"]["content"].as_str().unwrap_or("").to_string(),
            msg_type: message["msg_type"].as_str().unwrap_or("").to_string(),
        })
    }

    /// Send a message through the queue (respects 5 QPS)
    pub async fn send_queued(&self, msg: OutgoingMessage, agent_id: &str) -> Result<MessageId, CoreError> {
        self.queue.enqueue(msg, agent_id.to_string()).await;
        // Return a placeholder ID — real ID comes after send
        Ok("queued".into())
    }
}
