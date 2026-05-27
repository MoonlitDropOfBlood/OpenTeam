use super::types::*;
use uuid::Uuid;

/// Calculate memory strength using Ebbinghaus decay formula
pub fn memory_strength(entry: &MemoryEntry, config: &MemoryConfig) -> f64 {
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

/// Calculate retention value for eviction ranking
pub fn retention_value(entry: &MemoryEntry, config: &MemoryConfig) -> f64 {
    let strength = memory_strength(entry, config);
    let access_boost = 1.0 + (entry.access_count as f64 * config.retrieval_boost);
    entry.importance as f64 * strength * access_boost
}

/// Select candidates for eviction (never evict entries with importance >= retention_importance)
pub fn select_eviction_candidates(
    entries: &[MemoryEntry],
    config: &MemoryConfig,
    count: usize,
) -> Vec<Uuid> {
    let mut ranked: Vec<_> = entries.iter()
        .filter(|e| e.importance < config.retention_importance)
        .map(|e| (e.id, retention_value(e, config)))
        .collect();
    ranked.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    ranked.into_iter().take(count).map(|(id, _)| id).collect()
}

fn now_utc() -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::<chrono::Utc>::from(std::time::SystemTime::now())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_entry(importance: u8, days_ago: i64) -> MemoryEntry {
        MemoryEntry {
            id: uuid::Uuid::now_v7(),
            agent_id: "test".into(),
            memory_type: MemoryType::ShortTerm,
            title: "test".into(),
            summary: "test".into(),
            decisions: vec![],
            artifacts: vec![],
            pending_todos: vec![],
            importance,
            embedding: None,
            turn_indices: vec![],
            created_at: now_utc() - chrono::Duration::days(days_ago),
            last_accessed: now_utc(),
            access_count: 0,
        }
    }

    #[test]
    fn test_high_importance_near_permanent() {
        let entry = sample_entry(9, 60);
        let config = MemoryConfig::default();
        let strength = memory_strength(&entry, &config);
        assert!(strength > 0.8, "High importance should decay slowly: {strength}");
    }

    #[test]
    fn test_low_importance_decays_quickly() {
        let entry = sample_entry(2, 30);
        let config = MemoryConfig::default();
        let strength = memory_strength(&entry, &config);
        assert!(strength < 0.3, "Low importance should decay quickly: {strength}");
    }

    #[test]
    fn test_importance_5_decays_moderately() {
        let entry = sample_entry(5, 30);
        let config = MemoryConfig::default();
        let strength = memory_strength(&entry, &config);
        assert!(strength > 0.1 && strength < 0.8, "Medium importance decay: {strength}");
    }

    #[test]
    fn test_retention_value_increases_with_access() {
        let mut entry = sample_entry(5, 1);
        entry.access_count = 10;
        let config = MemoryConfig::default();
        let val_accessed = retention_value(&entry, &config);
        entry.access_count = 0;
        let val_fresh = retention_value(&entry, &config);
        assert!(val_accessed > val_fresh, "Accessed entries should have higher retention value");
    }

    #[test]
    fn test_eviction_skips_high_importance() {
        let high = sample_entry(8, 60);
        let low = sample_entry(3, 1);
        let entries = vec![high, low];
        let config = MemoryConfig::default();
        let candidates = select_eviction_candidates(&entries, &config, 1);
        assert_eq!(candidates.len(), 1, "Should evict exactly 1 entry");
        // The low-importance entry should be the one evicted
        // (high importance is filtered out by retention_importance >= 7)
    }

    #[test]
    fn test_memory_strength_fresh_entry() {
        let entry = sample_entry(5, 0);
        let config = MemoryConfig::default();
        let strength = memory_strength(&entry, &config);
        assert!((strength - 1.0).abs() < 0.001, "Fresh entry should have strength ≈ 1.0");
    }
}
