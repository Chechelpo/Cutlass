use crate::agent::names;
pub use crate::agent::presets::agent::Agent;
use crate::agent::sandbox::filesystem::SandboxedFilesystem;
use crate::agent::sandbox::sandbox::{Sandbox, create_sandbox};
use crate::agent::steering::SteeringInbox;
use crate::chat_completions::api::client::{ApiClient, ApiError};
use crate::chat_completions::messages::{AssistantMessage, Message};
use crate::chat_completions::tools::ToolResult;
use crate::config::ModelConfig;
use crate::tools::group::{ToolGroup, ToolGroupKind};
use crate::ui_interface::chat::{
    RenderMessageSection, RenderText, RenderToolCall, RenderToolGroup,
};
use names::get_agent_name;

#[derive(Debug)]
pub enum AgentEvent {
    /// A renderable entry in this session's message history.
    Message { message_index: usize },
    /// Results produced by one configured tool group.
    ToolGroup {
        group_index: usize,
        kind: ToolGroupKind,
        render: RenderToolGroup,
    },
    /// Calls that did not belong to any configured tool group.
    UnmatchedToolCalls { render: RenderToolGroup },
}

pub struct AgentSession<'a> {
    pub name: String,
    pub preset: &'a Agent,
    pub model_config: &'a ModelConfig,
    pub chat_history: Vec<Message>,
    events: Vec<AgentEvent>,
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
            events: Vec::new(),
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

    pub fn events(&self) -> &[AgentEvent] {
        &self.events
    }

    pub fn add_user_message(&mut self, message: impl Into<String>) {
        self.chat_history.push(Message::User {
            content: message.into(),
        });
        self.record_latest_message();
    }

    pub fn request_cancel(&mut self) {
        self.steering_inbox.cancel_turn();
    }

    /// Runs model rounds until the assistant produces a final response, the
    /// turn is cancelled, or an API call fails.
    pub fn run(&mut self, user_prompt: String) -> Result<(), ApiError> {
        self.run_with_events(user_prompt, &mut |_| {})
    }

    /// Publishes completed messages and group renders as each model round finishes.
    pub fn run_with_events(
        &mut self,
        user_prompt: String,
        emit: &mut dyn FnMut(RenderMessageSection),
    ) -> Result<(), ApiError> {
        let mut cursor = self.events.len();
        let client = ApiClient::new();
        self.chat_history.push(Message::User {
            content: user_prompt,
        });
        self.record_latest_message();
        self.emit_since(&mut cursor, emit);

        loop {
            if self.steering_inbox.end_turn_called() {
                self.steering_inbox.acknowledge_end_turn();
                return Ok(());
            }

            if let Some(message) = self.steering_inbox.drain_steering_message() {
                self.chat_history.push(Message::User { content: message });
                self.record_latest_message();
                self.emit_since(&mut cursor, emit);
            }

            let response = self.run_turn(&client)?;
            if response.is_final() {
                self.chat_history.push(Message::from(response));
                self.record_latest_message();
                self.emit_since(&mut cursor, emit);
                return Ok(());
            }

            self.handle_tool_calls(response);
            self.emit_since(&mut cursor, emit);
        }
    }

    /// Runs one model round. Retryable failures are retried without turning
    /// tool calls into additional retry attempts.
    fn run_turn(&self, client: &ApiClient) -> Result<AssistantMessage, ApiError> {
        let keys = self
            .model_config
            .decrypted_keys()
            .map_err(|error| ApiError {
                status: 0,
                is_retryable: false,
                message: error.to_string(),
            })?;
        let key = keys.first().ok_or_else(|| ApiError {
            status: 0,
            is_retryable: false,
            message: "The selected connection has no API key".into(),
        })?;
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
                key,
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
        let mut tool_results = Vec::with_capacity(message.tool_calls().len());
        let mut grouped_results: Vec<Vec<RenderToolCall>> =
            self.preset.tool_groups.iter().map(|_| Vec::new()).collect();
        let mut unmatched_results = Vec::new();

        // Execute in the order requested by the model. Group only the render
        // records, so presentation concerns never reorder tool side effects or
        // the provider-facing result messages.
        for call in message.tool_calls() {
            let handled =
                self.preset
                    .tool_groups
                    .iter()
                    .enumerate()
                    .find_map(|(group_index, group)| {
                        group
                            .execute_owned_call(self, call)
                            .map(|result| (group_index, result))
                    });

            let result = match handled {
                Some((group_index, result)) => {
                    grouped_results[group_index].push(result.render.clone());
                    result
                }
                None => {
                    let result = ToolResult::failure(
                        call,
                        format!("No tool found with name '{}'", call.function.name),
                    );
                    unmatched_results.push(result.render.clone());
                    result
                }
            };

            tool_results.push(result);
        }

        let mut group_events = grouped_results
            .into_iter()
            .enumerate()
            .filter(|(_, results)| !results.is_empty())
            .map(|(group_index, calls)| AgentEvent::ToolGroup {
                group_index,
                kind: self.preset.tool_groups[group_index].kind(),
                render: self.preset.tool_groups[group_index].render(calls),
            })
            .collect::<Vec<_>>();

        if !unmatched_results.is_empty() {
            group_events.push(AgentEvent::UnmatchedToolCalls {
                render: RenderToolGroup::new(
                    RenderText::plain("Unmatched tool calls"),
                    unmatched_results,
                ),
            });
        }

        self.respond_to_tool_calls(message, tool_results);
        self.events.extend(group_events);
    }

    /// Adds a complete tool-call exchange to the next Chat Completions request:
    /// the original assistant message followed by each corresponding tool result.
    fn respond_to_tool_calls<I>(&mut self, message: AssistantMessage, tool_results: I)
    where
        I: IntoIterator<Item = ToolResult>,
    {
        debug_assert!(message.has_tool_calls());
        self.chat_history.push(Message::from(message));
        self.record_latest_message();
        self.chat_history
            .extend(tool_results.into_iter().map(Message::from));
    }

    fn record_latest_message(&mut self) {
        self.events.push(AgentEvent::Message {
            message_index: self.chat_history.len() - 1,
        });
    }

    fn emit_since(&self, cursor: &mut usize, emit: &mut dyn FnMut(RenderMessageSection)) {
        for event in &self.events[*cursor..] {
            let section = match event {
                AgentEvent::Message { message_index } => match &self.chat_history[*message_index] {
                    Message::User { content } => RenderMessageSection::Message {
                        speaker: "You".into(),
                        content: RenderText::plain(content),
                    },
                    Message::Assistant {
                        content: Some(content),
                        ..
                    } if !content.is_empty() => RenderMessageSection::Message {
                        speaker: self.preset.name.clone(),
                        content: RenderText::markdown(content),
                    },
                    _ => continue,
                },
                AgentEvent::ToolGroup { render, .. }
                | AgentEvent::UnmatchedToolCalls { render } => {
                    RenderMessageSection::ToolGroup(render.clone())
                }
            };
            emit(section);
        }
        *cursor = self.events.len();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat_completions::tools::{ChatCompletionTool, FunctionDefinition, ToolCall};
    use crate::tools::tool::Tool;
    use serde_json::{Value, json};
    use std::cell::RefCell;
    use std::fs;
    use std::path::PathBuf;
    use std::rc::Rc;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
    enum MockAction {
        Run,
    }

    const MOCK_ACTIONS: [MockAction; 1] = [MockAction::Run];

    struct MockTool {
        name: &'static str,
        deferred: bool,
        executions: Rc<RefCell<Vec<&'static str>>>,
    }

    impl Tool for MockTool {
        type Action = MockAction;
        type Input = Value;
        type Config = ();

        fn id(&self) -> &str {
            self.name
        }

        fn name(&self) -> &str {
            self.name
        }

        fn description(&self) -> &str {
            "mock tool"
        }

        fn actions(&self) -> &[Self::Action] {
            &MOCK_ACTIONS
        }

        fn deferred(&self) -> bool {
            self.deferred
        }

        fn execute(
            &self,
            call: &ToolCall,
            _context: &AgentSession,
            _input: Self::Input,
        ) -> ToolResult {
            self.executions.borrow_mut().push(self.name);
            ToolResult::success(
                call,
                json!({"tool": self.name}),
                RenderToolCall::new(RenderText::plain(self.name))
                    .with_body(RenderText::markdown(format!("Ran **{}**", self.name))),
            )
        }

        fn as_chat_completion_tool(&self) -> ChatCompletionTool {
            ChatCompletionTool {
                tool_type: "function",
                function: FunctionDefinition {
                    name: self.name.into(),
                    description: self.description().into(),
                    parameters: json!({"type": "object"}),
                },
            }
        }
    }

    fn temporary_directory(test_name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "cutlass-agent-session-{test_name}-{}-{unique}",
            std::process::id(),
        ));
        fs::create_dir_all(&directory).unwrap();
        directory
    }

    fn model_config(directory: &std::path::Path) -> ModelConfig {
        ModelConfig::with_master_key_file(
            "test",
            "https://example.invalid",
            "test-model",
            1_024,
            128,
            0,
            Duration::ZERO,
            directory.join("master.key"),
        )
        .unwrap()
    }

    fn agent(tools: Vec<Box<dyn crate::tools::tool::DynTool>>) -> Agent {
        Agent::new(
            "Test agent".into(),
            "Agent-session test fixture".into(),
            |_| "test system prompt".into(),
            tools,
            Vec::new(),
        )
    }

    fn tool_response(names: &[&str]) -> AssistantMessage {
        let tool_calls = names
            .iter()
            .enumerate()
            .map(|(index, name)| {
                json!({
                    "id": format!("call-{index}"),
                    "type": "function",
                    "function": {"name": name, "arguments": "{}"},
                })
            })
            .collect::<Vec<_>>();

        serde_json::from_value(json!({
            "role": "assistant",
            "content": null,
            "tool_calls": tool_calls,
        }))
        .unwrap()
    }

    fn session<'a>(config: &'a ModelConfig, agent: &'a Agent) -> AgentSession<'a> {
        AgentSession::new(
            SandboxedFilesystem::new(Vec::new(), Vec::new()),
            config,
            agent,
        )
    }

    #[test]
    fn records_renderable_messages_by_history_index() {
        let directory = temporary_directory("message-event");
        let config = model_config(&directory);
        let agent = agent(Vec::new());
        let mut session = session(&config, &agent);

        session.add_user_message("hello");

        assert!(matches!(
            session.events(),
            [AgentEvent::Message { message_index: 1 }]
        ));
        assert!(matches!(
            &session.messages()[1],
            Message::User { content } if content == "hello"
        ));

        drop(session);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn publishes_new_sections_once_and_hides_system_and_empty_assistant_messages() {
        let directory = temporary_directory("published-events");
        let config = model_config(&directory);
        let agent = agent(Vec::new());
        let mut session = session(&config, &agent);
        session.add_user_message("**literal**");
        session.handle_tool_calls(tool_response(&["missing"]));
        let mut cursor = 0;
        let mut sections = Vec::new();
        session.emit_since(&mut cursor, &mut |section| sections.push(section));
        session.emit_since(&mut cursor, &mut |section| sections.push(section));
        assert_eq!(sections.len(), 2);
        assert!(
            matches!(&sections[0], RenderMessageSection::Message { speaker, content: RenderText::Plain(content) } if speaker == "You" && content == "**literal**")
        );
        assert!(matches!(&sections[1], RenderMessageSection::ToolGroup(_)));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn renders_results_by_exact_tool_group_without_reordering_execution() {
        let directory = temporary_directory("grouped-results");
        let config = model_config(&directory);
        let executions = Rc::new(RefCell::new(Vec::new()));
        let mut agent = agent(vec![
            Box::new(MockTool {
                name: "first",
                deferred: false,
                executions: executions.clone(),
            }),
            Box::new(MockTool {
                name: "later",
                deferred: true,
                executions: executions.clone(),
            }),
            Box::new(MockTool {
                name: "second",
                deferred: false,
                executions: executions.clone(),
            }),
        ]);
        agent.tool_groups[0].set_renderer(|calls| {
            RenderToolGroup::new(RenderText::markdown("### Immediate calls"), calls)
        });
        let mut session = session(&config, &agent);

        session.handle_tool_calls(tool_response(&["first", "later", "second"]));

        assert_eq!(&*executions.borrow(), &["first", "later", "second"]);
        assert_eq!(session.events().len(), 3);
        assert!(matches!(
            &session.events()[0],
            AgentEvent::Message { message_index: 1 }
        ));

        let AgentEvent::ToolGroup {
            group_index,
            kind,
            render,
        } = &session.events()[1]
        else {
            panic!("expected immediate tool group");
        };
        assert_eq!(*group_index, 0);
        assert_eq!(*kind, ToolGroupKind::Immediate);
        assert_eq!(render.header, RenderText::markdown("### Immediate calls"));
        assert_eq!(
            render
                .calls
                .iter()
                .map(|call| call.title.as_str())
                .collect::<Vec<_>>(),
            ["first", "second"],
        );

        let AgentEvent::ToolGroup { kind, render, .. } = &session.events()[2] else {
            panic!("expected deferred tool group");
        };
        assert_eq!(*kind, ToolGroupKind::Deferred);
        assert_eq!(
            render
                .calls
                .iter()
                .map(|call| call.title.as_str())
                .collect::<Vec<_>>(),
            ["later"],
        );

        let result_ids = session.messages()[2..]
            .iter()
            .map(|message| match message {
                Message::Tool { tool_call_id, .. } => tool_call_id.as_str(),
                _ => panic!("expected tool result message"),
            })
            .collect::<Vec<_>>();
        assert_eq!(result_ids, ["call-0", "call-1", "call-2"]);

        drop(session);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn renders_unmatched_calls_as_one_fallback_group() {
        let directory = temporary_directory("unmatched-results");
        let config = model_config(&directory);
        let agent = agent(Vec::new());
        let mut session = session(&config, &agent);

        session.handle_tool_calls(tool_response(&["missing-one", "missing-two"]));

        assert_eq!(session.events().len(), 2);
        let AgentEvent::UnmatchedToolCalls { render } = &session.events()[1] else {
            panic!("expected unmatched tool-call group");
        };
        assert_eq!(render.header, RenderText::plain("Unmatched tool calls"));
        assert_eq!(render.calls.len(), 2);
        assert!(render.calls[0].title.as_str().contains("call-0"));
        assert!(render.calls[1].title.as_str().contains("call-1"));

        drop(session);
        fs::remove_dir_all(directory).unwrap();
    }
}
