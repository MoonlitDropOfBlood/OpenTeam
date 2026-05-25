use sqlx::SqlitePool;
use uuid::Uuid;
use crate::CoreError;
use super::types::*;

fn now_utc() -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::<chrono::Utc>::from(std::time::SystemTime::now())
}

pub struct MemoryStore {
    pool: SqlitePool,
    config: MemoryConfig,
}

// Helper to serialize Vec<f32> to BLOB (little-endian f32 bytes)
fn embedding_to_blob(embedding: &[f32]) -> Vec<u8> {
    embedding.iter().flat_map(|f| f.to_le_bytes()).collect()
}

// Helper to deserialize BLOB to Vec<f32>
fn blob_to_embedding(blob: &[u8]) -> Vec<f32> {
    blob.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect()
}

/// Shared row struct for sqlx queries
#[derive(sqlx::FromRow)]
struct MemoryRow {
    id: String,
    agent_id: String,
    memory_type: String,
    title: String,
    summary: String,
    decisions: String,
    artifacts: String,
    pending_todos: String,
    importance: i32,
    embedding: Option<Vec<u8>>,
    turn_indices: String,
    created_at: String,
    last_accessed: String,
    access_count: i32,
}

impl MemoryStore {
    pub async fn new(db_path: &str, config: MemoryConfig) -> Result<Self, CoreError> {
        let opts = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(db_path)
            .create_if_missing(true);
        let pool = SqlitePool::connect_with(opts).await
            .map_err(|e| CoreError::Memory(format!("DB connect: {e}")))?;
        sqlx::query(super::migration::SCHEMA_SQL)
            .execute(&pool)
            .await
            .map_err(|e| CoreError::Memory(format!("Migration: {e}")))?;
        Ok(Self { pool, config })
    }

