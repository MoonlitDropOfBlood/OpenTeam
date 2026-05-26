use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use super::types::OutgoingMessage;

#[derive(Debug)]
pub struct SendQueueEntry {
    pub message: OutgoingMessage,
    pub enqueued_at: Instant,
    pub agent_id: String,
}

#[derive(Clone)]
pub struct SendQueue {
    queue: Arc<Mutex<VecDeque<SendQueueEntry>>>,
}

impl SendQueue {
    pub fn new() -> Self {
        Self { queue: Arc::new(Mutex::new(VecDeque::new())) }
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

    /// Start a background task that consumes from the queue at max 5 QPS
    pub fn start_consumer(queue: Arc<SendQueue>) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_millis(200)).await; // 5 QPS max

                if let Some(entry) = queue.dequeue().await {
                    tracing::info!(
                        "[SendQueue] Sending message via lark-cli: {}",
                        &entry.message.text[..entry.message.text.len().min(60)]
                    );
                    // Phase 3 V3: actual send via FeishuBridge.send_message()
                }
            }
        })
    }
}
