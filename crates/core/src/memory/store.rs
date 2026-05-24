use sqlx::SqlitePool;
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

impl MemoryStore {
    pub async fn new(db_path: &str, config: MemoryConfig) -> Result<Self, CoreError> {
        let pool = SqlitePool::connect(db_path).await
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

    pub fn config(&self) -> &MemoryConfig {
        &self.config
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
