use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Provider configuration — mirrors OpenCode's ProviderConfig schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    /// Display name for the provider
    pub name: Option<String>,
    /// AI SDK npm package (informational, not used in Rust)
    pub npm: Option<String>,
    /// Environment variable names for auto-detecting API keys
    #[serde(default)]
    pub env: Vec<String>,
    /// Only allow these models (whitelist)
    pub whitelist: Option<Vec<String>>,
    /// Block these models (blacklist)
    pub blacklist: Option<Vec<String>>,
    /// Provider-level options
    #[serde(default)]
    pub options: ProviderOptions,
    /// Per-model configuration overrides
    #[serde(default)]
    pub models: HashMap<String, ProviderModelConfig>,
}

/// Provider-level options — all fields are optional, defaults come from built-in
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderOptions {
    /// API key value or "{env:VAR}" reference
    pub api_key: Option<String>,
    /// Custom base URL
    pub base_url: Option<String>,
    /// Request timeout in milliseconds (set to 0 for no timeout)
    pub timeout: Option<u32>,
    /// SSE chunk timeout in milliseconds
    pub chunk_timeout: Option<u32>,
    /// Enable prompt caching
    pub set_cache_key: Option<bool>,
    /// Custom HTTP headers
    #[serde(default)]
    pub headers: HashMap<String, String>,
    /// GitHub Enterprise URL (for copilot provider). Reserved for future use.
    #[allow(dead_code)]
    pub enterprise_url: Option<String>,
}

impl Default for ProviderOptions {
    fn default() -> Self {
        Self {
            api_key: None,
            base_url: None,
            timeout: None,
            chunk_timeout: None,
            set_cache_key: None,
            headers: HashMap::new(),
            enterprise_url: None,
        }
    }
}

/// Per-model configuration overrides
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderModelConfig {
    /// Override model ID sent to API
    pub id: Option<String>,
    /// Display name
    pub name: Option<String>,
    /// Token cost overrides
    pub cost: Option<super::llm::ModelCostConfig>,
    /// Token limit overrides
    pub limit: Option<super::llm::ModelLimitConfig>,
    /// Per-model options (overrides provider-level options)
    #[serde(default)]
    pub options: HashMap<String, String>,
    /// Per-model custom headers
    #[serde(default)]
    pub headers: HashMap<String, String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_options_default() {
        let opts = ProviderOptions::default();
        assert!(opts.base_url.is_none());
        assert!(opts.api_key.is_none());
        assert!(opts.timeout.is_none());
        assert!(opts.headers.is_empty());
    }

    #[test]
    fn test_provider_config_minimal() {
        let yaml = r#"
name: "Test"
options:
  base_url: https://test.com/v1
  timeout: 60000
"#;
        let config: ProviderConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.name.unwrap(), "Test");
        assert_eq!(config.options.base_url.unwrap(), "https://test.com/v1");
        assert_eq!(config.options.timeout.unwrap(), 60000);
        assert!(config.models.is_empty());
    }

    #[test]
    fn test_provider_config_with_models() {
        let yaml = r#"
name: "Test Provider"
models:
  my-model:
    name: "My Model"
    limit:
      context: 64000
      output: 4096
"#;
        let config: ProviderConfig = serde_yaml::from_str(yaml).unwrap();
        let model = config.models.get("my-model").unwrap();
        assert_eq!(model.name.as_deref(), Some("My Model"));
        let limit = model.limit.as_ref().unwrap();
        assert_eq!(limit.context, Some(64000));
        assert_eq!(limit.output, Some(4096));
    }

    #[test]
    fn test_provider_config_with_headers() {
        let yaml = r#"
options:
  baseURL: https://api.example.com/v1
  headers:
    X-Custom: value1
    Authorization: Bearer token
"#;
        let config: ProviderConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.options.headers.get("X-Custom").unwrap(), "value1");
        assert_eq!(config.options.headers.get("Authorization").unwrap(), "Bearer token");
    }
}
