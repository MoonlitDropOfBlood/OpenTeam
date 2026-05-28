use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

/// Token limits for a model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelLimits {
    pub context: u32,    // max input + output context window
    pub input: u32,      // max input tokens (0 = same as context)
    pub output: u32,     // max output tokens (0 = use default_max_tokens)
}

impl Default for ModelLimits {
    fn default() -> Self {
        Self { context: 128000, input: 128000, output: 8192 }
    }
}

/// Token cost per 1M tokens (USD)
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ModelCost {
    pub input: f64,
    pub output: f64,
    pub cache_read: f64,
    pub cache_write: f64,
}

impl Default for ModelCost {
    fn default() -> Self {
        Self { input: 0.0, output: 0.0, cache_read: 0.0, cache_write: 0.0 }
    }
}

/// Model capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCapabilities {
    pub can_reason: bool,
    pub supports_attachments: bool,
    pub supports_tools: bool,
    pub supports_temperature: bool,
}

impl Default for ModelCapabilities {
    fn default() -> Self {
        Self {
            can_reason: false,
            supports_attachments: false,
            supports_tools: true,
            supports_temperature: true,
        }
    }
}

/// Built-in provider metadata (default endpoint and API key env var)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderDefaults {
    pub name: String,
    pub base_url: String,
    pub api_key_env: String,
    pub env_vars: Vec<String>,
    pub timeout_ms: u32,
}

/// A fully resolved model definition (built-in defaults + provider config overrides)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelDefinition {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub api_model: String,
    pub cost: ModelCost,
    pub limits: ModelLimits,
    pub capabilities: ModelCapabilities,
    pub default_max_tokens: u32,
    pub headers: Option<Vec<(String, String)>>,
}

// --- Built-in model definitions ---

static BUILTIN_MODELS: LazyLock<Vec<ModelDefinition>> = LazyLock::new(|| builtin_models_inner());

