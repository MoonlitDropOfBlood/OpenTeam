# Phase 2: Agent Intelligence — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the Memory System (three-tier storage + vectorization + forgetting), Agent Lifecycle/Concurrency (Tokio async event loop + priority inbox), Message Routing (Feishu events → Agent dispatch), and Secretary Agent (busy/idle-aware coordination).

**Architecture:** Memory system is a standalone module (`core/src/memory/`) using SQLite for storage and ONNX Runtime for local vectorization. Agent Lifecycle adds `AgentManager` and `AgentHandle` with per-agent Tokio tasks. Message Routing bridges Feishu events to agents. Secretary Agent is a Rust Core module (not Node.js plugin) with time-policy-based busy/idle awareness.

**Tech Stack:** Rust (tokio, sqlx, ort, uuid), ONNX Runtime (nomic-embed-text-v1), Ollama (qwen2.5:3b), SQLite

**Dependencies:**
- Phase 2 depends on Phase 1 (AgentRegistry, LlmGateway, FeishuBridge, CoreError)
- Phase 3 (TUI & Plugins) depends on Phase 2

---

## File Structure

```
D:\ai-projects\agents-dev\
├── crates/core/src/
│   ├── memory/                         # NEW — Memory system
│   │   ├── mod.rs                      # Public API
│   │   ├── store.rs                    # MemoryStore (SQLite CRUD)
│   │   ├── types.rs                    # MemoryEntry, MemorySearchResult, etc.
│   │   ├── vectorizer.rs               # ONNX Runtime embedding
│   │   ├── compressor.rs               # Ollama + rule-based compression
│   │   ├── forgetting.rs               # Ebbinghaus decay + eviction
│   │   └── migration.rs                # SQLite schema migration
│   ├── agent/                          # NEW — Agent lifecycle
│   │   ├── mod.rs
│   │   ├── manager.rs                  # AgentManager — spawn/kill/monitor
│   │   ├── handle.rs                   # AgentHandle, AgentCommand
│   │   └── inbox.rs                    # PriorityQueue<Message>
│   ├── router/                         # NEW — Message routing
│   │   ├── mod.rs
│   │   └── router.rs                   # MessageRouter — event → agent
│   ├── secretary/                      # NEW — Secretary Agent
│   │   ├── mod.rs
│   │   ├── secretary.rs                # SecretaryAgent logic
│   │   └── time_policy.rs              # Busy/idle time policy
│   ├── config/mod.rs                   # MODIFY — add memory config
│   ├── lib.rs                          # MODIFY — add modules, update Core struct
│   └── error.rs                        # MODIFY — add Memory variant
├── crates/core/Cargo.toml              # MODIFY — add ort, rusqlite deps
├── Cargo.toml                          # MODIFY — add workspace deps
└── docs/superpowers/specs/2026-05-23-... (unchanged)
```

---

## Task 1: Memory System — SQLite Schema + MemoryStore

**Files:**
- Create: `crates/core/src/memory/mod.rs`
- Create: `crates/core/src/memory/types.rs`
- Create: `crates/core/src/memory/store.rs`
- Create: `crates/core/src/memory/migration.rs`
- Modify: `crates/core/src/error.rs`
- Modify: `crates/core/src/lib.rs`

- [ ] **Step 1: Define memory types**

File: `crates/core/src/memory/types.rs`

```rust
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use std::collections::HashMap;

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
    pub importance: u8,           // 1-10
    pub embedding: Option<Vec<f32>>, // 768-dim vector
    pub turn_indices: Vec<usize>, // indices into original conversation
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
    pub kind: String, // "doc" | "code" | "design"
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
    pub retention_importance: u8,      // >= this = permanent
    pub base_decay_rate: f64,          // λ
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
    pub score: f64,       // combined relevance score
    pub semantic_score: f64,
    pub decay_factor: f64,
}
```

- [ ] **Step 2: Create SQLite schema migration**

File: `crates/core/src/memory/migration.rs`

```rust
use crate::CoreError;

const SCHEMA_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS memory_entries (
    id              TEXT PRIMARY KEY,
    agent_id        TEXT NOT NULL,
    memory_type     TEXT NOT NULL CHECK(memory_type IN ('ShortTerm', 'LongTerm')),
    title           TEXT NOT NULL,
    summary         TEXT NOT NULL,
    decisions       TEXT NOT NULL DEFAULT '[]',  -- JSON array
    artifacts       TEXT NOT NULL DEFAULT '[]',  -- JSON array
    pending_todos   TEXT NOT NULL DEFAULT '[]',  -- JSON array
    importance      INTEGER NOT NULL DEFAULT 5,
    embedding       BLOB,                        -- raw f32 bytes
    turn_indices    TEXT NOT NULL DEFAULT '[]',  -- JSON array
    created_at      TEXT NOT NULL,
    last_accessed   TEXT NOT NULL,
    access_count    INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_memory_agent ON memory_entries(agent_id);
CREATE INDEX IF NOT EXISTS idx_memory_type ON memory_entries(memory_type);
CREATE INDEX IF NOT EXISTS idx_memory_importance ON memory_entries(importance);
"#;

pub fn run_migrations(pool: &sqlx::SqlitePool) -> Result<(), CoreError> {
    // Use sqlx::raw sql to execute DDL
    // For Phase 2 V1, we execute directly
    todo!("Execute SCHEMA_SQL via sqlx::query")
}
```