    pub async fn insert(&self, entry: &MemoryEntry) -> Result<(), CoreError> {
        let decisions_json = serde_json::to_string(&entry.decisions)
            .map_err(|e| CoreError::Memory(format!("Serialize decisions: {e}")))?;
        let artifacts_json = serde_json::to_string(&entry.artifacts)
            .map_err(|e| CoreError::Memory(format!("Serialize artifacts: {e}")))?;
        let todos_json = serde_json::to_string(&entry.pending_todos)
            .map_err(|e| CoreError::Memory(format!("Serialize todos: {e}")))?;
        let indices_json = serde_json::to_string(&entry.turn_indices)
            .map_err(|e| CoreError::Memory(format!("Serialize indices: {e}")))?;
        let embedding_blob = entry.embedding.as_ref().map(|v| embedding_to_blob(v));

        sqlx::query(
            "INSERT INTO memory_entries (id, agent_id, memory_type, title, summary, decisions, artifacts, pending_todos, importance, embedding, turn_indices, created_at, last_accessed, access_count) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(entry.id.to_string())
        .bind(&entry.agent_id)
        .bind(match entry.memory_type {
            MemoryType::ShortTerm => "ShortTerm",
            MemoryType::LongTerm => "LongTerm",
        })
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

        match row {
            Some(r) => {
                let memory_type = match r.memory_type.as_str() {
                    "ShortTerm" => MemoryType::ShortTerm,
                    "LongTerm" => MemoryType::LongTerm,
                    other => return Err(CoreError::Memory(format!("Unknown memory_type: {other}"))),
                };
                let entry = MemoryEntry {
                    id: uuid::Uuid::parse_str(&r.id)
                        .map_err(|e| CoreError::Memory(format!("Parse UUID: {e}")))?,
                    agent_id: r.agent_id,
                    memory_type,
                    title: r.title,
                    summary: r.summary,
                    decisions: serde_json::from_str(&r.decisions)
                        .map_err(|e| CoreError::Memory(format!("Parse decisions: {e}")))?,
                    artifacts: serde_json::from_str(&r.artifacts)
                        .map_err(|e| CoreError::Memory(format!("Parse artifacts: {e}")))?,
                    pending_todos: serde_json::from_str(&r.pending_todos)
                        .map_err(|e| CoreError::Memory(format!("Parse todos: {e}")))?,
                    importance: r.importance as u8,
                    embedding: r.embedding.map(|b| blob_to_embedding(&b)),
                    turn_indices: serde_json::from_str(&r.turn_indices)
                        .map_err(|e| CoreError::Memory(format!("Parse indices: {e}")))?,
                    created_at: chrono::DateTime::parse_from_rfc3339(&r.created_at)
                        .map_err(|e| CoreError::Memory(format!("Parse created_at: {e}")))?
                        .with_timezone(&chrono::Utc),
                    last_accessed: chrono::DateTime::parse_from_rfc3339(&r.last_accessed)
                        .map_err(|e| CoreError::Memory(format!("Parse last_accessed: {e}")))?
                        .with_timezone(&chrono::Utc),
                    access_count: r.access_count as u32,
                };
                Ok(Some(entry))
            }
            None => Ok(None),
        }
    }

    pub async fn list_by_agent(&self, agent_id: &str) -> Result<Vec<MemoryEntry>, CoreError> {
        let rows = sqlx::query_as::<_, MemoryRow>(
            "SELECT * FROM memory_entries WHERE agent_id = ? ORDER BY created_at DESC"
        )
        .bind(agent_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| CoreError::Memory(format!("List: {e}")))?;

        rows.into_iter().map(|r| {
            let memory_type = match r.memory_type.as_str() {
                "ShortTerm" => MemoryType::ShortTerm,
                "LongTerm" => MemoryType::LongTerm,
                other => return Err(CoreError::Memory(format!("Unknown memory_type: {other}"))),
            };
            Ok(MemoryEntry {
                id: uuid::Uuid::parse_str(&r.id)
                    .map_err(|e| CoreError::Memory(format!("Parse UUID: {e}")))?,
                agent_id: r.agent_id,
                memory_type,
                title: r.title,
                summary: r.summary,
                decisions: serde_json::from_str(&r.decisions)
                    .map_err(|e| CoreError::Memory(format!("Parse decisions: {e}")))?,
                artifacts: serde_json::from_str(&r.artifacts)
                    .map_err(|e| CoreError::Memory(format!("Parse artifacts: {e}")))?,
                pending_todos: serde_json::from_str(&r.pending_todos)
                    .map_err(|e| CoreError::Memory(format!("Parse todos: {e}")))?,
                importance: r.importance as u8,
                embedding: r.embedding.map(|b| blob_to_embedding(&b)),
                turn_indices: serde_json::from_str(&r.turn_indices)
                    .map_err(|e| CoreError::Memory(format!("Parse indices: {e}")))?,
                created_at: chrono::DateTime::parse_from_rfc3339(&r.created_at)
                    .map_err(|e| CoreError::Memory(format!("Parse created_at: {e}")))?
                    .with_timezone(&chrono::Utc),
                last_accessed: chrono::DateTime::parse_from_rfc3339(&r.last_accessed)
                    .map_err(|e| CoreError::Memory(format!("Parse last_accessed: {e}")))?
                    .with_timezone(&chrono::Utc),
                access_count: r.access_count as u32,
            })
        }).collect()
    }

    pub async fn search_semantic(&self, agent_id: &str, query_embedding: &[f32], top_k: usize) -> Result<Vec<MemorySearchResult>, CoreError> {
        let entries = self.list_by_agent(agent_id).await?;
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
            .bind(now_utc().to_rfc3339())
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| CoreError::Memory(format!("Touch: {e}")))?;
        Ok(())
    }

