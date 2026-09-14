//! UI-agnostic access to a user-facing agent session.

use crate::chat_completions::api::client::ApiError;
use crate::chat_completions::messages::Message;
use crate::orchestrator::BasicWorkflow;

/// Lifecycle state exposed to GUI, TUI, and other frontend implementations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SessionState {
    Ready,
    Running,
    Complete,
    CancellationRequested,
    Failed(UiError),
}

/// Provider error data suitable for presentation by a frontend.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiError {
    pub status: usize,
    pub message: String,
    pub retryable: bool,
}

impl From<ApiError> for UiError {
    fn from(error: ApiError) -> Self {
        Self {
            status: error.status,
            message: error.message,
            retryable: error.is_retryable,
        }
    }
}

/// Frontend-facing façade for one user conversation.
///
/// Frontends should depend on this type rather than accessing the workflow or
/// agent session directly. It contains no rendering framework assumptions.
pub struct UserSessionViewport<'a> {
    workflow: BasicWorkflow<'a>,
    state: SessionState,
}

impl<'a> UserSessionViewport<'a> {
    pub fn new(workflow: BasicWorkflow<'a>) -> Self {
        Self {
            workflow,
            state: SessionState::Ready,
        }
    }

    pub fn supervising_agent(&self) -> &str {
        &self.workflow.session().name
    }

    pub fn state(&self) -> &SessionState {
        &self.state
    }

    /// Submits user input and synchronously runs the MVP workflow to its next
    /// final assistant response.
    pub fn submit(&mut self, input: impl Into<String>) -> Result<&[Message], UiError> {
        self.state = SessionState::Running;

        match self.workflow.run(input) {
            Ok(messages) => {
                self.state = SessionState::Complete;
                Ok(messages)
            }
            Err(error) => {
                let error = UiError::from(error);
                self.state = SessionState::Failed(error.clone());
                Err(error)
            }
        }
    }

    /// Requests cancellation. The agent observes this at the next model-round
    /// boundary.
    pub fn request_cancel(&mut self) {
        self.workflow.session_mut().request_cancel();
        self.state = SessionState::CancellationRequested;
    }

    /// Full ordered conversation, including locally renderable tool messages.
    pub fn messages(&self) -> &[Message] {
        self.workflow.messages()
    }

    /// Messages at and after a frontend-owned cursor. This lets a frontend
    /// render only entries it has not displayed yet.
    pub fn messages_since(&self, cursor: usize) -> &[Message] {
        self.messages().get(cursor..).unwrap_or_default()
    }
}
