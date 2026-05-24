use feishu_agent_core::Core;
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
