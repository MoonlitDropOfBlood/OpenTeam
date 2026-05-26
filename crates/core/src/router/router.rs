use std::collections::HashMap;
use tokio::sync::mpsc;
use crate::agent::handle::AgentCommand;
use crate::registry::{AgentId, AgentRegistry};
use crate::registry::registry::AgentRecord;

/// Routes incoming Feishu messages to the correct agent(s)
pub struct MessageRouter {
    agent_senders: HashMap<AgentId, mpsc::Sender<AgentCommand>>,
}

/// Check if a message matches any agent's trigger patterns
pub fn find_matching_agents<'a>(
    content: &str,
    registry: &'a AgentRegistry,
) -> Vec<&'a AgentRecord> {
    let mut matched = Vec::new();
    for agent in registry.all() {
        for trigger in &agent.config.triggers {
            // Simple substring match (Phase 3 V3: regex)
            if content.contains(&trigger.pattern) {
                matched.push(agent);
                break;
            }
            // Also check @mention by agent name
            if content.contains(&format!("@{}", agent.config.name)) {
                if !matched.iter().any(|a| a.config.name == agent.config.name) {
                    matched.push(agent);
                }
                break;
            }
        }
    }
    matched
}

impl MessageRouter {
    pub fn new() -> Self {
        Self { agent_senders: HashMap::new() }
    }

    pub fn register_agent(&mut self, id: AgentId, sender: mpsc::Sender<AgentCommand>) {
        self.agent_senders.insert(id, sender);
    }

    pub fn unregister_agent(&mut self, id: &AgentId) {
        self.agent_senders.remove(id);
    }

    /// Route a Feishu message to the appropriate agent(s)
    /// Matches by trigger patterns, @mentions, and agent name
    /// Returns list of agent IDs that received the message
    pub async fn route_message(
        &self,
        content: &str,
        registry: &AgentRegistry,
    ) -> Vec<AgentId> {
        let mut targeted = Vec::new();

        // First, check trigger patterns and @mentions
        let matched = find_matching_agents(content, registry);
        for agent in matched {
            if let Some(sender) = self.agent_senders.get(&agent.id) {
                let cmd = AgentCommand::InjectMessage(content.to_string());
                if sender.send(cmd).await.is_ok() {
                    targeted.push(agent.id);
                }
            }
        }

        // Fallback: also check by agent name substring (backward compat)
        if targeted.is_empty() {
            for agent in registry.all() {
                if content.contains(&agent.config.name) {
                    if let Some(sender) = self.agent_senders.get(&agent.id) {
                        let cmd = AgentCommand::InjectMessage(content.to_string());
                        if sender.send(cmd).await.is_ok() {
                            targeted.push(agent.id);
                        }
                    }
                }
            }
        }

        if targeted.is_empty() {
            tracing::debug!("MessageRouter: no agent matched for content");
        }

        targeted
    }

    pub fn agent_count(&self) -> usize {
        self.agent_senders.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_register_and_count() {
        let mut router = MessageRouter::new();
        let (tx, _rx) = tokio::sync::mpsc::channel(64);
        let id = uuid::Uuid::now_v7();
        router.register_agent(id, tx);
        assert_eq!(router.agent_count(), 1);
        router.unregister_agent(&id);
        assert_eq!(router.agent_count(), 0);
    }
}