    /// Get all memory entries across all agents
    pub async fn list_all(&self) -> Result<Vec<MemoryEntry>, CoreError> {
        let rows = sqlx::query_as::<_, MemoryRow>(
            "SELECT * FROM memory_entries ORDER BY created_at DESC"
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| CoreError::Memory(format!("List all: {e}")))?;

        rows.into_iter().map(|r| {
            let memory_type = match r.memory_type.as_str() {
                "ShortTerm" => MemoryType::ShortTerm,
                "LongTerm" => MemoryType::LongTerm,
                other => return Err(CoreError::Memory(format!("Unknown memory_type: {other}"))),
            };
            Ok(MemoryEntry {
                id: uuid::Uuid::parse_str(&r.id)
                    .map_err(|e| CoreError::Memory(format!("Parse UUID: {e}")))?,
                agent_id: r.agent_id,
                memory_type,
                title: r.title,
                summary: r.summary,
                decisions: serde_json::from_str(&r.decisions)
                    .map_err(|e| CoreError::Memory(format!("Parse decisions: {e}")))?,
                artifacts: serde_json::from_str(&r.artifacts)
                    .map_err(|e| CoreError::Memory(format!("Parse artifacts: {e}")))?,
                pending_todos: serde_json::from_str(&r.pending_todos)
                    .map_err(|e| CoreError::Memory(format!("Parse todos: {e}")))?,
                importance: r.importance as u8,
                embedding: r.embedding.map(|b| blob_to_embedding(&b)),
                turn_indices: serde_json::from_str(&r.turn_indices)
                    .map_err(|e| CoreError::Memory(format!("Parse indices: {e}")))?,
                created_at: chrono::DateTime::parse_from_rfc3339(&r.created_at)
                    .map_err(|e| CoreError::Memory(format!("Parse created_at: {e}")))?
                    .with_timezone(&chrono::Utc),
                last_accessed: chrono::DateTime::parse_from_rfc3339(&r.last_accessed)
                    .map_err(|e| CoreError::Memory(format!("Parse last_accessed: {e}")))?
                    .with_timezone(&chrono::Utc),
                access_count: r.access_count as u32,
            })
        }).collect()
    }

    pub fn config(&self) -> &MemoryConfig {
        &self.config
    }

    /// Apply eviction: remove lowest-value entries when limits are exceeded
    pub async fn apply_eviction(&self) -> Result<Vec<Uuid>, CoreError> {
        let mut removed = Vec::new();

        // Get all entries
        let all = self.list_all().await?;
        // Separate by type
        let short_term: Vec<&MemoryEntry> = all.iter().filter(|e| e.memory_type == MemoryType::ShortTerm).collect();
        let long_term: Vec<&MemoryEntry> = all.iter().filter(|e| e.memory_type == MemoryType::LongTerm).collect();

        // Short-term: evict expired entries
        for entry in &short_term {
            let age_days = (now_utc() - entry.created_at).num_hours() as f64 / 24.0;
            if age_days > self.config.short_term_max_age_days as f64 {
                self.delete(&entry.id).await?;
                removed.push(entry.id);
            }
        }

        // Short-term: if still over max_count, evict lowest value
        let remaining_short = self.list_all().await?.into_iter()
            .filter(|e| e.memory_type == MemoryType::ShortTerm)
            .collect::<Vec<_>>();
        if remaining_short.len() > self.config.short_term_max_count as usize {
            let excess = remaining_short.len() - self.config.short_term_max_count as usize;
            let candidates = super::forgetting::select_eviction_candidates(&remaining_short, &self.config, excess);
            for id in &candidates {
                self.delete(id).await?;
                removed.push(*id);
            }
        }

        // Long-term: if over max_count, evict lowest value
        if long_term.len() > self.config.long_term_max_count as usize {
            let long_entries = all.iter().filter(|e| e.memory_type == MemoryType::LongTerm).cloned().collect::<Vec<_>>();
            let excess = long_entries.len() - self.config.long_term_max_count as usize;
            let candidates = super::forgetting::select_eviction_candidates(&long_entries, &self.config, excess);
            for id in &candidates {
                self.delete(id).await?;
                removed.push(*id);
            }
        }

        Ok(removed)
    }
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 { return 0.0; }
    (dot / (norm_a * norm_b)) as f64
}

