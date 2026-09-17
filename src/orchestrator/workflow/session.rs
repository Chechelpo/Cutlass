//! Executable workflow factories. Configuration and selection create no sessions.

use super::{BasicWorkflow, workflow::WorkflowError};
use crate::agent::steering::SteeringInbox;
use crate::agent::interactions::UserInteractionBroker;
use crate::agent::presets::registry::AgentPresetRegistry;
use crate::agent::sandbox::filesystem::SandboxedFilesystem;
use crate::config::ModelConfig;
use crate::ui_interface::chat::RenderMessageSection;
use tracing::{debug, info};

/// A live workflow, potentially coordinating multiple agent sessions.
/// Constructed and used on the same worker thread, so it need not be `Send`.
pub trait WorkflowSession {
    fn submit(
        &mut self,
        prompt: String,
        emit: &mut dyn FnMut(RenderMessageSection),
    ) -> Result<(), WorkflowError>;

    /// A shared handle used to steer or stop the currently running turn.
    fn steering_inbox(&self) -> Option<SteeringInbox> {
        None
    }
}

pub struct WorkflowContext<'a> {
    pub workspace: SandboxedFilesystem,
    pub model: &'a ModelConfig,
    pub agents: &'a AgentPresetRegistry,
    pub user_interactions: Option<UserInteractionBroker>,
}

pub type WorkflowFactory =
    for<'a> fn(WorkflowContext<'a>) -> Result<Box<dyn WorkflowSession + 'a>, WorkflowError>;

#[derive(Clone)]
pub struct WorkflowDefinition {
    pub name: String,
    pub description: String,
    pub create: WorkflowFactory,
}

/// A catalog of factories, separate from the existing workflow metadata registry.
pub fn built_in_workflows() -> Vec<WorkflowDefinition> {
    let workflows = vec![WorkflowDefinition {
        name: "Basic".into(),
        description: "A persistent conversation with the Coder agent and its tools.".into(),
        create: |context| {
            let agent = context
                .agents
                .get_with_name("Coder")
                .ok_or_else(|| WorkflowError {
                    message: "The Coder preset is unavailable".into(),
                    retryable: false,
                })?;
            Ok(Box::new(BasicWorkflow::with_user_interactions(
                context.workspace,
                context.model,
                agent,
                context.user_interactions,
            )))
        },
    }];
    debug!(workflow_count = workflows.len(), "registered built-in workflows");
    workflows
}

impl WorkflowSession for BasicWorkflow<'_> {
    fn submit(
        &mut self,
        prompt: String,
        emit: &mut dyn FnMut(RenderMessageSection),
    ) -> Result<(), WorkflowError> {
        info!(workflow = "Basic", prompt_chars = prompt.chars().count(), "submitting workflow turn");
        self.session_mut()
            .run_with_events(prompt, emit)
            .map_err(|error| WorkflowError {
                message: error.message,
                retryable: error.is_retryable,
            })
    }

    fn steering_inbox(&self) -> Option<SteeringInbox> {
        Some(self.session().steering_inbox.clone())
    }
}