fn builtin_models_inner() -> Vec<ModelDefinition> {
    vec![
        // === Anthropic ===
        ModelDefinition {
            id: "claude-sonnet-4-20250514".into(),
            name: "Claude Sonnet 4".into(),
            provider: "anthropic".into(),
            api_model: "claude-sonnet-4-20250514".into(),
            cost: ModelCost { input: 3.0, output: 15.0, cache_read: 0.30, cache_write: 3.75 },
            limits: ModelLimits { context: 200000, input: 200000, output: 50000 },
            capabilities: ModelCapabilities {
                can_reason: true, supports_attachments: true, supports_tools: true, supports_temperature: true,
            },
            default_max_tokens: 50000,
            headers: None,
        },
        ModelDefinition {
            id: "claude-opus-4-20250514".into(),
            name: "Claude Opus 4".into(),
            provider: "anthropic".into(),
            api_model: "claude-opus-4-20250514".into(),
            cost: ModelCost { input: 15.0, output: 75.0, cache_read: 1.50, cache_write: 18.75 },
            limits: ModelLimits { context: 200000, input: 200000, output: 4096 },
            capabilities: ModelCapabilities {
                can_reason: true, supports_attachments: true, supports_tools: true, supports_temperature: true,
            },
            default_max_tokens: 4096,
            headers: None,
        },
        ModelDefinition {
            id: "claude-3-7-sonnet-latest".into(),
            name: "Claude 3.7 Sonnet".into(),
            provider: "anthropic".into(),
            api_model: "claude-3-7-sonnet-latest".into(),
            cost: ModelCost { input: 3.0, output: 15.0, cache_read: 0.30, cache_write: 3.75 },
            limits: ModelLimits { context: 200000, input: 200000, output: 50000 },
            capabilities: ModelCapabilities {
                can_reason: true, supports_attachments: true, supports_tools: true, supports_temperature: true,
            },
            default_max_tokens: 50000,
            headers: None,
        },
        ModelDefinition {
            id: "claude-3-5-sonnet-latest".into(),
            name: "Claude 3.5 Sonnet".into(),
            provider: "anthropic".into(),
            api_model: "claude-3-5-sonnet-latest".into(),
            cost: ModelCost { input: 3.0, output: 15.0, cache_read: 0.30, cache_write: 3.75 },
            limits: ModelLimits { context: 200000, input: 200000, output: 5000 },
            capabilities: ModelCapabilities {
                can_reason: false, supports_attachments: true, supports_tools: true, supports_temperature: true,
            },
            default_max_tokens: 5000,
            headers: None,
        },
        ModelDefinition {
            id: "claude-3-5-haiku-latest".into(),
            name: "Claude 3.5 Haiku".into(),
            provider: "anthropic".into(),
            api_model: "claude-3-5-haiku-latest".into(),
            cost: ModelCost { input: 0.80, output: 4.0, cache_read: 0.08, cache_write: 1.0 },
            limits: ModelLimits { context: 200000, input: 200000, output: 4096 },
            capabilities: ModelCapabilities {
                can_reason: false, supports_attachments: true, supports_tools: true, supports_temperature: true,
            },
            default_max_tokens: 4096,
            headers: None,
        },
        ModelDefinition {
            id: "claude-3-haiku-20240307".into(),
            name: "Claude 3 Haiku".into(),
            provider: "anthropic".into(),
            api_model: "claude-3-haiku-20240307".into(),
            cost: ModelCost { input: 0.25, output: 1.25, cache_read: 0.03, cache_write: 0.30 },
            limits: ModelLimits { context: 200000, input: 200000, output: 4096 },
            capabilities: ModelCapabilities {
                can_reason: false, supports_attachments: true, supports_tools: true, supports_temperature: true,
            },
            default_max_tokens: 4096,
            headers: None,
        },
        ModelDefinition {
            id: "claude-3-opus-latest".into(),
            name: "Claude 3 Opus".into(),
            provider: "anthropic".into(),
            api_model: "claude-3-opus-latest".into(),
            cost: ModelCost { input: 15.0, output: 75.0, cache_read: 1.50, cache_write: 18.75 },
            limits: ModelLimits { context: 200000, input: 200000, output: 4096 },
            capabilities: ModelCapabilities {
                can_reason: false, supports_attachments: true, supports_tools: true, supports_temperature: true,
            },
            default_max_tokens: 4096,
            headers: None,
        },
        // === DeepSeek ===
        ModelDefinition {
            id: "deepseek-v4-pro".into(),
            name: "DeepSeek V4 Pro".into(),
            provider: "deepseek".into(),
            api_model: "deepseek-v4-pro".into(),
            cost: ModelCost { input: 2.0, output: 8.0, cache_read: 0.0, cache_write: 0.0 },
            limits: ModelLimits { context: 64000, input: 64000, output: 8192 },
            capabilities: ModelCapabilities {
                can_reason: true, supports_attachments: false, supports_tools: true, supports_temperature: true,
            },
            default_max_tokens: 8192,
            headers: None,
        },
        ModelDefinition {
            id: "deepseek-v4-flash".into(),
            name: "DeepSeek V4 Flash".into(),
            provider: "deepseek".into(),
            api_model: "deepseek-v4-flash".into(),
            cost: ModelCost { input: 0.50, output: 2.0, cache_read: 0.0, cache_write: 0.0 },
            limits: ModelLimits { context: 64000, input: 64000, output: 8192 },
            capabilities: ModelCapabilities {
                can_reason: true, supports_attachments: false, supports_tools: true, supports_temperature: true,
            },
            default_max_tokens: 8192,
            headers: None,
        },
        ModelDefinition {
            id: "deepseek-chat".into(),
            name: "DeepSeek Chat".into(),
            provider: "deepseek".into(),
            api_model: "deepseek-chat".into(),
            cost: ModelCost { input: 0.14, output: 0.28, cache_read: 0.0, cache_write: 0.0 },
            limits: ModelLimits { context: 64000, input: 64000, output: 8192 },
            capabilities: ModelCapabilities {
                can_reason: false, supports_attachments: false, supports_tools: true, supports_temperature: true,
            },
            default_max_tokens: 8192,
            headers: None,
        },
        ModelDefinition {
            id: "deepseek-reasoner".into(),
            name: "DeepSeek Reasoner".into(),
            provider: "deepseek".into(),
            api_model: "deepseek-reasoner".into(),
            cost: ModelCost { input: 0.55, output: 2.19, cache_read: 0.0, cache_write: 0.0 },
            limits: ModelLimits { context: 64000, input: 64000, output: 8192 },
            capabilities: ModelCapabilities {
                can_reason: true, supports_attachments: false, supports_tools: true, supports_temperature: true,
            },
            default_max_tokens: 8192,
            headers: None,
        },
        // === OpenAI ===
        ModelDefinition {
            id: "gpt-4o".into(),
            name: "GPT 4o".into(),
            provider: "openai".into(),
            api_model: "gpt-4o".into(),
            cost: ModelCost { input: 2.50, output: 10.0, cache_read: 0.0, cache_write: 1.25 },
            limits: ModelLimits { context: 128000, input: 128000, output: 4096 },
            capabilities: ModelCapabilities {
                can_reason: false, supports_attachments: true, supports_tools: true, supports_temperature: true,
            },
            default_max_tokens: 4096,
            headers: None,
        },
        ModelDefinition {
            id: "gpt-4o-mini".into(),
            name: "GPT 4o Mini".into(),
            provider: "openai".into(),
            api_model: "gpt-4o-mini".into(),
            cost: ModelCost { input: 0.15, output: 0.60, cache_read: 0.0, cache_write: 0.075 },
            limits: ModelLimits { context: 128000, input: 128000, output: 4096 },
            capabilities: ModelCapabilities {
                can_reason: false, supports_attachments: true, supports_tools: true, supports_temperature: true,
            },
            default_max_tokens: 4096,
            headers: None,
        },
        ModelDefinition {
            id: "gpt-4.1".into(),
            name: "GPT 4.1".into(),
            provider: "openai".into(),
            api_model: "gpt-4.1".into(),
            cost: ModelCost { input: 2.0, output: 8.0, cache_read: 0.0, cache_write: 0.50 },
            limits: ModelLimits { context: 1047576, input: 1047576, output: 20000 },
            capabilities: ModelCapabilities {
                can_reason: false, supports_attachments: true, supports_tools: true, supports_temperature: true,
            },
            default_max_tokens: 20000,
            headers: None,
        },
        ModelDefinition {
            id: "gpt-4.1-mini".into(),
            name: "GPT 4.1 mini".into(),
            provider: "openai".into(),
            api_model: "gpt-4.1-mini".into(),
            cost: ModelCost { input: 0.40, output: 1.60, cache_read: 0.0, cache_write: 0.10 },
            limits: ModelLimits { context: 200000, input: 200000, output: 20000 },
            capabilities: ModelCapabilities {
                can_reason: false, supports_attachments: true, supports_tools: true, supports_temperature: true,
            },
            default_max_tokens: 20000,
            headers: None,
        },
        ModelDefinition {
            id: "gpt-4.1-nano".into(),
            name: "GPT 4.1 nano".into(),
            provider: "openai".into(),
            api_model: "gpt-4.1-nano".into(),
            cost: ModelCost { input: 0.10, output: 0.40, cache_read: 0.0, cache_write: 0.025 },
            limits: ModelLimits { context: 1047576, input: 1047576, output: 20000 },
            capabilities: ModelCapabilities {
                can_reason: false, supports_attachments: true, supports_tools: true, supports_temperature: true,
            },
            default_max_tokens: 20000,
            headers: None,
        },
        ModelDefinition {
            id: "o3".into(),
            name: "o3".into(),
            provider: "openai".into(),
            api_model: "o3".into(),
            cost: ModelCost { input: 10.0, output: 40.0, cache_read: 0.0, cache_write: 2.50 },
            limits: ModelLimits { context: 200000, input: 200000, output: 50000 },
            capabilities: ModelCapabilities {
                can_reason: true, supports_attachments: true, supports_tools: true, supports_temperature: true,
            },
            default_max_tokens: 50000,
            headers: None,
        },
        ModelDefinition {
            id: "o3-mini".into(),
            name: "o3 mini".into(),
            provider: "openai".into(),
            api_model: "o3-mini".into(),
            cost: ModelCost { input: 1.10, output: 4.40, cache_read: 0.0, cache_write: 0.55 },
            limits: ModelLimits { context: 200000, input: 200000, output: 50000 },
            capabilities: ModelCapabilities {
                can_reason: true, supports_attachments: false, supports_tools: true, supports_temperature: true,
            },
            default_max_tokens: 50000,
            headers: None,
        },
        ModelDefinition {
            id: "o4-mini".into(),
            name: "o4 mini".into(),
            provider: "openai".into(),
            api_model: "o4-mini".into(),
            cost: ModelCost { input: 1.10, output: 4.40, cache_read: 0.0, cache_write: 0.275 },
            limits: ModelLimits { context: 128000, input: 128000, output: 50000 },
            capabilities: ModelCapabilities {
                can_reason: true, supports_attachments: true, supports_tools: true, supports_temperature: true,
            },
            default_max_tokens: 50000,
            headers: None,
        },
        ModelDefinition {
            id: "o1".into(),
            name: "o1".into(),
            provider: "openai".into(),
            api_model: "o1".into(),
            cost: ModelCost { input: 15.0, output: 60.0, cache_read: 0.0, cache_write: 7.50 },
            limits: ModelLimits { context: 200000, input: 200000, output: 50000 },
            capabilities: ModelCapabilities {
                can_reason: true, supports_attachments: true, supports_tools: true, supports_temperature: true,
            },
            default_max_tokens: 50000,
            headers: None,
        },
        // === Ollama (generic fallback) ===
        ModelDefinition {
            id: "*".into(),
            name: "Ollama (local)".into(),
            provider: "ollama".into(),
            api_model: "*".into(),
            cost: ModelCost { input: 0.0, output: 0.0, cache_read: 0.0, cache_write: 0.0 },
            limits: ModelLimits { context: 128000, input: 128000, output: 4096 },
            capabilities: ModelCapabilities {
                can_reason: false, supports_attachments: false, supports_tools: false, supports_temperature: true,
            },
            default_max_tokens: 4096,
            headers: None,
        },
    ]
}

