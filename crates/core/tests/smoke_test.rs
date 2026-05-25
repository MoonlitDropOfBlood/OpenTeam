use feishu_agent_core::Core;
use feishu_agent_core::memory::types::{MemoryEntry, MemoryType};
use std::path::Path;

#[tokio::test]
async fn test_core_initialization() {
    let agents_dir = Path::new("../../agents");
    let llm_config = Path::new("../../llm_config.yaml");

    // If config files don't exist (e.g., CI), skip
    if !agents_dir.exists() || !llm_config.exists() {
        eprintln!("Skipping smoke test: configs not found");
        return;
    }

    let core = Core::new(agents_dir, llm_config, ":memory:").await.unwrap();
    let agents = core.list_agents();
    assert!(!agents.is_empty(), "Should have loaded at least one agent");

    let feishu_ok = core.check_feishu_auth().await;
    eprintln!("Feishu auth status: {}", feishu_ok);
}

#[tokio::test]
async fn test_agent_config_loading() {
    use feishu_agent_core::config;

    let agents = config::load_all_agents(Path::new("../../agents")).unwrap();
    assert!(!agents.is_empty());
    for agent in &agents {
        assert!(!agent.llm.primary.provider.is_empty(), "Agent must have an LLM configured");
    }
}

#[tokio::test]
async fn test_memory_insert_and_retrieve() {
    let agents_dir = Path::new("../../agents");
    let llm_config = Path::new("../../llm_config.yaml");
    if !agents_dir.exists() || !llm_config.exists() {
        eprintln!("Skipping memory test: configs not found");
        return;
    }

    // Use temp file DB (not ":memory:") — SQLite connection pool doesn't
    // share schema across connections with in-memory databases.
    let mut tmp = std::env::temp_dir();
    let name = format!("feishu_e2e_{}.db", uuid::Uuid::now_v7().to_string().replace('-', ""));
    tmp.push(&name);
    let _ = std::fs::File::create(&tmp);
    let db_path = tmp.to_string_lossy().replace('\\', "/");

    let core = Core::new(agents_dir, llm_config, &db_path).await.unwrap();

    // Insert a memory
    let entry = MemoryEntry {
        id: uuid::Uuid::now_v7(),
        agent_id: "test-agent".into(),
        memory_type: MemoryType::ShortTerm,
        title: "E2E Test Decision".into(),
        summary: "Test integration".into(),
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
    let id = entry.id;
    core.memory_store.insert(&entry).await.unwrap();

    // Retrieve and verify
    let loaded = core.memory_store.get(&id).await.unwrap().expect("Entry should exist");
    assert_eq!(loaded.title, "E2E Test Decision");
    assert_eq!(loaded.agent_id, "test-agent");
}

#[tokio::test]
async fn test_assistant_time_policy() {
    let agents_dir = Path::new("../../agents");
    let llm_config = Path::new("../../llm_config.yaml");
    if !agents_dir.exists() || !llm_config.exists() {
        eprintln!("Skipping assistant test: configs not found");
        return;
    }

    let mut tmp = std::env::temp_dir();
    let name = format!("feishu_e2e_{}.db", uuid::Uuid::now_v7().to_string().replace('-', ""));
    tmp.push(&name);
    let _ = std::fs::File::create(&tmp);
    let db_path = tmp.to_string_lossy().replace('\\', "/");

    let core = Core::new(agents_dir, llm_config, &db_path).await.unwrap();
    let urgent = core.assistant.lock().await.is_urgent("紧急！线上挂了");
    assert!(urgent, "Should detect urgent keywords");
    let normal = core.assistant.lock().await.is_urgent("写个PRD需求");
    assert!(!normal, "Should not flag normal messages as urgent");
}

#[tokio::test]
async fn test_core_shutdown() {
    let agents_dir = Path::new("../../agents");
    let llm_config = Path::new("../../llm_config.yaml");
    if !agents_dir.exists() || !llm_config.exists() {
        eprintln!("Skipping shutdown test: configs not found");
        return;
    }

    let mut tmp = std::env::temp_dir();
    let name = format!("feishu_e2e_{}.db", uuid::Uuid::now_v7().to_string().replace('-', ""));
    tmp.push(&name);
    let _ = std::fs::File::create(&tmp);
    let db_path = tmp.to_string_lossy().replace('\\', "/");

    let core = Core::new(agents_dir, llm_config, &db_path).await.unwrap();
    // Shutdown should not panic
    core.shutdown().await;
    // After shutdown, plugin system should report not running
    assert!(!core.plugin_manager.is_running().await);
}