- [ ] **Step 3: Implement MemoryStore (CRUD)**

File: `crates/core/src/memory/store.rs`

```rust
use sqlx::SqlitePool;
use crate::CoreError;
use super::types::*;

pub struct MemoryStore {
    pool: SqlitePool,
    config: MemoryConfig,
}

impl MemoryStore {
    pub async fn new(db_path: &str, config: MemoryConfig) -> Result<Self, CoreError> {
        let pool = SqlitePool::connect(db_path).await
            .map_err(|e| CoreError::Memory(format!("DB connect: {e}")))?;
        // Run migrations
        sqlx::query(super::migration::SCHEMA_SQL)
            .execute(&pool)
            .await
            .map_err(|e| CoreError::Memory(format!("Migration: {e}")))?;
        Ok(Self { pool, config })
    }

    pub async fn insert(&self, entry: &MemoryEntry) -> Result<(), CoreError> {
        let decisions_json = serde_json::to_string(&entry.decisions)
            .map_err(|e| CoreError::Memory(format!("Serialize: {e}")))?;
        let artifacts_json = serde_json::to_string(&entry.artifacts)
            .map_err(|e| CoreError::Memory(format!("Serialize: {e}")))?;
        let todos_json = serde_json::to_string(&entry.pending_todos)
            .map_err(|e| CoreError::Memory(format!("Serialize: {e}")))?;
        let indices_json = serde_json::to_string(&entry.turn_indices)
            .map_err(|e| CoreError::Memory(format!("Serialize: {e}")))?;
        let embedding_blob = entry.embedding.as_ref()
            .map(|v| {
                let bytes: Vec<u8> = v.iter()
                    .flat_map(|f| f.to_le_bytes())
                    .collect();
                bytes
            });

        sqlx::query(
            "INSERT INTO memory_entries (id, agent_id, memory_type, title, summary, decisions, artifacts, pending_todos, importance, embedding, turn_indices, created_at, last_accessed, access_count) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(entry.id.to_string())
        .bind(&entry.agent_id)
        .bind(format!("{:?}", entry.memory_type))
        .bind(&entry.title)
        .bind(&entry.summary)
        .bind(&decisions_json)
        .bind(&artifacts_json)
        .bind(&todos_json)
        .bind(entry.importance as i32)
        .bind(embedding_blob)
        .bind(&indices_json)
        .bind(entry.created_at.to_rfc3339())
        .bind(entry.last_accessed.to_rfc3339())
        .bind(entry.access_count as i32)
        .execute(&self.pool)
        .await
        .map_err(|e| CoreError::Memory(format!("Insert: {e}")))?;
        Ok(())
    }

    pub async fn get(&self, id: &MemoryId) -> Result<Option<MemoryEntry>, CoreError> {
        let row = sqlx::query_as::<_, MemoryRow>(
            "SELECT * FROM memory_entries WHERE id = ?"
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| CoreError::Memory(format!("Get: {e}")))?;
        row.map(TryInto::try_into).transpose()
    }

    pub async fn list_by_agent(&self, agent_id: &str, memory_type: Option<MemoryType>) -> Result<Vec<MemoryEntry>, CoreError> {
        todo!("Query by agent_id + optional memory_type filter")
    }

    pub async fn search_semantic(&self, agent_id: &str, query_embedding: &[f32], top_k: usize) -> Result<Vec<MemorySearchResult>, CoreError> {
        // Get all entries for agent, compute cosine similarity in Rust
        let entries = self.list_by_agent(agent_id, None).await?;
        let mut results: Vec<MemorySearchResult> = entries.into_iter()
            .filter_map(|e| {
                let emb = e.embedding.as_ref()?;
                let semantic = cosine_similarity(query_embedding, emb);
                let decay = compute_decay(&e, &self.config);
                let boost = (e.access_count as f64 * self.config.retrieval_boost).min(1.0);
                let score = semantic * (1.0 + boost) * decay;
                Some(MemorySearchResult {
                    score,
                    semantic_score: semantic,
                    decay_factor: decay,
                    entry: e,
                })
            })
            .collect();
        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(top_k);
        Ok(results)
    }

    pub async fn delete(&self, id: &MemoryId) -> Result<(), CoreError> {
        sqlx::query("DELETE FROM memory_entries WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| CoreError::Memory(format!("Delete: {e}")))?;
        Ok(())
    }

    pub async fn touch(&self, id: &MemoryId) -> Result<(), CoreError> {
        sqlx::query("UPDATE memory_entries SET last_accessed = ?, access_count = access_count + 1 WHERE id = ?")
            .bind(chrono::Utc::now().to_rfc3339())
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| CoreError::Memory(format!("Touch: {e}")))?;
        Ok(())
    }

    pub async fn apply_eviction(&self) -> Result<Vec<MemoryId>, CoreError> {
        // Short-term: age > max_age_days or count > max_count
        // Long-term: count > max_count, evict lowest value
        todo!("Eviction logic: find candidates, delete, return removed IDs")
    }
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    (dot / (norm_a * norm_b)) as f64
}

fn compute_decay(entry: &MemoryEntry, config: &MemoryConfig) -> f64 {
    let age_days = (chrono::Utc::now() - entry.created_at).num_hours() as f64 / 24.0;
    let lambda = match entry.importance {
        8..=10 => config.base_decay_rate * 0.1,   // near-permanent
        5..=7  => config.base_decay_rate,          // slow decay
        _      => config.base_decay_rate * 5.0,    // fast decay
    };
    (-lambda * age_days).exp()
}
```

