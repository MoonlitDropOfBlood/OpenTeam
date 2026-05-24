use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    tracing::info!("Feishu Agent Orchestrator — Phase 1 starting");
    tracing::info!("Core library loaded successfully");

    Ok(())
}