fn compute_decay(entry: &MemoryEntry, config: &MemoryConfig) -> f64 {
    let age_hours = (now_utc() - entry.created_at).num_hours() as f64;
    let age_days = age_hours / 24.0;
    if age_days <= 0.0 { return 1.0; }
    let lambda = match entry.importance {
        8..=10 => config.base_decay_rate * 0.1,
        5..=7  => config.base_decay_rate,
        _      => config.base_decay_rate * 5.0,
    };
    (-lambda * age_days).exp()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_store() -> MemoryStore {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let config = MemoryConfig::default();
            // Create temp file first to ensure parent dir exists
            let mut tmp = std::env::temp_dir();
            let name = format!("feishu_test_{}.db", uuid::Uuid::now_v7().to_string().replace('-', ""));
            tmp.push(&name);
            // Pre-create the file so SQLite can see it
            let _ = std::fs::File::create(&tmp);
            let path_str = tmp.to_string_lossy().replace('\\', "/");
            MemoryStore::new(&path_str, config).await.unwrap()
        })
    }

    fn now() -> chrono::DateTime<chrono::Utc> {
        chrono::DateTime::<chrono::Utc>::from(std::time::SystemTime::now())
    }

    fn sample_entry(agent: &str) -> MemoryEntry {
        MemoryEntry {
            id: uuid::Uuid::now_v7(),
            agent_id: agent.to_string(),
            memory_type: MemoryType::ShortTerm,
            title: "Test decision".into(),
            summary: "Test summary".into(),
            decisions: vec![Decision {
                title: "Use JWT".into(),
                decision: "Use JWT for auth".into(),
                reason: "Better scalability".into(),
                made_by: agent.to_string(),
            }],
            artifacts: vec![],
            pending_todos: vec![],
            importance: 5,
            embedding: None,
            turn_indices: vec![0, 1, 2],
            created_at: now(),
            last_accessed: now(),
            access_count: 0,
        }
    }

    #[test]
    fn test_insert_and_get() {
        let store = test_store();
        let entry = sample_entry("小红");
        let id = entry.id;
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            store.insert(&entry).await.unwrap();
            let loaded = store.get(&id).await.unwrap().expect("Entry should exist");
            assert_eq!(loaded.title, "Test decision");
            assert_eq!(loaded.decisions.len(), 1);
            assert_eq!(loaded.decisions[0].decision, "Use JWT for auth");
            assert_eq!(loaded.turn_indices, vec![0, 1, 2]);
        });
    }

    #[test]
    fn test_list_by_agent() {
        let store = test_store();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            store.insert(&sample_entry("小红")).await.unwrap();
            store.insert(&sample_entry("CodeCat")).await.unwrap();
            let xiao_hong = store.list_by_agent("小红").await.unwrap();
            assert_eq!(xiao_hong.len(), 1);
            assert_eq!(xiao_hong[0].agent_id, "小红");
            let all = store.list_by_agent("CodeCat").await.unwrap();
            assert_eq!(all.len(), 1);
        });
    }

    #[test]
    fn test_delete() {
        let store = test_store();
        let entry = sample_entry("test");
        let id = entry.id;
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            store.insert(&entry).await.unwrap();
            assert!(store.get(&id).await.unwrap().is_some());
            store.delete(&id).await.unwrap();
            assert!(store.get(&id).await.unwrap().is_none());
        });
    }

    #[test]
    fn test_touch_increments_access_count() {
        let store = test_store();
        let entry = sample_entry("test");
        let id = entry.id;
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            store.insert(&entry).await.unwrap();
            assert_eq!(store.get(&id).await.unwrap().unwrap().access_count, 0);
            store.touch(&id).await.unwrap();
            assert_eq!(store.get(&id).await.unwrap().unwrap().access_count, 1);
        });
    }

    #[test]
    fn test_cosine_similarity_identical() {
        let a = vec![1.0, 2.0, 3.0];
        let sim = cosine_similarity(&a, &a);
        assert!((sim - 1.0).abs() < 0.001, "Identical vectors should have similarity ~1.0, got {sim}");
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim - 0.0).abs() < 0.001, "Orthogonal vectors should have similarity 0.0, got {sim}");
    }

    #[test]
    fn test_cosine_similarity_zero_vector() {
        let a = vec![0.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim - 0.0).abs() < 0.001, "Zero vector should return 0.0, got {sim}");
    }

    #[test]
    fn test_list_all() {
        let store = test_store();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            store.insert(&sample_entry("A")).await.unwrap();
            store.insert(&sample_entry("B")).await.unwrap();
            let all = store.list_all().await.unwrap();
            assert_eq!(all.len(), 2);
        });
    }
}