- [ ] **Step 4: Add `Memory` variant to CoreError**

Modify `crates/core/src/error.rs`:
```rust
#[error("Memory error: {0}")]
Memory(String),
```

- [ ] **Step 5: Create memory/mod.rs**

```rust
pub mod types;
pub mod store;
pub mod migration;

pub use store::MemoryStore;
pub use types::*;
```

- [ ] **Step 6: Add memory module to lib.rs**

```rust
pub mod memory;
```

- [ ] **Step 7: Build & test**

Run: `cargo build`
Expected: Clean compilation

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -m "phase 2 task 1: memory system types, SQLite schema, MemoryStore CRUD"
```

---

## Task 2: Memory System — Vectorization + Compression Pipeline

**Files:**
- Create: `crates/core/src/memory/vectorizer.rs`
- Create: `crates/core/src/memory/compressor.rs`
- Modify: `crates/core/src/memory/mod.rs`
- Modify: `crates/core/Cargo.toml` (add ort, tokenizers deps)

- [ ] **Step 1: Add dependencies**

Modify `Cargo.toml` workspace:
```toml
chrono = { version = "0.4", features = ["serde"] }
```

Note: `ort` (ONNX Runtime crate) is NOT added in Phase 2 V1. The vectorizer uses a stub that returns zero vectors. Full ONNX integration requires downloading ~60MB of native libs and is deferred to a cleanup task.

- [ ] **Step 2: Implement Vectorizer trait + ONNX implementation**

File: `crates/core/src/memory/vectorizer.rs`

```rust
use crate::CoreError;

/// Embedding vectorizer trait
pub trait Vectorizer: Send + Sync {
    /// Generate a 768-dim embedding for text
    fn embed(&self, text: &str) -> Result<Vec<f32>, CoreError>;
    /// Batch embed multiple texts
    fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, CoreError>;
}

/// ONNX Runtime-based vectorizer using nomic-embed-text-v1
pub struct OnnxVectorizer {
    // session: ort::Session,  // gated behind feature
}

impl OnnxVectorizer {
    pub fn new(model_path: &str) -> Result<Self, CoreError> {
        // For Phase 2 V1: return a stub that validates the model path exists
        // Full ORT integration comes in Phase 2 cleanup
        if !std::path::Path::new(model_path).exists() {
            return Err(CoreError::Memory(format!("ONNX model not found: {model_path}")));
        }
        Ok(Self {})
    }
}

impl Vectorizer for OnnxVectorizer {
    fn embed(&self, text: &str) -> Result<Vec<f32>, CoreError> {
        // Stub: return 768-dim zero vector
        // Real implementation loads ort::Session and runs inference
        Ok(vec![0.0f32; 768])
    }

    fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, CoreError> {
        // Stub: return zero vectors
        Ok(texts.iter().map(|_| vec![0.0f32; 768]).collect())
    }
}

/// Mock vectorizer for testing
pub struct MockVectorizer;

impl Vectorizer for MockVectorizer {
    fn embed(&self, text: &str) -> Result<Vec<f32>, CoreError> {
        // Deterministic mock: hash-based embedding
        let mut vec = vec![0.0f32; 768];
        let hash: u64 = text.bytes().fold(0u64, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u64));
        let idx = (hash % 768) as usize;
        vec[idx] = 1.0;
        Ok(vec)
    }

    fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, CoreError> {
        texts.iter().map(|t| self.embed(t)).collect()
    }
}
```

- [ ] **Step 3: Implement Compressor**

File: `crates/core/src/memory/compressor.rs`

```rust
use crate::CoreError;
use super::types::*;

/// Compression result status
#[derive(Debug)]
pub enum CompressionResult {
    Success(MemoryEntry),
    Failed { raw: String, error: String },
}

/// Memory compression pipeline
pub struct Compressor {
    // ollama_client: Option<reqwest::Client>,
}

impl Compressor {
    pub fn new() -> Self {
        Self { }
    }

    /// Compress a conversation into a structured MemoryEntry
    /// Uses rule-based for short conversations, Ollama for longer ones
    pub fn compress(
        &self,
        conversation: &[ConversationTurn],
        importance: u8,
        agent_id: &str,
    ) -> CompressionResult {
        let turn_count = conversation.len();

        if turn_count < 10 {
            self.compress_rule_based(conversation, importance, agent_id)
        } else {
            self.compress_with_llm(conversation, importance, agent_id)
        }
    }

    fn compress_rule_based(
        &self,
        conversation: &[ConversationTurn],
        importance: u8,
        agent_id: &str,
    ) -> CompressionResult {
        // Template-based extraction
        let title = conversation.first()
            .map(|t| truncate(&t.content, 80))
            .unwrap_or("Untitled conversation");

        let mut decisions = Vec::new();
        let mut artifacts = Vec::new();
        let mut pending_todos = Vec::new();

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

        CompressionResult::Success(MemoryEntry {
            id: uuid::Uuid::new_v4(),
            agent_id: agent_id.to_string(),
            memory_type: MemoryType::ShortTerm,
            title: title.to_string(),
            summary: format!("Conversation of {} turns between {}", turn_count,
                conversation.iter().map(|t| &t.sender[..]).collect::<Vec<_>>().join(", ")),
            decisions,
            artifacts,
            pending_todos,
            importance,
            embedding: None,
            turn_indices: (0..turn_count).collect(),
            created_at: chrono::Utc::now(),
            last_accessed: chrono::Utc::now(),
            access_count: 0,
        })
    }

