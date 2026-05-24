use std::cmp::Ordering;
use std::collections::BinaryHeap;

#[derive(Debug, Clone)]
pub struct InboxMessage {
    pub priority: u8,
    pub content: String,
    pub received_at: i64,
}

impl Eq for InboxMessage {}

impl PartialEq for InboxMessage {
    fn eq(&self, other: &Self) -> bool {
        self.priority == other.priority && self.received_at == other.received_at
    }
}

impl PartialOrd for InboxMessage {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for InboxMessage {
    fn cmp(&self, other: &Self) -> Ordering {
        other.priority.cmp(&self.priority)
            .then(self.received_at.cmp(&other.received_at))
    }
}

/// Priority queue for agent messages (non-FIFO)
pub struct PriorityInbox {
    queue: BinaryHeap<InboxMessage>,
}

impl PriorityInbox {
    pub fn new() -> Self {
        Self { queue: BinaryHeap::new() }
    }

    pub fn push(&mut self, msg: InboxMessage) {
        self.queue.push(msg);
    }

    pub fn pop(&mut self) -> Option<InboxMessage> {
        self.queue.pop()
    }

    pub fn len(&self) -> usize {
        self.queue.len()
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }
}
