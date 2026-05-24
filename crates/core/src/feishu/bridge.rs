use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;
use super::message_queue::SendQueue;
use super::types::*;
use crate::CoreError;

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
}
