use serde::{Deserialize, Serialize};

pub type ChatId = String;
pub type MessageId = String;
pub type ThreadId = String;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeishuMessage {
    pub message_id: MessageId,
    pub chat_id: ChatId,
    pub thread_id: Option<ThreadId>,
    pub sender: SenderInfo,
    pub content: String,
    pub msg_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SenderInfo {
    pub id: String,
    pub name: String,
}

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

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum MessagePriority {
    UserDirect = 0,
    Urgent = 1,
    Secretary = 2,
    InterAgent = 3,
}
