use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use crate::config::agent::ModelConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    pub models: HashMap<String, ModelConfig>,
}
