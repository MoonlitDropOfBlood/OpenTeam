use std::collections::HashMap;
use crate::config::agent::AgentConfig;
use crate::CoreError;
use uuid::Uuid;

pub type AgentId = Uuid;

#[derive(Debug, Clone, PartialEq)]
pub enum AgentStatus {
    Idle,
    Busy,
    Paused,
    Offline,
}

#[derive(Debug, Clone)]
pub struct AgentRecord {
    pub id: AgentId,
    pub config: AgentConfig,
    pub status: AgentStatus,
    pub current_task: Option<String>,
}

#[derive(Clone)]
pub struct AgentRegistry {
    agents: HashMap<AgentId, AgentRecord>,
}

impl AgentRegistry {
    pub fn new() -> Self {
        Self { agents: HashMap::new() }
    }

    pub fn register(&mut self, config: AgentConfig) -> AgentId {
        let id = Uuid::now_v7();
        let record = AgentRecord {
            id,
            config,
            status: AgentStatus::Idle,
            current_task: None,
        };
        self.agents.insert(id, record);
        id
    }

    pub fn get(&self, id: &AgentId) -> Option<&AgentRecord> {
        self.agents.get(id)
    }

    pub fn find_by_role(&self, role_keyword: &str) -> Vec<&AgentRecord> {
        self.agents.values()
            .filter(|a| a.config.role.contains(role_keyword))
            .collect()
    }

    pub fn find_idle(&self) -> Vec<&AgentRecord> {
        self.agents.values()
            .filter(|a| a.status == AgentStatus::Idle)
            .collect()
    }

    pub fn update_status(&mut self, id: &AgentId, status: AgentStatus) -> Result<(), CoreError> {
        self.agents.get_mut(id)
            .map(|record| record.status = status)
            .ok_or_else(|| CoreError::Registry(format!("Agent {id} not found")))
    }

    pub fn all(&self) -> Vec<&AgentRecord> {
        self.agents.values().collect()
    }

    /// Remove an agent from the registry by ID. Returns the removed record.
    pub fn remove(&mut self, id: &AgentId) -> Option<AgentRecord> {
        self.agents.remove(id)
    }

    /// Find an agent by name
    pub fn find_by_name(&self, name: &str) -> Option<&AgentRecord> {
        self.agents.values().find(|r| r.config.name == name)
    }
}
