use crate::agent::names;
pub use crate::agent::presets::agent::Agent;
use crate::agent::sandbox::filesystem::SandboxedFilesystem;
use crate::agent::sandbox::sandbox::{Sandbox, create_sandbox};
use crate::agent::steering::SteeringInbox;
use crate::chat_completions::api::client::{ApiClient, ApiError};
use crate::chat_completions::messages::{AssistantMessage, Message};
use crate::chat_completions::tools::ToolResult;
use crate::config::ModelConfig;
use crate::tools::group::ToolGroup;
use names::get_agent_name;

pub struct AgentSession<'a> {
    pub name: String,
    pub preset: &'a Agent,
    pub model_config: &'a ModelConfig,
    pub chat_history: Vec<Message>,
    pub steering_inbox: SteeringInbox,
    pub sandbox: Box<dyn Sandbox>,
}

impl<'a> AgentSession<'a> {
    pub fn new(
        sandboxed_filesystem: SandboxedFilesystem,
        config: &'a ModelConfig,
        preset: &'a Agent,
    ) -> Self {
        let system_prompt = (preset.system_prompt)(&sandboxed_filesystem);
        AgentSession {
            name: get_agent_name(),
            preset,
            model_config: config,
            chat_history: vec![Message::System {
                content: system_prompt,
            }],
            steering_inbox: SteeringInbox::new(),
            sandbox: create_sandbox(sandboxed_filesystem),
        }
    }
    pub fn tool_groups(&self) -> &[ToolGroup] {
        &self.preset.tool_groups
    }

    pub fn messages(&self) -> &[Message] {
        &self.chat_history
    }

    pub fn add_user_message(&mut self, message: impl Into<String>) {
        self.chat_history.push(Message::User {
            content: message.into(),
        });
    }

    pub fn request_cancel(&mut self) {
        self.steering_inbox.cancel_turn();
    }

    /// Runs model rounds until the assistant produces a final response, the
    /// turn is cancelled, or an API call fails.
    pub fn run(&mut self, user_prompt: String) -> Result<(), ApiError> {
        let client = ApiClient::new();
        self.chat_history.push(Message::User {
            content: user_prompt,
        });

        loop {
            if self.steering_inbox.end_turn_called() {
                self.steering_inbox.acknowledge_end_turn();
                return Ok(());
            }

            if let Some(message) = self.steering_inbox.drain_steering_message() {
                self.chat_history.push(Message::User { content: message });
            }

            let response = self.run_turn(&client)?;
            if response.is_final() {
                self.chat_history.push(Message::from(response));
                return Ok(());
            }

            self.handle_tool_calls(response);
        }
    }

    /// Runs one model round. Retryable failures are retried without turning
    /// tool calls into additional retry attempts.
    fn run_turn(&self, client: &ApiClient) -> Result<AssistantMessage, ApiError> {
        let mut retries_remaining = self.model_config.retry_amount();
        let tools = self
            .tool_groups()
            .iter()
            .flat_map(|group| group.tools())
            .map(|tool| tool.as_chat_completion_tool())
            .collect::<Vec<_>>();

        loop {
            match client.call(
                &self.chat_history,
                self.model_config.id(),
                self.model_config.host_url(),
                self.model_config.decrypted_keys().unwrap().first().unwrap(),
                self.model_config.max_output_tokens(),
                &tools,
            ) {
                Ok(response) => return Ok(response),
                Err(error) if error.is_retryable && retries_remaining > 0 => {
                    retries_remaining -= 1;
                }
                Err(error) => return Err(error),
            }
        }
    }

    fn handle_tool_calls(&mut self, message: AssistantMessage) {
        let tool_results: Vec<_> = message
            .tool_calls()
            .iter()
            .map(|call| {
                self.preset
                    .tool_groups
                    .iter()
                    .find_map(|group| group.execute_owned_call(self, call))
                    .unwrap_or_else(|| {
                        ToolResult::failure(
                            call,
                            format!("No tool found with name '{}'", call.function.name),
                        )
                    })
            })
            .collect();

        self.respond_to_tool_calls(message, tool_results);
    }

    /// Adds a complete tool-call exchange to the next Chat Completions request:
    /// the original assistant message followed by each corresponding tool result.
    pub fn respond_to_tool_calls<I>(&mut self, message: AssistantMessage, tool_results: I)
    where
        I: IntoIterator<Item = ToolResult>,
    {
        debug_assert!(message.has_tool_calls());
        self.chat_history.push(Message::from(message));
        self.chat_history
            .extend(tool_results.into_iter().map(Message::from));
    }
}
