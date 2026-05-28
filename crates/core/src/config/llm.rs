use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use crate::config::agent::ModelConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    /// Provider configurations (keyed by provider ID)
    #[serde(default)]
    pub provider: HashMap<String, super::provider::ProviderConfig>,
    /// Legacy model pool (kept for backward compatibility)
    #[serde(default)]
    pub models: HashMap<String, ModelConfig>,
}

/// Model cost config in YAML (per 1M tokens)
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ModelCostConfig {
    pub input: Option<f64>,
    pub output: Option<f64>,
    #[serde(default)]
    pub cache_read: Option<f64>,
    #[serde(default)]
    pub cache_write: Option<f64>,
}

/// Model limit config in YAML
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelLimitConfig {
    pub context: Option<u32>,
    pub input: Option<u32>,
    pub output: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_llm_config_with_provider() {
        let yaml = r#"
provider:
  test-provider:
    name: "Test"
    options:
      baseURL: https://test.com/v1
    models:
      test-model:
        name: "Test Model"
models:
  legacy-model:
    model: anthropic/claude-sonnet-4-20250514
    max_tokens: 8192
"#;
        let config: LlmConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.provider.contains_key("test-provider"));
        assert!(config.models.contains_key("legacy-model"));
    }

    #[test]
    fn test_llm_config_provider_options_parsed() {
        let yaml = r#"
provider:
  custom:
    name: "Custom API"
    options:
      base_url: https://custom.ai/v1
      timeout: 120000
      headers:
        X-Auth: my-token
"#;
        let config: LlmConfig = serde_yaml::from_str(yaml).unwrap();
        let custom = config.provider.get("custom").unwrap();
        assert_eq!(custom.name.as_deref(), Some("Custom API"));
        assert_eq!(custom.options.base_url.as_deref(), Some("https://custom.ai/v1"));
        assert_eq!(custom.options.timeout, Some(120000));
        assert_eq!(custom.options.headers.get("X-Auth").unwrap(), "my-token");
    }

    #[test]
    fn test_llm_config_empty() {
        let yaml = r#"{}"#;
        let config: LlmConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.provider.is_empty());
        assert!(config.models.is_empty());
    }

    #[test]
    fn test_model_cost_config() {
        let yaml = r#"
input: 3.0
output: 15.0
cache_read: 0.9
cache_write: 3.0
"#;
        let cost: ModelCostConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(cost.input, Some(3.0));
        assert_eq!(cost.output, Some(15.0));
        assert_eq!(cost.cache_read, Some(0.9));
        assert_eq!(cost.cache_write, Some(3.0));
    }

    #[test]
    fn test_model_limit_config() {
        let yaml = r#"
context: 128000
input: 64000
output: 16384
"#;
        let limit: ModelLimitConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(limit.context, Some(128000));
        assert_eq!(limit.input, Some(64000));
        assert_eq!(limit.output, Some(16384));
    }
}