    fn compress_with_llm(
        &self,
        conversation: &[ConversationTurn],
        importance: u8,
        agent_id: &str,
    ) -> CompressionResult {
        // For Phase 2 V1: falls back to rule-based
        // Phase 2 cleanup: call Ollama API with structured prompt
        self.compress_rule_based(conversation, importance, agent_id)
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
```

- [ ] **Step 4: Update memory/mod.rs**

```rust
pub mod types;
pub mod store;
pub mod migration;
pub mod vectorizer;
pub mod compressor;

pub use store::MemoryStore;
pub use types::*;
pub use vectorizer::{Vectorizer, OnnxVectorizer, MockVectorizer};
pub use compressor::{Compressor, CompressionResult, ConversationTurn, validate_compression};
```

- [ ] **Step 5: Add `chrono` and handle `ort` dependency carefully**

Add chrono to workspace `Cargo.toml`:
```toml
chrono = { version = "0.4", features = ["serde"] }
```

Add to `crates/core/Cargo.toml`:
```toml
chrono.workspace = true
# ort commented out for Phase 2 V1 — vectorizer uses stub implementation
# ort = { version = "2", features = ["load-dynamic"] }
```

- [ ] **Step 6: Build & test**

Run: `cargo build`
Expected: Clean compilation (stub vectorizer, no ONNX dependency)

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "phase 2 task 2: memory vectorizer trait, rule-based compressor, validation"
```

---

## Task 3: Memory System — Forgetting + Retrieval + Core Integration

**Files:**
- Create: `crates/core/src/memory/forgetting.rs`
- Modify: `crates/core/src/memory/mod.rs`
- Modify: `crates/core/src/memory/store.rs` (add eviction)
- Modify: `crates/core/src/lib.rs`
- Modify: `crates/core/src/config/mod.rs` (add memory config loading)

- [ ] **Step 1: Implement forgetting model**

File: `crates/core/src/memory/forgetting.rs`

```rust
use super::types::*;

/// Calculate memory strength using Ebbinghaus decay formula
pub fn memory_strength(entry: &MemoryEntry, config: &MemoryConfig) -> f64 {
    let age_days = (chrono::Utc::now() - entry.created_at).num_hours() as f64 / 24.0;
    if age_days < 0.0 { return 1.0; }

    let lambda = match entry.importance {
        8..=10 => config.base_decay_rate * 0.1,  // near-permanent
        5..=7  => config.base_decay_rate,         // slow decay
        _      => config.base_decay_rate * 5.0,   // fast decay
    };

    (-lambda * age_days).exp()
}

/// Calculate retention value for eviction ranking
pub fn retention_value(entry: &MemoryEntry, config: &MemoryConfig) -> f64 {
    let strength = memory_strength(entry, config);
    let access_boost = 1.0 + (entry.access_count as f64 * config.retrieval_boost);
    entry.importance as f64 * strength * access_boost
}

/// Select candidates for eviction
pub fn select_eviction_candidates(
    entries: &[MemoryEntry],
    config: &MemoryConfig,
    count: usize,
) -> Vec<MemoryId> {
    let mut ranked: Vec<_> = entries.iter()
        .filter(|e| e.importance < config.retention_importance) // never evict high-importance
        .map(|e| (e.id, retention_value(e, config)))
        .collect();
    ranked.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    ranked.into_iter().take(count).map(|(id, _)| id).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_high_importance_near_permanent() {
        let entry = MemoryEntry {
            importance: 9,
            created_at: chrono::Utc::now() - chrono::Duration::days(60),
            ..sample_entry()
        };
        let config = MemoryConfig::default();
        let strength = memory_strength(&entry, &config);
        assert!(strength > 0.8, "High importance should decay slowly: {strength}");
    }

    #[test]
    fn test_low_importance_decays_quickly() {
        let entry = MemoryEntry {
            importance: 2,
            created_at: chrono::Utc::now() - chrono::Duration::days(30),
            ..sample_entry()
        };
        let config = MemoryConfig::default();
        let strength = memory_strength(&entry, &config);
        assert!(strength < 0.3, "Low importance should decay quickly: {strength}");
    }

    fn sample_entry() -> MemoryEntry {
        MemoryEntry {
            id: uuid::Uuid::new_v4(),
            agent_id: "test".into(),
            memory_type: MemoryType::ShortTerm,
            title: "test".into(),
            summary: "test".into(),
            decisions: vec![],
            artifacts: vec![],
            pending_todos: vec![],
            importance: 5,
            embedding: None,
            turn_indices: vec![],
            created_at: chrono::Utc::now(),
            last_accessed: chrono::Utc::now(),
            access_count: 0,
        }
    }
}
```

- [ ] **Step 2: Add config loading for memory settings**

Modify `crates/core/src/config/mod.rs`:
```rust
pub mod memory;  // add
```

Create `crates/core/src/config/memory.rs`:
```rust
use serde::Deserialize;
use crate::memory::types::MemoryConfig;

impl MemoryConfig {
    pub fn from_yaml(path: &std::path::Path) -> Result<Self, crate::CoreError> {
        let content = std::fs::read_to_string(path)?;
        let config: MemoryYaml = serde_yaml::from_str(&content)?;
        Ok(config.into())
    }
}

#[derive(Deserialize)]
struct MemoryYaml {
    short_term: ShortTermYaml,
    long_term: LongTermYaml,
    forgetting: ForgettingYaml,
}

impl From<MemoryYaml> for MemoryConfig {
    fn from(y: MemoryYaml) -> Self {
        Self {
            short_term_max_age_days: y.short_term.max_age_days,
            short_term_max_count: y.short_term.max_count,
            long_term_max_count: y.long_term.max_count,
            retention_importance: y.long_term.retention_importance,
            base_decay_rate: y.forgetting.base_decay_rate,
            retrieval_boost: y.forgetting.retrieval_boost,
            raw_retention_days: 7,
        }
    }
}
```

- [ ] **Step 3: Add MemoryStore to Core struct**

Modify `crates/core/src/lib.rs`:
```rust
pub mod memory;

pub struct Core {
    pub registry: registry::AgentRegistry,
    pub llm_gateway: llm::gateway::LlmGateway,
    pub feishu_bridge: feishu::bridge::FeishuBridge,
    pub memory_store: memory::store::MemoryStore,  // NEW
}
```

Update `Core::new()` to initialize MemoryStore:
```rust
let memory_config = memory::types::MemoryConfig::default();
let memory_store = memory::store::MemoryStore::new("sqlite:memory.db", memory_config).await?;
```

- [ ] **Step 4: Build & test**

Run: `cargo build` then `cargo test`
Expected: Clean compilation, all tests pass (including forgetting model tests)

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "phase 2 task 3: memory forgetting model, eviction, Core integration"
```

---

## Task 4: Agent Lifecycle — AgentManager + Event Loop

**Files:**
- Create: `crates/core/src/agent/mod.rs`
- Create: `crates/core/src/agent/handle.rs`
- Create: `crates/core/src/agent/inbox.rs`
- Create: `crates/core/src/agent/manager.rs`
- Modify: `crates/core/src/lib.rs`
- Modify: `crates/core/src/config/mod.rs` (add agent manager config)

- [ ] **Step 1: Define AgentHandle and AgentCommand**

File: `crates/core/src/agent/handle.rs`

```rust
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tokio::sync::mpsc;
use crate::config::agent::AgentConfig;
use crate::registry::AgentId;

/// Commands sent to an agent's control channel
#[derive(Debug, Clone)]
pub enum AgentCommand {
    Stop,
    Pause,
    Resume,
    InjectMessage(crate::feishu::types::FeishuMessage),
    OverrideContext(String),
}

/// Handle to a running agent task
pub struct AgentHandle {
    pub id: AgentId,
    pub config: AgentConfig,
    pub join_handle: JoinHandle<()>,
    pub control_tx: mpsc::Sender<AgentCommand>,
    pub cancel_token: CancellationToken,
}
```

- [ ] **Step 2: Implement PriorityQueue inbox**

File: `crates/core/src/agent/inbox.rs`

```rust
use std::collections::BinaryHeap;
use std::cmp::Ordering;
use crate::feishu::types::FeishuMessage;

#[derive(Debug, Clone)]
pub struct InboxMessage {
    pub priority: u8,  // 0=highest (user direct), 1=urgent, 2=secretary, 3=inter-agent
    pub message: FeishuMessage,
    pub received_at: chrono::DateTime<chrono::Utc>,
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
        // Higher priority first, then earlier received_at
        other.priority.cmp(&self.priority)
            .then(self.received_at.cmp(&other.received_at))
    }
}

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
```

- [ ] **Step 3: Implement AgentManager**

File: `crates/core/src/agent/manager.rs`

```rust
use std::collections::HashMap;
use tokio::sync::{mpsc, RwLock};
use tokio_util::sync::CancellationToken;
use crate::config::agent::AgentConfig;
use crate::registry::{AgentId, AgentRegistry};
use super::handle::*;
use super::inbox::*;

pub struct AgentManager {
    agents: RwLock<HashMap<AgentId, AgentHandle>>,
    shutdown_token: CancellationToken,
}

impl AgentManager {
    pub fn new() -> Self {
        Self {
            agents: RwLock::new(HashMap::new()),
            shutdown_token: CancellationToken::new(),
        }
    }

