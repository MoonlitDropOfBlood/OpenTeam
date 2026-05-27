pub const SCHEMA_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS memory_entries (
    id              TEXT PRIMARY KEY,
    agent_id        TEXT NOT NULL,
    memory_type     TEXT NOT NULL CHECK(memory_type IN ('ShortTerm', 'LongTerm')),
    title           TEXT NOT NULL,
    summary         TEXT NOT NULL,
    decisions       TEXT NOT NULL DEFAULT '[]',
    artifacts       TEXT NOT NULL DEFAULT '[]',
    pending_todos   TEXT NOT NULL DEFAULT '[]',
    importance      INTEGER NOT NULL DEFAULT 5,
    embedding       BLOB,
    turn_indices    TEXT NOT NULL DEFAULT '[]',
    created_at      TEXT NOT NULL,
    last_accessed   TEXT NOT NULL,
    access_count    INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_memory_agent ON memory_entries(agent_id);
CREATE INDEX IF NOT EXISTS idx_memory_type ON memory_entries(memory_type);
CREATE INDEX IF NOT EXISTS idx_memory_importance ON memory_entries(importance);
"#;
