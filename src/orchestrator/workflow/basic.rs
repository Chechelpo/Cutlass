//! The default, Codex-style single-agent workflow.

use crate::agent::agent_session::{Agent, AgentSession};
use crate::agent::sandbox::filesystem::SandboxedFilesystem;
use crate::chat_completions::api::client::ApiError;
use crate::chat_completions::messages::Message;
use crate::config::ModelConfig;

/// A persistent conversation supervised by a single agent.
///
/// Each call to [`run`](Self::run) adds one user prompt and lets the agent
/// execute tool rounds until it produces a final assistant response.
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

    pub fn run(&mut self, prompt: impl Into<String>) -> Result<&[Message], ApiError> {
        self.session.run(prompt.into())?;
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
