use std::cmp::Ordering;
use std::collections::BinaryHeap;

#[derive(Debug, Clone)]
pub struct InboxMessage {
    pub priority: u8,
    pub content: String,
    pub thread_id: Option<String>,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(priority: u8, content: &str) -> InboxMessage {
        InboxMessage {
            priority,
            content: content.to_string(),
            thread_id: None,
            received_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64,
        }
    }

    #[test]
    fn test_priority_ordering() {
        let mut inbox = PriorityInbox::new();
        inbox.push(msg(3, "low"));
        inbox.push(msg(0, "high"));
        inbox.push(msg(1, "medium"));

        assert_eq!(inbox.pop().unwrap().content, "high");
        assert_eq!(inbox.pop().unwrap().content, "medium");
        assert_eq!(inbox.pop().unwrap().content, "low");
        assert!(inbox.is_empty());
    }

    #[test]
    fn test_empty_inbox() {
        let mut inbox = PriorityInbox::new();
        assert!(inbox.is_empty());
        assert_eq!(inbox.len(), 0);
        assert!(inbox.pop().is_none());
    }

    #[test]
    fn test_same_priority_fifo() {
        let mut inbox = PriorityInbox::new();
        inbox.push(msg(1, "first"));
        std::thread::sleep(std::time::Duration::from_millis(2));
        inbox.push(msg(1, "second"));
        assert_eq!(inbox.pop().unwrap().content, "first");
        assert_eq!(inbox.pop().unwrap().content, "second");
    }
}
