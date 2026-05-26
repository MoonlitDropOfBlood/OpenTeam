use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::Mutex as TokioMutex;
use super::bridge::FeishuBridge;
use super::types::OutgoingMessage;

#[derive(Debug)]
pub struct SendQueueEntry {
    pub message: OutgoingMessage,
    pub enqueued_at: Instant,
    pub agent_id: String,
}

#[derive(Clone)]
pub struct SendQueue {
    queue: Arc<TokioMutex<VecDeque<SendQueueEntry>>>,
}

impl SendQueue {
    pub fn new() -> Self {
        Self { queue: Arc::new(TokioMutex::new(VecDeque::new())) }
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
    pub fn start_consumer(
        queue: Arc<SendQueue>,
        bridge: Arc<Mutex<Option<FeishuBridge>>>,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_millis(200)).await; // 5 QPS max

                if let Some(entry) = queue.dequeue().await {
                    tracing::info!(
                        "[SendQueue] Sending: {}",
                        &entry.message.text[..entry.message.text.len().min(60)],
                    );

                    // Try to send via bridge (clone to drop MutexGuard before await)
                    let bridge_opt = bridge.lock().unwrap().clone();
                    if let Some(bridge) = bridge_opt {
                        match bridge.send_message(&entry.message).await {
                            Ok(msg_id) => {
                                tracing::info!("[SendQueue] Sent: message_id={msg_id}");
                            }
                            Err(e) => {
                                tracing::error!("[SendQueue] Send failed: {e}");
                            }
                        }
                    } else {
                        tracing::warn!("[SendQueue] FeishuBridge not available");
                    }
                }
            }
        })
    }
}
