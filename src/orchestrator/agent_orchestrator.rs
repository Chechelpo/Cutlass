//! Multi-agent orchestration reserved for a future workflow.

use crate::agent::agent_session::{Agent, AgentSession};
use crate::agent::sandbox::filesystem::SandboxedFilesystem;
use crate::config::ModelConfigStore;

/// Manages active and historical agent sessions for future multi-agent modes.
///
/// The model configuration store is borrowed because sessions retain a
/// reference to their selected configuration. The MVP does not construct or
/// use this manager.
#[allow(dead_code)]
pub(crate) struct AgentManager<'a> {
    model_config_store: &'a ModelConfigStore,
    agent_sessions: Vec<AgentSession<'a>>,
    running_agents: Vec<String>,
    orchestrator_agent: AgentSession<'a>,
}

#[allow(dead_code)]
impl<'a> AgentManager<'a> {
    pub fn spawn_agent(&mut self, agent: &'a Agent, workspace: SandboxedFilesystem) {
        let model_config: &'a _ = self
            .model_config_store
            .active_config()
            .expect("No active config");
        let session = AgentSession::new(workspace, model_config, agent);

        self.running_agents.push(session.name.clone());
        self.agent_sessions.push(session);
    }

    pub fn agent(&self, name: &str) -> Option<&AgentSession<'a>> {
        self.agent_sessions
            .iter()
            .find(|session| session.name == name)
    }
}
