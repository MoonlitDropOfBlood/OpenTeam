use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    /// Provider configurations (keyed by provider ID)
    #[serde(default)]
    pub provider: HashMap<String, super::provider::ProviderConfig>,
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
"#;
        let config: LlmConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.provider.contains_key("test-provider"));
    }

    #[test]
    fn test_llm_config_provider_options_parsed() {
        let yaml = r#"
provider:
  custom:
    name: "Custom API"
    options:
      baseUrl: https://custom.ai/v1
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
