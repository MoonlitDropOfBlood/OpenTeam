use crate::config::agent::ModelConfig;
use crate::config::provider::ProviderConfig;
use crate::llm::models::{ModelLimits, ModelCapabilities, ProviderDefaults};
use std::collections::HashMap;

/// A fully resolved model with all configuration merged:
/// built-in defaults → provider config → agent overrides
/// NOTE: All fields are FLAT — no nested model_config reference.
#[derive(Debug, Clone)]
pub struct ResolvedModel {
    pub provider: String,
    pub model_name: String,
    pub api_model: String,
    pub base_url: String,
    pub api_key: String,
    pub api_key_is_direct: bool,
    pub max_tokens: u32,
    pub timeout_secs: u64,
    pub chunk_timeout_secs: u64,
    pub set_cache_key: bool,
    pub headers: Vec<(String, String)>,
    pub limits: ModelLimits,
    pub capabilities: ModelCapabilities,
    pub can_reason: bool,
    pub default_max_tokens: u32,
    pub skip_verify_ssl: bool,
    // Agent-level optional params (flat)
    pub temperature: Option<f64>,
    pub top_p: Option<f64>,
    pub top_k: Option<u32>,
    pub stop: Option<Vec<String>>,
    pub presence_penalty: Option<f64>,
    pub frequency_penalty: Option<f64>,
    pub reasoning_effort: Option<String>,
    pub thinking: Option<bool>,
    pub max_retries: Option<u32>,
    /// Unique key for rate limiting — "{provider}/{model_name}"
    pub rate_limiter_key: String,
}

#[derive(Debug, Clone)]
pub struct ProviderResolver {
    provider_configs: HashMap<String, ProviderConfig>,
}

impl ProviderResolver {
    pub fn new(provider_configs: HashMap<String, ProviderConfig>) -> Self {
        Self { provider_configs }
    }

    /// Resolve a ModelConfig into a fully resolved ResolvedModel.
    /// Resolution priority (highest wins):
    ///   1. Agent ModelConfig explicit fields
    ///   2. User provider config (from YAML)
    ///   3. Built-in defaults (from Rust code)
    ///   4. Generic fallback
    pub fn resolve(&self, config: &ModelConfig) -> ResolvedModel {
        let provider = config.provider();
        let model_name = config.model_name();

        // Layer 3: Built-in defaults
        let builtin_model = crate::llm::models::resolve_builtin_model(provider, model_name);
        let provider_default = crate::llm::models::builtin_provider_defaults()
            .iter()
            .find(|p| p.name == provider);

        // Layer 2: User provider config
        let user_provider = self.provider_configs.get(provider);
        let user_model = user_provider.and_then(|p| p.models.get(model_name));

        // base_url: agent → user provider → builtin → generic
        let base_url = config
            .base_url
            .clone()
            .or_else(|| user_provider.and_then(|p| p.options.base_url.clone()))
            .or_else(|| provider_default.map(|p| p.base_url.clone()))
            .unwrap_or_else(|| format!("https://api.{}.com/v1", provider));

        // api_key resolution
        let (api_key, api_key_is_direct) =
            Self::resolve_api_key(config, user_provider, provider_default);

        // timeout_secs: agent → user provider (ms→s) → builtin (ms→s)
        let timeout_secs = config
            .timeout_secs
            .or_else(|| {
                user_provider
                    .and_then(|p| p.options.timeout)
                    .map(|ms| (ms / 1000).max(1) as u64)
            })
            .or_else(|| {
                provider_default
                    .map(|p| (p.timeout_ms / 1000).max(1) as u64)
            })
            .unwrap_or(300);

        // headers: user provider + user model + builtin model (no override)
        let mut headers: Vec<(String, String)> = Vec::new();
        if let Some(up) = user_provider {
            for (k, v) in &up.options.headers {
                headers.push((k.clone(), v.clone()));
            }
            if let Some(um) = user_model {
                for (k, v) in &um.headers {
                    headers.push((k.clone(), v.clone()));
                }
            }
        }
        if let Some(bm) = builtin_model {
            if let Some(bh) = &bm.headers {
                for (k, v) in bh {
                    if !headers.iter().any(|(hk, _)| hk == k) {
                        headers.push((k.clone(), v.clone()));
                    }
                }
            }
        }

        let capabilities = builtin_model
            .map(|m| m.capabilities.clone())
            .unwrap_or_default();

        let limits = builtin_model
            .map(|m| m.limits.clone())
            .unwrap_or_default();

        let set_cache_key = user_provider
            .and_then(|p| p.options.set_cache_key)
            .unwrap_or(false);

        let chunk_timeout_secs = user_provider
            .and_then(|p| p.options.chunk_timeout)
            .map(|ms| (ms / 1000).max(1) as u64)
            .unwrap_or(30);

        let api_model = user_model
            .and_then(|m| m.id.clone())
            .or_else(|| builtin_model.map(|m| m.api_model.clone()))
            .unwrap_or_else(|| model_name.to_string());

        // Flat params from ModelConfig
        let skip_verify_ssl = config.skip_verify_ssl.unwrap_or(false);
        let rate_limiter_key = format!("{}/{}", provider, model_name);

        // Cap max_tokens using model's output limit
        let max_tokens = if limits.output > 0 && config.max_tokens > limits.output {
            tracing::warn!(
                "Model {}/{}: capping max_tokens from {} to {} (model output limit)",
                provider, model_name, config.max_tokens, limits.output
            );
            limits.output
        } else {
            config.max_tokens
        };

        ResolvedModel {
            provider: provider.to_string(),
            model_name: model_name.to_string(),
            api_model,
            base_url,
            api_key,
            api_key_is_direct,
            max_tokens: config.max_tokens,
            timeout_secs,
            chunk_timeout_secs,
            set_cache_key,
            headers,
            limits,
            capabilities: capabilities.clone(),
            can_reason: capabilities.can_reason,
            default_max_tokens: builtin_model
                .map(|m| m.default_max_tokens)
                .unwrap_or(4096),
            skip_verify_ssl,
            temperature: config.temperature,
            top_p: config.top_p,
            top_k: config.top_k,
            stop: config.stop.clone(),
            presence_penalty: config.presence_penalty,
            frequency_penalty: config.frequency_penalty,
            reasoning_effort: config.reasoning_effort.clone(),
            thinking: config.thinking,
            max_retries: config.max_retries,
            rate_limiter_key,
        }
    }

