use super::time_policy::*;

pub struct SecretaryAgent {
    pub time_policy_config: TimePolicyConfig,
    pub current_mode: WakeMode,
}

impl SecretaryAgent {
    pub fn new() -> Self {
        Self {
            time_policy_config: TimePolicyConfig::default(),
            current_mode: WakeMode::Proactive,
        }
    }

    /// Evaluate the current time policy based on wall clock and message content
    pub fn evaluate(&mut self, message: &str) -> &TimePolicyConfig {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let policy = self.time_policy_config.resolve(now, message);
        self.current_mode = policy.wake_mode.clone();
        &self.time_policy_config
    }

    /// Check if an urgent keyword is present (convenience method)
    pub fn is_urgent(&self, message: &str) -> bool {
        self.time_policy_config.urgent_keywords.iter().any(|k| message.contains(k.as_str()))
    }
}
