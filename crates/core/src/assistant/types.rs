use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

/// Actions the assistant can take after processing a message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantResponse {
    pub reasoning: String,
    pub actions: Vec<AssistantAction>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AssistantAction {
    /// Dispatch a task to another agent
    #[serde(rename = "dispatch")]
    Dispatch {
        target_agent: String,
        message: String,
    },
    /// Respond directly to the user
    #[serde(rename = "respond")]
    Respond {
        message: String,
    },
    /// Store an important decision/topic in memory
    #[serde(rename = "store_memory")]
    StoreMemory {
        title: String,
        summary: String,
        importance: u8,
    },
}

/// Track a dispatched task
#[derive(Debug, Clone)]
pub struct TaskTracking {
    pub id: String,
    pub description: String,
    pub assigned_to: String,
    pub created_at: DateTime<Utc>,
    pub status: TaskStatus,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TaskStatus {
    Pending,
    InProgress,
    Completed,
    Blocked(String),
}
