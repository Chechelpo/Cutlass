//! Executable workflow factories. Configuration and selection create no sessions.

use super::{BasicWorkflow, workflow::WorkflowError};
use crate::agent::presets::registry::AgentPresetRegistry;
use crate::agent::sandbox::filesystem::SandboxedFilesystem;
use crate::config::ModelConfig;
use crate::ui_interface::chat::RenderMessageSection;

/// A live workflow, potentially coordinating multiple agent sessions.
/// Constructed and used on the same worker thread, so it need not be `Send`.
pub trait WorkflowSession {
    fn submit(
        &mut self,
        prompt: String,
        emit: &mut dyn FnMut(RenderMessageSection),
    ) -> Result<(), WorkflowError>;
}

pub struct WorkflowContext<'a> {
    pub workspace: SandboxedFilesystem,
    pub model: &'a ModelConfig,
    pub agents: &'a AgentPresetRegistry,
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
    vec![WorkflowDefinition {
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
            Ok(Box::new(BasicWorkflow::new(
                context.workspace,
                context.model,
                agent,
            )))
        },
    }]
}

impl WorkflowSession for BasicWorkflow<'_> {
    fn submit(
        &mut self,
        prompt: String,
        emit: &mut dyn FnMut(RenderMessageSection),
    ) -> Result<(), WorkflowError> {
        self.session_mut()
            .run_with_events(prompt, emit)
            .map_err(|error| WorkflowError {
                message: error.message,
                retryable: error.is_retryable,
            })
    }
}
