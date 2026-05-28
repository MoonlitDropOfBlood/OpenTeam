use serde::{Deserialize, Serialize};

// ---- Identity types (kept from original) ----

pub type ChatId = String;
pub type MessageId = String;
pub type ThreadId = String;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SenderInfo {
    pub id: String,
    pub name: String,
}

// ---- Legacy FeishuMessage (kept for backward compat) ----

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeishuMessage {
    pub message_id: MessageId,
    pub chat_id: ChatId,
    pub thread_id: Option<ThreadId>,
    pub sender: SenderInfo,
    pub content: String,
    pub msg_type: String,
}

// ---- Channel SDK NormalizedMessage (matches Node.js SDK format) ----

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalizedMessage {
    pub message_id: MessageId,
    pub chat_id: ChatId,
    pub chat_type: String, // "p2p" | "group"
    pub sender_id: String,
    pub sender_name: Option<String>,
    pub content: String,          // Normalized markdown content
    pub raw_content_type: String, // e.g. "text", "post", "image"
    pub mentions: Vec<MentionInfo>,
    pub mention_all: bool,
    pub mentioned_bot: bool,
    pub resources: Vec<ResourceDescriptor>,
    pub root_id: Option<String>,
    pub thread_id: Option<ThreadId>,
    pub reply_to_message_id: Option<MessageId>,
    pub create_time: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MentionInfo {
    pub user_id: String,
    pub name: String,
    pub is_bot: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceDescriptor {
    pub resource_type: String, // "image" | "file" | "audio" | "video" | "sticker"
    pub file_key: String,
}

// ---- Outbound message types ----

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendContent {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub markdown: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<String>,
    #[serde(default)]
    pub reply_in_thread: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mentions: Option<Vec<MentionInfo>>,
}

/// Input for channel.send()
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendInput {
    pub chat_id: ChatId,
    pub content: SendContent,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<SendOptions>,
}

/// Input for channel.stream() — content to stream as markdown chunks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamInput {
    pub chat_id: ChatId,
    pub chunks: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<SendOptions>,
}

// ---- Message priority (kept from original) ----

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum MessagePriority {
    UserDirect = 0,
    Urgent = 1,
    Secretary = 2,
    InterAgent = 3,
}

// ---- OutgoingMessage (kept from original for mcp/builtin compat) ----

#[derive(Debug, Clone)]
pub struct OutgoingMessage {
    pub chat_id: ChatId,
    pub thread_id: Option<ThreadId>,
    pub text: String,
    pub mentions: Vec<MentionTarget>,
    pub priority: MessagePriority,
}

#[derive(Debug, Clone)]
pub struct MentionTarget {
    pub user_id: String,
    pub name: String,
}

// ---- Channel event types ----

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CardActionEvent {
    pub action: CardActionValue,
    pub open_id: String,
    pub open_message_id: String,
    pub open_chat_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CardActionValue {
    pub value: serde_json::Value,
    pub tag: String,
}

// ---- Channel status ----

#[derive(Debug, Clone, PartialEq)]
pub enum ChannelStatus {
    Disconnected,
    Connecting,
    Connected { bot_name: String },
    Error(String),
}

// ---- IPC Protocol types ----

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcNotification {
    pub jsonrpc: String,
    pub id: Option<serde_json::Value>, // null for notifications
    pub method: String,
    pub params: serde_json::Value,
}