/// Look up a model definition from built-in defaults.
/// Uses LazyLock for static storage — no allocation per call.
pub fn resolve_builtin_model(provider: &str, model_name: &str) -> Option<&'static ModelDefinition> {
    BUILTIN_MODELS
        .iter()
        .find(|m| m.provider == provider && m.id == model_name)
}

/// Get all built-in provider defaults (uses LazyLock for static storage)
static BUILTIN_PROVIDER_DEFAULTS: LazyLock<Vec<ProviderDefaults>> = LazyLock::new(|| {
    vec![
        ProviderDefaults {
            name: "anthropic".into(),
            base_url: "https://api.anthropic.com/v1".into(),
            api_key_env: "ANTHROPIC_API_KEY".into(),
            env_vars: vec!["ANTHROPIC_API_KEY".into()],
            timeout_ms: 300000,
        },
        ProviderDefaults {
            name: "deepseek".into(),
            base_url: "https://api.deepseek.com/v1".into(),
            api_key_env: "DEEPSEEK_API_KEY".into(),
            env_vars: vec!["DEEPSEEK_API_KEY".into()],
            timeout_ms: 300000,
        },
        ProviderDefaults {
            name: "openai".into(),
            base_url: "https://api.openai.com/v1".into(),
            api_key_env: "OPENAI_API_KEY".into(),
            env_vars: vec!["OPENAI_API_KEY".into()],
            timeout_ms: 300000,
        },
        ProviderDefaults {
            name: "ollama".into(),
            base_url: "http://localhost:11434/api".into(),
            api_key_env: String::new(),
            env_vars: vec![],
            timeout_ms: 60000,
        },
    ]
});

