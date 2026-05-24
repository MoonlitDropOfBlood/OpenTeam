use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub type MemoryId = Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub id: MemoryId,
    pub agent_id: String,
    pub memory_type: MemoryType,
    pub title: String,
    pub summary: String,
    pub decisions: Vec<Decision>,
    pub artifacts: Vec<ArtifactRef>,
    pub pending_todos: Vec<String>,
    pub importance: u8,
    pub embedding: Option<Vec<f32>>,
    pub turn_indices: Vec<usize>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub last_accessed: chrono::DateTime<chrono::Utc>,
    pub access_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Decision {
    pub title: String,
    pub decision: String,
    pub reason: String,
    pub made_by: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactRef {
    pub name: String,
    pub url: String,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MemoryType {
    ShortTerm,
    LongTerm,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConfig {
    pub short_term_max_age_days: u32,
    pub short_term_max_count: u32,
    pub long_term_max_count: u32,
    pub retention_importance: u8,
    pub base_decay_rate: f64,
    pub retrieval_boost: f64,
    pub raw_retention_days: u32,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            short_term_max_age_days: 30,
            short_term_max_count: 500,
            long_term_max_count: 2000,
            retention_importance: 7,
            base_decay_rate: 0.01,
            retrieval_boost: 0.1,
            raw_retention_days: 7,
        }
    }
}

#[derive(Debug, Clone)]
pub struct MemorySearchResult {
    pub entry: MemoryEntry,
    pub score: f64,
    pub semantic_score: f64,
    pub decay_factor: f64,
}
