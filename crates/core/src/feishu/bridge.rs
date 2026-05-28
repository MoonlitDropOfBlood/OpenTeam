use std::sync::Arc;
use tokio::sync::broadcast;
use super::channel_bridge::FeishuChannelBridge;
use super::message_queue::SendQueue;
use super::types::*;
use crate::CoreError;

/// FeishuBridge — high-level API for Feishu messaging.
///
/// Internally delegates to `FeishuChannelBridge` (IPC with the Node.js Channel SDK plugin).
///
/// # Migration from lark-cli
/// Previously this module spawned `lark-cli` subprocesses directly. Now it communicates
/// with a Node.js plugin that uses `@larksuiteoapi/node-sdk`'s `createLarkChannel()`.
/// The `send_queued()` method preserves the 5 QPS rate limiter; all other methods now
/// call the Channel Bridge instead of `lark-cli`.
#[derive(Clone)]
pub struct FeishuBridge {
    /// The Channel Bridge (IPC to Node.js plugin). Set after `start()`.
    channel_bridge: Arc<tokio::sync::Mutex<Option<FeishuChannelBridge>>>,
    queue: SendQueue,
}

impl FeishuBridge {
    pub fn new() -> Self {
        Self {
            channel_bridge: Arc::new(tokio::sync::Mutex::new(None)),
            queue: SendQueue::new(),
        }
    }

    /// Register the Channel Bridge after the plugin has been started.
    pub async fn set_channel_bridge(&self, bridge: FeishuChannelBridge) {
        let mut guard = self.channel_bridge.lock().await;
        *guard = Some(bridge);
        tracing::info!("[FeishuBridge] Channel Bridge registered");
    }

    pub fn queue(&self) -> &SendQueue {
        &self.queue
    }

    /// Format @mention text for Feishu message
    pub fn format_mention(target: &MentionTarget) -> String {
        format!(r#"<at user_id="{}">{}</at>"#, target.user_id, target.name)
    }

    /// Send a text message to a Feishu group chat (via Channel Bridge).
    pub async fn send_message(&self, msg: &OutgoingMessage) -> Result<MessageId, CoreError> {
        let guard = self.channel_bridge.lock().await;
        let bridge = guard.as_ref()
            .ok_or_else(|| CoreError::Feishu("Channel Bridge not initialized — call start() first".into()))?;
        bridge.send_message(msg).await
    }

    /// Reply to a message in a thread (via Channel Bridge).
    pub async fn reply_to_message(
        &self,
        message_id: &str,
        text: &str,
        reply_in_thread: bool,
    ) -> Result<MessageId, CoreError> {
        let guard = self.channel_bridge.lock().await;
        let bridge = guard.as_ref()
            .ok_or_else(|| CoreError::Feishu("Channel Bridge not initialized".into()))?;
        bridge.reply_to_message(message_id, text, reply_in_thread).await
    }

    /// Check Feishu connection status (via Channel Bridge).
    pub async fn check_auth(&self) -> Result<bool, CoreError> {
        let guard = self.channel_bridge.lock().await;
        if let Some(bridge) = guard.as_ref() {
            bridge.check_connection().await
        } else {
            Ok(false)
        }
    }

    /// Subscribe to incoming Feishu messages (via Channel Bridge).
    pub fn subscribe_messages(&self) -> Option<broadcast::Receiver<NormalizedMessage>> {
        let guard = self.channel_bridge.try_lock().ok()?;
        guard.as_ref().map(|b| b.subscribe_messages())
    }

    /// Start consuming events from Feishu via Channel SDK WebSocket.
    ///
    /// This replaces the old `lark-cli event +subscribe` subprocess approach.
    /// The Channel Bridge manages the WebSocket connection internally.
    pub async fn subscribe_events(
        &self,
        _event_types: &[&str],
    ) -> Result<(), CoreError> {
        // Events are automatically subscribed when the Channel Bridge connects.
        // The Channel SDK subscribes to `im.message.receive_v1` by default.
        tracing::info!("[FeishuBridge] Events handled by Channel Bridge (no-op)");
        Ok(())
    }

    /// Read next event from event subscription (DEPRECATED — use subscribe_messages() instead).
    ///
    /// This was previously used with the lark-cli subprocess stdout reader.
    /// With the Channel Bridge, incoming messages are pushed via broadcast channel.
    pub async fn read_event(_reader: &mut ()) -> Option<String> {
        None
    }

    /// Parse event JSON to extract message content (DEPRECATED).
    ///
    /// With the Channel Bridge, messages are already parsed into `NormalizedMessage`.
    pub fn parse_message_event(_json: &str) -> Result<FeishuMessage, CoreError> {
        Err(CoreError::Feishu("parse_message_event deprecated — use Channel Bridge subscribe_messages()".into()))
    }

    /// Send a message through the queue (respects 5 QPS).
    pub async fn send_queued(
        &self,
        msg: OutgoingMessage,
        agent_id: &str,
    ) -> Result<MessageId, CoreError> {
        self.queue.enqueue(msg, agent_id.to_string()).await;
        Ok("queued".into())
    }
}

impl Default for FeishuBridge {
    fn default() -> Self {
        Self::new()
    }
}