pub fn builtin_provider_defaults() -> &'static [ProviderDefaults] {
    &BUILTIN_PROVIDER_DEFAULTS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_anthropic_model() {
        let model = resolve_builtin_model("anthropic", "claude-sonnet-4-20250514");
        assert!(model.is_some());
        let m = model.unwrap();
        assert_eq!(m.name, "Claude Sonnet 4");
        assert!(m.capabilities.can_reason);
        assert!(m.capabilities.supports_attachments);
        assert_eq!(m.limits.context, 200000);
    }

    #[test]
    fn test_resolve_deepseek_model() {
        let model = resolve_builtin_model("deepseek", "deepseek-v4-flash");
        assert!(model.is_some());
        let m = model.unwrap();
        assert_eq!(m.name, "DeepSeek V4 Flash");
        assert!(m.capabilities.can_reason);
        assert_eq!(m.limits.context, 64000);
    }

    #[test]
    fn test_resolve_openai_model() {
        let model = resolve_builtin_model("openai", "gpt-4o");
        assert!(model.is_some());
        let m = model.unwrap();
        assert_eq!(m.name, "GPT 4o");
        assert_eq!(m.limits.context, 128000);
    }

    #[test]
    fn test_resolve_unknown_model() {
        let model = resolve_builtin_model("unknown", "fake-model");
        assert!(model.is_none());
    }

    #[test]
    fn test_builtin_provider_defaults() {
        let defaults = builtin_provider_defaults();
        let anthropic = defaults.iter().find(|p| p.name == "anthropic").unwrap();
        assert_eq!(anthropic.base_url, "https://api.anthropic.com/v1");
        assert_eq!(anthropic.api_key_env, "ANTHROPIC_API_KEY");

        let ollama = defaults.iter().find(|p| p.name == "ollama").unwrap();
        assert_eq!(ollama.base_url, "http://localhost:11434/api");
    }

    #[test]
    fn test_model_limits_default() {
        let limits = ModelLimits::default();
        assert_eq!(limits.context, 128000);
        assert_eq!(limits.output, 8192);
    }

    #[test]
    fn test_model_capabilities_default() {
        let caps = ModelCapabilities::default();
        assert!(!caps.can_reason);
        assert!(caps.supports_tools);
    }
}
