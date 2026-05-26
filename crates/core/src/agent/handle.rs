use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tokio::sync::mpsc;
use crate::config::agent::AgentConfig;
use crate::registry::AgentId;

/// Commands sent to an agent's control channel
#[derive(Debug, Clone)]
pub enum AgentCommand {
    Stop,
    Pause,
    Resume,
    InjectMessage { content: String, thread_id: Option<String> },
    OverrideContext(String),
}

/// Handle to a running agent task
pub struct AgentHandle {
    pub id: AgentId,
    pub config: AgentConfig,
    pub join_handle: JoinHandle<()>,
    pub control_tx: mpsc::Sender<AgentCommand>,
    pub cancel_token: CancellationToken,
}
