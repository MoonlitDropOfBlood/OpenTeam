use std::collections::VecDeque;
use std::time::Instant;
use tokio::sync::Mutex;
use super::types::OutgoingMessage;

pub struct SendQueueEntry {
    pub message: OutgoingMessage,
    pub enqueued_at: Instant,
    pub agent_id: String,
}

pub struct SendQueue {
    queue: Mutex<VecDeque<SendQueueEntry>>,
}

impl SendQueue {
    pub fn new() -> Self {
        Self { queue: Mutex::new(VecDeque::new()) }
    }

    pub async fn enqueue(&self, message: OutgoingMessage, agent_id: String) {
        let mut queue = self.queue.lock().await;
        let entry = SendQueueEntry {
            message,
            enqueued_at: Instant::now(),
            agent_id,
        };
        queue.push_back(entry);
    }

    pub async fn dequeue(&self) -> Option<SendQueueEntry> {
        let mut queue = self.queue.lock().await;
        queue.pop_front()
    }

    pub async fn len(&self) -> usize {
        self.queue.lock().await.len()
    }
}
