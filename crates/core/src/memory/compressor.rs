use super::types::*;

fn now_utc() -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::<chrono::Utc>::from(std::time::SystemTime::now())
}

/// Compression result status
#[derive(Debug)]
pub enum CompressionResult {
    Success(MemoryEntry),
    Failed { raw: String, error: String },
}

/// Memory compression pipeline
pub struct Compressor;

impl Compressor {
    pub fn new() -> Self {
        Self
    }

    /// Compress a conversation into a structured MemoryEntry
    /// Uses rule-based for short conversations (<10 turns), LLM for longer ones
    pub fn compress(
        &self,
        conversation: &[ConversationTurn],
        importance: u8,
        agent_id: &str,
    ) -> CompressionResult {
        let turn_count = conversation.len();

        if turn_count < 10 {
            self.compress_rule_based(conversation, importance, agent_id, turn_count)
        } else {
            self.compress_with_llm(conversation, importance, agent_id, turn_count)
        }
    }

    fn compress_rule_based(
        &self,
        conversation: &[ConversationTurn],
        importance: u8,
        agent_id: &str,
        turn_count: usize,
    ) -> CompressionResult {
        let title = conversation.first()
            .map(|t| truncate(&t.content, 80))
            .unwrap_or_else(|| "Untitled conversation".to_string());

        let mut decisions = Vec::new();
        let artifacts = Vec::new();
        let pending_todos = Vec::new();

        for turn in conversation {
            let lower = turn.content.to_lowercase();
            if lower.contains("决定") || lower.contains("确定") || lower.contains("decide") {
                decisions.push(Decision {
                    title: truncate(&turn.content, 60),
                    decision: turn.content.clone(),
                    reason: "Extracted from conversation".into(),
                    made_by: turn.sender.clone(),
                });
            }
        }

        let summary = format!(
            "Conversation of {} turns between {}. Title: {}",
            turn_count,
            conversation.iter().map(|t| &t.sender[..]).collect::<Vec<_>>().join(", "),
            title
        );

        CompressionResult::Success(MemoryEntry {
            id: uuid::Uuid::now_v7(),
            agent_id: agent_id.to_string(),
            memory_type: MemoryType::ShortTerm,
            title: title.to_string(),
            summary,
            decisions,
            artifacts,
            pending_todos,
            importance,
            embedding: None,
            turn_indices: (0..turn_count).collect(),
            created_at: now_utc(),
            last_accessed: now_utc(),
            access_count: 0,
        })
    }

    fn compress_with_llm(
        &self,
        conversation: &[ConversationTurn],
        importance: u8,
        agent_id: &str,
        turn_count: usize,
    ) -> CompressionResult {
        // Phase 2 V1: falls back to rule-based
        // Full Ollama integration deferred to cleanup
        self.compress_rule_based(conversation, importance, agent_id, turn_count)
    }
}

#[derive(Debug, Clone)]
pub struct ConversationTurn {
    pub sender: String,
    pub content: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max { s.to_string() } else { format!("{}...", &s[..max]) }
}

/// Validate compressed output has required fields
pub fn validate_compression(entry: &MemoryEntry) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();
    if entry.title.is_empty() { errors.push("title is required".into()); }
    if entry.summary.is_empty() { errors.push("summary is required".into()); }
    if errors.is_empty() { Ok(()) } else { Err(errors) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_turn(sender: &str, content: &str) -> ConversationTurn {
        ConversationTurn {
            sender: sender.to_string(),
            content: content.to_string(),
            timestamp: chrono::DateTime::from(std::time::SystemTime::now()),
        }
    }

    #[test]
    fn test_rule_based_compression_short() {
        let compressor = Compressor::new();
        let turns = vec![
            make_turn("小红", "我决定使用JWT方案"),
            make_turn("CodeCat", "同意，性能更好"),
        ];
        let result = compressor.compress(&turns, 5, "小红");
        match result {
            CompressionResult::Success(entry) => {
                assert!(!entry.title.is_empty());
                assert!(!entry.summary.is_empty());
                assert_eq!(entry.importance, 5);
                assert_eq!(entry.agent_id, "小红");
                assert_eq!(entry.turn_indices.len(), 2);
            }
            CompressionResult::Failed { error, .. } => {
                panic!("Compression failed: {error}");
            }
        }
    }

    #[test]
    fn test_rule_based_extracts_decision() {
        let compressor = Compressor::new();
        let turns = vec![
            make_turn("小红", "经过讨论，决定使用Redis缓存"),
        ];
        let result = compressor.compress(&turns, 7, "小红");
        match result {
            CompressionResult::Success(entry) => {
                assert!(!entry.decisions.is_empty(), "Should have extracted a decision");
                assert!(entry.decisions[0].decision.contains("Redis"));
            }
            CompressionResult::Failed { error, .. } => {
                panic!("Compression failed: {error}");
            }
        }
    }

    #[test]
    fn test_validate_compression_valid() {
        let entry = MemoryEntry {
            id: uuid::Uuid::now_v7(),
            agent_id: "test".into(),
            memory_type: MemoryType::ShortTerm,
            title: "Valid Title".into(),
            summary: "Valid summary".into(),
            decisions: vec![],
            artifacts: vec![],
            pending_todos: vec![],
            importance: 5,
            embedding: None,
            turn_indices: vec![],
            created_at: chrono::DateTime::from(std::time::SystemTime::now()),
            last_accessed: chrono::DateTime::from(std::time::SystemTime::now()),
            access_count: 0,
        };
        assert!(validate_compression(&entry).is_ok());
    }

    #[test]
    fn test_validate_compression_empty_title() {
        let entry = MemoryEntry {
            title: "".into(),
            summary: "valid".into(),
            ..create_minimal()
        };
        assert!(validate_compression(&entry).is_err());
    }

    fn create_minimal() -> MemoryEntry {
        MemoryEntry {
            id: uuid::Uuid::now_v7(),
            agent_id: "test".into(),
            memory_type: MemoryType::ShortTerm,
            title: "t".into(),
            summary: "s".into(),
            decisions: vec![],
            artifacts: vec![],
            pending_todos: vec![],
            importance: 5,
            embedding: None,
            turn_indices: vec![],
            created_at: chrono::DateTime::from(std::time::SystemTime::now()),
            last_accessed: chrono::DateTime::from(std::time::SystemTime::now()),
            access_count: 0,
        }
    }
}
