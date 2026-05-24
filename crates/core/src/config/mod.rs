pub mod agent;
pub mod llm;

use std::path::Path;
use crate::CoreError;

// Shared re-exports
pub use agent::{AgentConfig, LlmAgentConfig, ModelConfig, RateLimitConfig, TriggerConfig};

/// Load a single agent config from YAML file
pub fn load_agent_config(path: &Path) -> Result<agent::AgentConfig, CoreError> {
    let content = std::fs::read_to_string(path)?;
    let config: agent::AgentConfig = serde_yaml::from_str(&content)?;
    Ok(config)
}

/// Load LLM config from YAML file
pub fn load_llm_config(path: &Path) -> Result<llm::LlmConfig, CoreError> {
    let content = std::fs::read_to_string(path)?;
    let config: llm::LlmConfig = serde_yaml::from_str(&content)?;
    Ok(config)
}

/// Load all agent configs from a directory (sorted by filename for determinism)
pub fn load_all_agents(dir: &Path) -> Result<Vec<agent::AgentConfig>, CoreError> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)?
        .collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|e| e.file_name());

    let mut configs = Vec::new();
    for entry in entries {
        let path = entry.path();
        if path.extension().map_or(false, |e| e == "yaml") {
            configs.push(load_agent_config(&path)?);
        }
    }
    Ok(configs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_agent_config() {
        let yaml = r#"
name: "小红"
role: "产品经理"
llm:
  primary:
    provider: anthropic
    model: claude-sonnet-4-20250514
    api_key_env: ANTHROPIC_API_KEY
    max_tokens: 8192
triggers:
  - pattern: "需求"
"#;
        let config: agent::AgentConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.name, "小红");
        assert_eq!(config.llm.primary.provider, "anthropic");
    }

    #[test]
    fn test_load_all_agents_from_dir() {
        use std::io::Write;
        let dir = std::env::temp_dir().join("feishu_agents_test");
        std::fs::create_dir_all(&dir).unwrap();

        let yaml = br#"name: "test-agent"
role: "test"
llm:
  primary:
    provider: anthropic
    model: claude-sonnet-4-20250514
    api_key_env: ANTHROPIC_API_KEY
    max_tokens: 8192
triggers: []"#;

        let path = dir.join("test.yaml");
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(yaml).unwrap();

        let configs = load_all_agents(&dir).unwrap();
        assert_eq!(configs.len(), 1);
        assert_eq!(configs[0].name, "test-agent");

        std::fs::remove_dir_all(dir).unwrap();
    }
}
