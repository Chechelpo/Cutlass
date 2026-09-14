use std::hash::Hash;

use crate::agent::agent_session::AgentSession;
use crate::chat_completions::tools::{ChatCompletionTool, ToolCall, ToolResult};
use serde::de::DeserializeOwned;

///
/// Typed tool definition.
///
pub trait Tool {
    type Action: Eq + Hash + Clone;
    type Input: DeserializeOwned;
    type Config;

    fn id(&self) -> &str;
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn actions(&self) -> &[Self::Action];
    fn deferred(&self) -> bool;

    fn execute(&self, call: &ToolCall, context: &AgentSession, input: Self::Input) -> ToolResult;

    fn as_chat_completion_tool(&self) -> ChatCompletionTool;
}

///
/// Type-erased tool used by registries/presets.
///
pub trait DynTool {
    fn id(&self) -> &str;
    fn name(&self) -> &str;
    fn description(&self) -> &str;

    fn deferred(&self) -> bool;

    fn run(&self, context: &AgentSession, call: &ToolCall) -> ToolResult;

    fn as_chat_completion_tool(&self) -> ChatCompletionTool;
}

impl<T> DynTool for T
where
    T: Tool,
{
    fn id(&self) -> &str {
        Tool::id(self)
    }

    fn name(&self) -> &str {
        Tool::name(self)
    }

    fn description(&self) -> &str {
        Tool::description(self)
    }

    fn deferred(&self) -> bool {
        Tool::deferred(self)
    }

    fn run(&self, context: &AgentSession, call: &ToolCall) -> ToolResult {
        let input: T::Input = match serde_json::from_str(&call.function.arguments) {
            Ok(value) => value,

            Err(err) => {
                return ToolResult::failure(call, format!("Invalid arguments: {}", err));
            }
        };

        self.execute(call, context, input)
    }

    fn as_chat_completion_tool(&self) -> ChatCompletionTool {
        Tool::as_chat_completion_tool(self)
    }
}
