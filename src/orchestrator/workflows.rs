//! System-defined agent workflows.

use crate::agent::agent_session::{Agent, AgentSession};
use crate::agent::sandbox::filesystem::SandboxedFilesystem;
use crate::chat_completions::api::client::ApiError;
use crate::chat_completions::messages::Message;
use crate::config::ModelConfig;

/// The MVP workflow: one agent session handling one user conversation.
///
/// It deliberately exposes no agent-spawning or delegation capability. More
/// elaborate serial or multi-agent workflows can be added as separate types
/// without complicating the basic execution path.
pub struct BasicWorkflow<'a> {
    session: AgentSession<'a>,
}

impl<'a> BasicWorkflow<'a> {
    pub fn new(
        workspace: SandboxedFilesystem,
        model_config: &'a ModelConfig,
        agent: &'a Agent,
    ) -> Self {
        Self {
            session: AgentSession::new(workspace, model_config, agent),
        }
    }

    /// Sends one user message through the agent loop and returns the complete
    /// ordered history for rendering.
    pub fn run(&mut self, message: impl Into<String>) -> Result<&[Message], ApiError> {
        self.session.add_user_message(message);
        self.session.run()?;
        Ok(self.session.messages())
    }

    pub fn session(&self) -> &AgentSession<'a> {
        &self.session
    }

    pub fn session_mut(&mut self) -> &mut AgentSession<'a> {
        &mut self.session
    }

    pub fn messages(&self) -> &[Message] {
        self.session.messages()
    }
}
