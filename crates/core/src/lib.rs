pub mod config;
pub mod registry;
pub mod llm;
pub mod feishu;
pub mod error;

use std::path::Path;

pub use error::CoreError;

pub fn bootstrap_registry(agents_dir: &Path) -> Result<registry::AgentRegistry, CoreError> {
    let mut registry = registry::AgentRegistry::new();
    let configs = config::load_all_agents(agents_dir)?;
    for config in configs {
        registry.register(config);
    }
    Ok(registry)
}
