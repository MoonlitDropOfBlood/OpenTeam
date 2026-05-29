use serde::Deserialize;
use crate::CoreError;

/// Top-level global config structure
#[derive(Debug, Clone, Deserialize, Default)]
pub struct GlobalConfig {
    #[serde(default)]
    pub feishu: Option<FeishuConfig>,
}

/// Feishu-specific config
#[derive(Debug, Clone, Deserialize)]
pub struct FeishuConfig {
    pub app_id: Option<String>,
    pub app_secret: Option<String>,
    pub chat_id: Option<String>,
}

/// Default config path: ~/.config/OpenTeam/config.yaml
pub fn global_config_path() -> std::path::PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".into());
    std::path::PathBuf::from(home).join(".config/OpenTeam/config.yaml")
}

/// Load global config from the default path. Returns empty config if file doesn't exist.
pub fn load_global_config() -> Result<GlobalConfig, CoreError> {
    let path = global_config_path();
    if !path.exists() {
        return Ok(GlobalConfig { feishu: None });
    }
    let content = std::fs::read_to_string(&path)?;
    let config: GlobalConfig = serde_yaml::from_str(&content)?;
    Ok(config)
}

/// Resolve Feishu app_id: config file first, then env var, then error.
pub fn resolve_app_id(config: &GlobalConfig) -> Result<String, CoreError> {
    if let Some(f) = &config.feishu {
        if let Some(id) = &f.app_id {
            if !id.is_empty() && !id.starts_with("cli_xxx") {
                return Ok(id.clone());
            }
        }
    }
    std::env::var("FEISHU_APP_ID").map_err(|_| CoreError::Config(
        "FEISHU_APP_ID not found. Set it in ~/.config/OpenTeam/config.yaml or as env var.".into()
    ))
}

/// Resolve Feishu app_secret: config file first, then env var, then error.
pub fn resolve_app_secret(config: &GlobalConfig) -> Result<String, CoreError> {
    if let Some(f) = &config.feishu {
        if let Some(s) = &f.app_secret {
            if !s.is_empty() && !s.contains("xxx") {
                return Ok(s.clone());
            }
        }
    }
    std::env::var("FEISHU_APP_SECRET").map_err(|_| CoreError::Config(
        "FEISHU_APP_SECRET not found. Set it in ~/.config/OpenTeam/config.yaml or as env var.".into()
    ))
}

/// Resolve Feishu chat_id: config file first, then env var, then error.
pub fn resolve_chat_id(config: &GlobalConfig) -> Result<String, CoreError> {
    if let Some(f) = &config.feishu {
        if let Some(id) = &f.chat_id {
            if !id.is_empty() && !id.starts_with("oc_xxx") {
                return Ok(id.clone());
            }
        }
    }
    std::env::var("FEISHU_CHAT_ID").map_err(|_| CoreError::Config(
        "FEISHU_CHAT_ID not found. Set it in ~/.config/OpenTeam/config.yaml or as env var.".into()
    ))
}

#[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn test_load_global_config_no_panic() {
            // Should never panic — returns empty config if file missing, or parsed config if exists
            let config = load_global_config().unwrap();
            // If file exists, feishu may be Some; if not, it's None — both valid
            if config.feishu.is_none() {
                assert!(true, "no config file, empty config returned");
            } else {
                assert!(config.feishu.is_some(), "config file loaded");
            }
        }
    }