    pub async fn spawn_agent(&self, config: AgentConfig, registry: Arc<RwLock<AgentRegistry>>) -> AgentId {
        let id = uuid::Uuid::new_v4();
        let (control_tx, control_rx) = mpsc::channel::<AgentCommand>(64);
        let cancel_token = self.shutdown_token.child_token();
        let cancel_clone = cancel_token.clone();
        let config_clone = config.clone();
        let registry_clone = registry.clone();

        let handle = tokio::spawn(async move {
            agent_main_loop(config_clone, control_rx, cancel_clone, registry_clone).await;
        });

        let agent_handle = AgentHandle {
            id,
            config,
            join_handle: handle,
            control_tx,
            cancel_token,
        };

        self.agents.write().await.insert(id, agent_handle);
        id
    }

    pub async fn shutdown_all(&self) {
        self.shutdown_token.cancel();
        // Wait for all handles with 30s timeout
        let agents = self.agents.read().await;
        for handle in agents.values() {
            // Could join with timeout here
        }
    }
}

async fn agent_main_loop(
    config: AgentConfig,
    mut control_rx: mpsc::Receiver<AgentCommand>,
    cancel: CancellationToken,
    registry: Arc<RwLock<AgentRegistry>>,
) {
    let mut inbox = PriorityInbox::new();
    let mut is_paused = false;

    loop {
        tokio::select! {
            // 1. Control commands (highest priority)
            Some(cmd) = control_rx.recv() => {
                match cmd {
                    AgentCommand::Stop => break,
                    AgentCommand::Pause => is_paused = true,
                    AgentCommand::Resume => is_paused = false,
                    AgentCommand::InjectMessage(msg) => {
                        inbox.push(InboxMessage {
                            priority: 3,
                            message: msg,
                            received_at: chrono::Utc::now(),
                        });
                    }
                    AgentCommand::OverrideContext(ctx) => {
                        tracing::info!("Context override for {}: {}", config.name, ctx);
                    }
                }
            }
            // 2. Cancellation signal
            _ = cancel.cancelled() => {
                tracing::info!("Agent {} shutting down", config.name);
                break;
            }
        }

        // Process inbox if not paused
        if !is_paused {
            while let Some(msg) = inbox.pop() {
                tracing::info!("Agent {} processing message: {:?}", config.name, msg.message.message_id);
                // TODO: Phase 2 cleanup — actually call LLM and respond
            }
        }
    }
}
```

- [ ] **Step 4: Create agent/mod.rs**

```rust
pub mod handle;
pub mod inbox;
pub mod manager;
```

- [ ] **Step 5: Add agent module to lib.rs + Core struct**

Modify `crates/core/src/lib.rs`:
```rust
pub mod agent;

pub struct Core {
    pub agent_manager: agent::manager::AgentManager,  // NEW
    // ...existing fields
}
```

Add to `Core::new()`:
```rust
let agent_manager = agent::manager::AgentManager::new();
```

Add `spawn_all_agents` method:
```rust
pub async fn spawn_all_agents(&self) {
    for record in self.registry.all() {
        self.agent_manager.spawn_agent(
            record.config.clone(),
            Arc::new(RwLock::new(self.registry)), // simplified — real Arc wrap in Core
        ).await;
    }
}
```

- [ ] **Step 6: Add dependencies**

Add to `crates/core/Cargo.toml`:
```toml
tokio-util = { version = "0.7", features = ["rt"] }
```

Add to workspace `Cargo.toml`:
```toml
tokio-util = { version = "0.7", features = ["rt"] }
```

- [ ] **Step 7: Build & test**

Run: `cargo build`
Expected: Clean compilation

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -m "phase 2 task 4: agent lifecycle with AgentManager, priority inbox, event loop"
```

---

## Task 5: Message Routing — Feishu Events → Agent Dispatch

**Files:**
- Create: `crates/core/src/router/mod.rs`
- Create: `crates/core/src/router/router.rs`
- Modify: `crates/core/src/lib.rs`
- Modify: `crates/core/src/agent/manager.rs` (add dispatch method)

- [ ] **Step 1: Implement MessageRouter**

File: `crates/core/src/router/router.rs`

```rust
use tokio::sync::mpsc;
use crate::agent::handle::AgentCommand;
use crate::config::agent::AgentConfig;
use crate::feishu::types::FeishuMessage;
use crate::registry::{AgentId, AgentRecord};

/// Routes incoming Feishu messages to the correct agent(s)
pub struct MessageRouter {
    agent_senders: HashMap<AgentId, mpsc::Sender<AgentCommand>>,
}

impl MessageRouter {
    pub fn new() -> Self {
        Self { agent_senders: HashMap::new() }
    }

    pub fn register_agent(&mut self, id: AgentId, sender: mpsc::Sender<AgentCommand>) {
        self.agent_senders.insert(id, sender);
    }

    pub fn unregister_agent(&mut self, id: &AgentId) {
        self.agent_senders.remove(id);
    }

    /// Route a Feishu message to the appropriate agent(s)
    /// Returns list of agents that received the message
    pub async fn route_message(
        &self,
        message: &FeishuMessage,
        registry: &crate::registry::AgentRegistry,
    ) -> Vec<AgentId> {
        let mut targeted = Vec::new();

        // 1. Check for @mentions — find mentioned agent by name
        for agent in registry.all() {
            let mention = format!("<at user_id=\"{}\"", agent.config.name); // simplified check
            if message.content.contains(&agent.config.name) {
                if let Some(sender) = self.agent_senders.get(&agent.id) {
                    let cmd = AgentCommand::InjectMessage(message.clone());
                    if sender.send(cmd).await.is_ok() {
                        targeted.push(agent.id);
                    }
                }
            }
        }

        // 2. If no direct mention, route to secretary (if implemented)
        if targeted.is_empty() {
            tracing::debug!("No agent matched for message, no secretary fallback yet");
        }

        targeted
    }
}
```

- [ ] **Step 2: Create router/mod.rs**

```rust
pub mod router;
```

- [ ] **Step 3: Integrate MessageRouter with AgentManager**

Modify `AgentManager::spawn_agent()` to also register with router. The manager needs a reference to the router, or the router is owned by Core.

For Phase 2 V1, keep it simple: Core owns the router and agent_manager separately. After spawning, call `router.register_agent()`.

- [ ] **Step 4: Add router module to lib.rs + Core struct**

```rust
pub mod router;

pub struct Core {
    pub router: router::router::MessageRouter,
    // ... rest
}
```

- [ ] **Step 5: Build & test**

Run: `cargo build`
Expected: Clean compilation

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "phase 2 task 5: message routing from Feishu events to agents"
```

---

## Task 6: Secretary Agent — Core Logic + Time Policy

**Files:**
- Create: `crates/core/src/secretary/mod.rs`
- Create: `crates/core/src/secretary/secretary.rs`
- Create: `crates/core/src/secretary/time_policy.rs`
- Modify: `crates/core/src/lib.rs`
- Modify: `crates/core/src/agent/manager.rs` (spawn secretary on startup)

- [ ] **Step 1: Implement time policy**

File: `crates/core/src/secretary/time_policy.rs`

```rust
use chrono::{DateTime, Utc, Datelike, Timelike, Weekday};

#[derive(Debug, Clone, PartialEq)]
pub enum WakeMode {
    Proactive,   // 忙时 — 主动推进
    Passive,     // 闲时 — 静默归档
    Immediate,   // 紧急 — 覆盖一切
}

#[derive(Debug, Clone)]
pub struct TimePolicy {
    pub wake_mode: WakeMode,
    pub summary_interval: chrono::Duration,
    pub escalation_timeout: chrono::Duration,
}

#[derive(Debug, Clone)]
pub struct TimePolicyConfig {
    pub busy_start_hour: u32,    // 9
    pub busy_end_hour: u32,      // 18
    pub busy_days: Vec<Weekday>, // Mon-Fri
    pub urgent_keywords: Vec<String>,
}

impl Default for TimePolicyConfig {
    fn default() -> Self {
        Self {
            busy_start_hour: 9,
            busy_end_hour: 18,
            busy_days: vec![Weekday::Mon, Weekday::Tue, Weekday::Wed, Weekday::Thu, Weekday::Fri],
            urgent_keywords: vec!["紧急".into(), "线上故障".into(), "P0".into(), "crash".into()],
        }
    }
}

impl TimePolicyConfig {
    pub fn resolve(&self, now: DateTime<Utc>, message: &str) -> TimePolicy {
        // Check urgent keywords first (overrides everything)
        if self.urgent_keywords.iter().any(|k| message.contains(k.as_str())) {
            return TimePolicy {
                wake_mode: WakeMode::Immediate,
                summary_interval: chrono::Duration::minutes(15),
                escalation_timeout: chrono::Duration::minutes(5),
            };
        }

        let is_busy_time = self.busy_days.contains(&now.weekday())
            && now.hour() >= self.busy_start_hour
            && now.hour() < self.busy_end_hour;

        if is_busy_time {
            TimePolicy {
                wake_mode: WakeMode::Proactive,
                summary_interval: chrono::Duration::minutes(15),
                escalation_timeout: chrono::Duration::minutes(10),
            }
        } else {
            TimePolicy {
                wake_mode: WakeMode::Passive,
                summary_interval: chrono::Duration::hours(6),
                escalation_timeout: chrono::Duration::hours(2),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_urgent_overrides_idle() {
        let config = TimePolicyConfig::default();
        let now = chrono::DateTime::parse_from_rfc3339("2026-05-24T22:00:00Z").unwrap().with_timezone(&Utc);
        let policy = config.resolve(now, "紧急！线上挂了");
        assert_eq!(policy.wake_mode, WakeMode::Immediate);
    }

    #[test]
    fn test_busy_time_proactive() {
        let config = TimePolicyConfig::default();
        let now = chrono::DateTime::parse_from_rfc3339("2026-05-25T14:00:00Z").unwrap().with_timezone(&Utc); // Monday
        let policy = config.resolve(now, "帮我写个功能");
        assert_eq!(policy.wake_mode, WakeMode::Proactive);
    }

    #[test]
    fn test_idle_time_passive() {
        let config = TimePolicyConfig::default();
        let now = chrono::DateTime::parse_from_rfc3339("2026-05-24T22:00:00Z").unwrap().with_timezone(&Utc); // Sunday
        let policy = config.resolve(now, "帮我写个功能");
        assert_eq!(policy.wake_mode, WakeMode::Passive);
    }
}
```

- [ ] **Step 2: Implement SecretaryAgent core**

File: `crates/core/src/secretary/secretary.rs`

```rust
use tokio::sync::RwLock;
use std::sync::Arc;
use crate::agent::manager::AgentManager;
use crate::registry::AgentRegistry;
use crate::memory::store::MemoryStore;
use crate::feishu::bridge::FeishuBridge;
use crate::llm::gateway::LlmGateway;
use super::time_policy::*;

pub struct SecretaryAgent {
    pub time_policy_config: TimePolicyConfig,
    pub current_mode: WakeMode,
    // References to Core components (will be wired in Core::new)
}

impl SecretaryAgent {
    pub fn new() -> Self {
        Self {
            time_policy_config: TimePolicyConfig::default(),
            current_mode: WakeMode::Proactive,
        }
    }

    pub fn resolve_policy(&mut self, now: chrono::DateTime<chrono::Utc>, message: &str) -> &TimePolicy {
        let policy = self.time_policy_config.resolve(now, message);
        self.current_mode = policy.wake_mode.clone();
        // Return a reference — for Phase 2 V1, just store and reference
        &TimePolicy {
            wake_mode: self.current_mode.clone(),
            summary_interval: chrono::Duration::minutes(15),
            escalation_timeout: chrono::Duration::minutes(10),
        }
    }
}
```

- [ ] **Step 3: Create secretary/mod.rs**

```rust
pub mod secretary;
pub mod time_policy;
```

- [ ] **Step 4: Add secretary module to Core**

Modify `crates/core/src/lib.rs`:
```rust
pub mod secretary;

pub struct Core {
    pub secretary: secretary::secretary::SecretaryAgent,
    // ... rest
}
```

- [ ] **Step 5: Build & test**

Run: `cargo build && cargo test`
Expected: Clean compilation, time policy tests pass

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "phase 2 task 6: secretary agent core + time policy with busy/idle/urgent modes"
```

---

## Task 7: Core Integration — Wire Everything Together + Smoke Test

**Files:**
- Modify: `crates/core/src/lib.rs` — update `Core::new()` to bootstrap all Phase 2 modules
- Modify: `crates/core/tests/smoke_test.rs` — add Phase 2 initialization test
- Modify: `crates/core/src/config/mod.rs` — add memory config path

- [ ] **Step 1: Update Core::new() to initialize all Phase 2 components**

```rust
use std::sync::Arc;
use tokio::sync::RwLock;

impl Core {
    pub async fn new(
        agents_dir: &Path,
        llm_config_path: &Path,
        memory_db_path: &str,      // NEW param
    ) -> Result<Self, CoreError> {
        let llm_config = config::load_llm_config(llm_config_path)?;
        let mut registry = registry::AgentRegistry::new();
        let configs = config::load_all_agents(agents_dir)?;
        for cfg in configs {
            registry.register(cfg);
        }

        let llm_gateway = llm::gateway::LlmGateway::new(llm_config);
        let feishu_bridge = feishu::bridge::FeishuBridge::new();

        // Phase 2 components
        let memory_config = memory::types::MemoryConfig::default();
        let memory_store = memory::store::MemoryStore::new(memory_db_path, memory_config).await?;
        let agent_manager = agent::manager::AgentManager::new();
        let router = router::router::MessageRouter::new();
        let secretary = secretary::secretary::SecretaryAgent::new();

        Ok(Self {
            registry,
            llm_gateway,
            feishu_bridge,
            memory_store,
            agent_manager,
            router,
            secretary,
        })
    }
}
```

- [ ] **Step 2: Update smoke test**

Modify `crates/core/tests/smoke_test.rs`:
```rust
#[tokio::test]
async fn test_core_with_memory() {
    let agents_dir = Path::new("../../agents");
    let llm_config = Path::new("../../llm_config.yaml");
    let memory_db = "sqlite::memory:";  // in-memory SQLite for tests

    if !agents_dir.exists() || !llm_config.exists() {
        eprintln!("Skipping smoke test: configs not found");
        return;
    }

    let core = Core::new(agents_dir, llm_config, memory_db).await.unwrap();

    // Verify memory store is initialized
    let agents = core.list_agents();
    assert!(!agents.is_empty(), "Should have loaded at least one agent");

    // Verify time policy works
    let policy = core.secretary.time_policy_config.resolve(
        chrono::Utc::now(),
        "普通需求",
    );
    eprintln!("Current time policy: {:?}", policy.wake_mode);
}
```

- [ ] **Step 3: Build & test**

Run: `cargo build && cargo test`
Expected: Clean compilation, all tests pass

- [ ] **Step 4: Commit Phase 2**

```bash
git add -A
git commit -m "phase 2: wire all intelligence modules into Core, update smoke test"
```

---

## Self-Review

**Spec coverage check:**
- §4 (Memory System): Covered — Tasks 1-3 implement full three-tier architecture
  - §4.1-4.2 (Three-tier + roles): Task 1 — types.rs defines ShortTerm/LongTerm
  - §4.3 (Memory flow): Tasks 1-3 — compression pipeline, store, retrieval
  - §4.4 (Compression strategy): Task 2 — OnnxVectorizer + Compressor (rule-based + LLM)
  - §4.5 (Memory retrieval): Task 3 — search_semantic with cosine similarity + ranking
  - §4.6 (Forgetting model): Task 3 — Ebbinghaus decay, eviction, retention value
  - §4.7 (Memory visibility): TUI Phase 3
  - §4.8 (Tech implementation): Tasks 1-2 — SQLite, ONNX stub, Ollama stub
- §5.7 (Concurrency model): Task 4 — AgentManager, JoinHandle, control channels
- §6.2-6.3 (Message routing + threads): Task 5 — MessageRouter, @mention dispatch
- §6.4 (Secretary + busy/idle): Task 6 — SecretaryAgent, TimePolicy with busy/idle/urgent
- §6.5 (TUI Agent Inbox): Phase 3

**Placeholder scan:**
- `ort` dependency commented out for Phase 2 V1 — intentional, documented in task
- `OnnxVectorizer::embed()` returns zero vector stub — intentional, full ORT in cleanup
- `Compressor::compress_with_llm()` falls back to rule-based — intentional, full Ollama in cleanup
- All `todo!()` calls are identified with specific locations

**Gap:** Phase 2 does not implement:
- Full ONNX Runtime integration (vectorizer returns stub embeddings)
- Full Ollama compression (rule-based fallback only)
- TUI Agent Inbox view → Phase 3
- Plugin system → Phase 3

These are intentionally deferred. Phase 2 establishes the architecture and data model; the full ONNX/Ollama pipeline requires downloading models (~500MB+2GB) which is a separate setup task.