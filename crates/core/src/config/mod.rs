pub mod agent;
pub mod llm;
pub mod provider;

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

/// Load an agent config from a single YAML file path (public alias)
pub fn load_agent_config_from_path(path: &Path) -> Result<agent::AgentConfig, CoreError> {
    load_agent_config(path)
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
    model: anthropic/claude-sonnet-4-20250514
    api_key_env: ANTHROPIC_API_KEY
    max_tokens: 8192
triggers:
  - pattern: "需求"
"#;
        let config: agent::AgentConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.name, "小红");
        assert_eq!(config.llm.primary.provider(), "anthropic");
        assert_eq!(config.llm.primary.model_name(), "claude-sonnet-4-20250514");
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
    model: anthropic/claude-sonnet-4-20250514
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

    #[test]
    fn test_model_resolution_from_yaml() {
        use crate::llm::provider::ProviderResolver;

        let resolver = ProviderResolver::new(std::collections::HashMap::new());
        let yaml = r#"
name: "test-agent"
role: "test"
llm:
  primary:
    model: deepseek/deepseek-v4-flash
    max_tokens: 8192
triggers: []
"#;
        let config: crate::config::agent::AgentConfig = serde_yaml::from_str(yaml).unwrap();
        let resolved = resolver.resolve(&config.llm.primary);

        assert_eq!(resolved.provider, "deepseek");
        assert_eq!(resolved.base_url, "https://api.deepseek.com/v1");
        assert!(resolved.can_reason);
        assert_eq!(resolved.rate_limiter_key, "deepseek/deepseek-v4-flash");
    }

    #[test]
    fn test_model_resolution_with_yaml_provider_override() {
        use crate::config::provider::{ProviderConfig, ProviderOptions};
        use crate::llm::provider::ProviderResolver;

        let mut provider_configs = std::collections::HashMap::new();
        provider_configs.insert("deepseek".to_string(), ProviderConfig {
            name: Some("Custom DeepSeek".into()),
            npm: None,
            env: vec![],
            whitelist: None,
            blacklist: None,
            options: ProviderOptions {
                base_url: Some("https://custom-deepseek.com/v1".into()),
                timeout: Some(120000),
                ..Default::default()
            },
            models: std::collections::HashMap::new(),
        });

        let resolver = ProviderResolver::new(provider_configs);
        let yaml = r#"
name: "test"
role: "test"
llm:
  primary:
    model: deepseek/deepseek-v4-pro
    max_tokens: 4096
triggers: []
"#;
        let config: crate::config::agent::AgentConfig = serde_yaml::from_str(yaml).unwrap();
        let resolved = resolver.resolve(&config.llm.primary);

        assert_eq!(resolved.base_url, "https://custom-deepseek.com/v1");
        assert_eq!(resolved.timeout_secs, 120); // 120000ms → 120s
        assert_eq!(resolved.max_tokens, 4096);
    }

    #[test]
    fn test_model_resolution_ollama_yaml() {
        use crate::llm::provider::ProviderResolver;

        let resolver = ProviderResolver::new(std::collections::HashMap::new());
        let yaml = r#"
name: "test"
role: "test"
llm:
  primary:
    model: ollama/qwen2.5:3b
    max_tokens: 4096
triggers: []
"#;
        let config: crate::config::agent::AgentConfig = serde_yaml::from_str(yaml).unwrap();
        let resolved = resolver.resolve(&config.llm.primary);

        assert_eq!(resolved.provider, "ollama");
        assert_eq!(resolved.base_url, "http://localhost:11434/api");
        assert_eq!(resolved.timeout_secs, 60); // 60000ms → 60s
        assert!(!resolved.can_reason);
    }

    #[test]
    fn test_load_llm_config_with_provider_section() {
        let config_path = std::path::Path::new("llm_config.yaml");
        if !config_path.exists() {
            eprintln!("llm_config.yaml not found at {:?}, skipping test", config_path);
            return;
        }
        let config = crate::config::load_llm_config(config_path).unwrap();

        assert!(config.provider.contains_key("anthropic"), "llm_config.yaml should have anthropic provider");
        assert!(config.provider.contains_key("deepseek"), "llm_config.yaml should have deepseek provider");
        assert!(config.provider.contains_key("openai"), "llm_config.yaml should have openai provider");
        assert!(config.provider.contains_key("ollama"), "llm_config.yaml should have ollama provider");

        assert!(config.models.contains_key("claude-sonnet-4"), "llm_config.yaml should have legacy models");
    }
}