    fn resolve_api_key(
        config: &ModelConfig,
        user_provider: Option<&ProviderConfig>,
        provider_default: Option<&ProviderDefaults>,
    ) -> (String, bool) {
        // Priority 1: Agent config's api_key_env
        if let Some(env) = &config.api_key_env {
            return (env.clone(), false);
        }

        // Priority 2: User provider config's options.api_key (direct value)
        if let Some(up) = user_provider {
            if let Some(api_key) = &up.options.api_key {
                // Check if it's an {env:VAR} reference
                if api_key.starts_with("{env:") && api_key.ends_with("}") {
                    let env_var = &api_key[5..api_key.len() - 1];
                    return (env_var.to_string(), false);
                }
                // Direct API key value
                return (api_key.clone(), true);
            }
        }

        // Priority 3: Built-in provider default env var
        if let Some(pd) = provider_default {
            if !pd.api_key_env.is_empty() {
                return (pd.api_key_env.clone(), false);
            }
        }

        ("OPENAI_API_KEY".to_string(), false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::agent::ModelConfig;
    use crate::config::provider::ProviderOptions;

    fn make_model_config(model: &str, max_tokens: u32) -> ModelConfig {
        ModelConfig {
            model: model.to_string(),
            api_key_env: None,
            max_tokens,
            fallback: None,
            timeout_secs: None,
            rate_limit: None,
            base_url: None,
            temperature: None,
            top_p: None,
            top_k: None,
            stop: None,
            presence_penalty: None,
            frequency_penalty: None,
            reasoning_effort: None,
            thinking: None,
            max_retries: None,
            skip_verify_ssl: None,
        }
    }

    #[test]
    fn test_resolve_anthropic_model() {
        let resolver = ProviderResolver::new(HashMap::new());
        let config = make_model_config("anthropic/claude-sonnet-4-20250514", 8192);
        let resolved = resolver.resolve(&config);

        assert_eq!(resolved.provider, "anthropic");
        assert_eq!(resolved.model_name, "claude-sonnet-4-20250514");
        assert_eq!(resolved.api_model, "claude-sonnet-4-20250514");
        assert_eq!(resolved.base_url, "https://api.anthropic.com/v1");
        assert!(resolved.can_reason);
        assert_eq!(resolved.max_tokens, 8192);
        assert_eq!(resolved.timeout_secs, 300);
        assert_eq!(
            resolved.rate_limiter_key,
            "anthropic/claude-sonnet-4-20250514"
        );
    }

    #[test]
    fn test_resolve_deepseek_model() {
        let resolver = ProviderResolver::new(HashMap::new());
        let config = make_model_config("deepseek/deepseek-v4-flash", 4096);
        let resolved = resolver.resolve(&config);

        assert_eq!(resolved.provider, "deepseek");
        assert_eq!(resolved.api_model, "deepseek-v4-flash");
        assert!(resolved.can_reason);
        assert_eq!(resolved.limits.context, 64000);
    }

    #[test]
    fn test_resolve_with_agent_override() {
        let resolver = ProviderResolver::new(HashMap::new());
        let mut config = make_model_config("anthropic/claude-sonnet-4-20250514", 16384);
        config.base_url = Some("https://custom-proxy.com/v1".into());
        config.api_key_env = Some("MY_CUSTOM_KEY".into());
        let resolved = resolver.resolve(&config);

        assert_eq!(resolved.base_url, "https://custom-proxy.com/v1");
        assert_eq!(resolved.max_tokens, 16384);
        assert!(!resolved.api_key_is_direct);
        assert_eq!(resolved.api_key, "MY_CUSTOM_KEY");
    }

    #[test]
    fn test_resolve_unknown_model() {
        let resolver = ProviderResolver::new(HashMap::new());
        let config = make_model_config("unknown/custom-model-v1", 4096);
        let resolved = resolver.resolve(&config);

        assert_eq!(resolved.provider, "unknown");
        assert_eq!(resolved.api_model, "custom-model-v1");
        assert!(!resolved.can_reason);
        assert_eq!(resolved.base_url, "https://api.unknown.com/v1");
    }

    #[test]
    fn test_resolve_openai_model() {
        let resolver = ProviderResolver::new(HashMap::new());
        let config = make_model_config("openai/gpt-4o", 4096);
        let resolved = resolver.resolve(&config);

        assert_eq!(resolved.provider, "openai");
        assert_eq!(resolved.api_model, "gpt-4o");
        assert!(!resolved.can_reason);
        assert_eq!(resolved.limits.context, 128000);
    }

    #[test]
    fn test_provider_config_overrides_builtin() {
        let mut provider_configs = HashMap::new();
        provider_configs.insert(
            "deepseek".to_string(),
            ProviderConfig {
                name: Some("My DeepSeek".into()),
                npm: None,
                env: vec![],
                whitelist: None,
                blacklist: None,
                options: ProviderOptions {
                    base_url: Some("https://my-deepseek-proxy.com/v1".into()),
                    timeout: Some(60000),
                    ..Default::default()
                },
                models: HashMap::new(),
            },
        );

        let resolver = ProviderResolver::new(provider_configs);
        let config = make_model_config("deepseek/deepseek-v4-pro", 8192);
        let resolved = resolver.resolve(&config);

        assert_eq!(resolved.base_url, "https://my-deepseek-proxy.com/v1");
        assert_eq!(resolved.timeout_secs, 60);
    }

    #[test]
    fn test_resolve_env_var_key_detection() {
        let mut provider_configs = HashMap::new();
        provider_configs.insert("test".to_string(), ProviderConfig {
            name: Some("Test".into()),
            npm: None,
            env: vec![],
            whitelist: None,
            blacklist: None,
            options: ProviderOptions {
                api_key: Some("{env:MY_CUSTOM_KEY}".into()),
                ..Default::default()
            },
            models: HashMap::new(),
        });

        let resolver = ProviderResolver::new(provider_configs);
        let config = make_model_config("test/test-model", 4096);
        let resolved = resolver.resolve(&config);

        assert_eq!(resolved.api_key, "MY_CUSTOM_KEY");
        assert!(!resolved.api_key_is_direct);
    }

    #[test]
    fn test_resolve_direct_api_key() {
        let mut provider_configs = HashMap::new();
        provider_configs.insert("test".to_string(), ProviderConfig {
            name: Some("Test".into()),
            npm: None,
            env: vec![],
            whitelist: None,
            blacklist: None,
            options: ProviderOptions {
                api_key: Some("sk-my-direct-key".into()),
                ..Default::default()
            },
            models: HashMap::new(),
        });

        let resolver = ProviderResolver::new(provider_configs);
        let config = make_model_config("test/test-model", 4096);
        let resolved = resolver.resolve(&config);

        assert_eq!(resolved.api_key, "sk-my-direct-key");
        assert!(resolved.api_key_is_direct);
    }

    #[test]
    fn test_resolve_with_skip_verify_ssl() {
        let resolver = ProviderResolver::new(HashMap::new());
        let mut config = make_model_config("deepseek/deepseek-v4-pro", 8192);
        config.skip_verify_ssl = Some(true);
        let resolved = resolver.resolve(&config);
        assert!(resolved.skip_verify_ssl);
    }

    #[test]
    fn test_resolve_with_custom_temperature() {
        let resolver = ProviderResolver::new(HashMap::new());
        let mut config = make_model_config("anthropic/claude-sonnet-4-20250514", 8192);
        config.temperature = Some(0.7);
        config.top_p = Some(0.9);
        let resolved = resolver.resolve(&config);
        assert_eq!(resolved.temperature, Some(0.7));
        assert_eq!(resolved.top_p, Some(0.9));
    }
}