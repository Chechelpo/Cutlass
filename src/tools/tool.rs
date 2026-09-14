use std::hash::Hash;

use serde::Serialize;
use serde::de::DeserializeOwned;
use crate::agent::agent::Agent;
use crate::chat_completions::tools::{
    ChatCompletionTool,
    ToolCall,
    ToolResult,
};

///
/// Typed tool definition.
///
pub trait Tool {
    type Action: Eq + Hash + Clone;
    type Input: DeserializeOwned;
    type Output: Serialize;
    type Config;

    fn id(&self) -> &str;
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn deferred(&self) -> bool;
    fn all_actions(&self) -> &[Self::Action];

    fn execute(
        &self,
        context: &Agent,
        input: Self::Input,
    ) -> Self::Output;

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

    fn run(
        &self,
        context: &Agent,
        call: &ToolCall,
    ) -> ToolResult;

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

    fn run(
        &self,
        context: &Agent,
        call: &ToolCall,
    ) -> ToolResult {
        let input: T::Input =
            match serde_json::from_str(
                &call.function.arguments,
            ) {
                Ok(value) => value,

                Err(err) => {
                    return ToolResult::failure(
                        call,
                        &format!(
                            "Invalid arguments: {}",
                            err
                        ),
                    );
                }
            };


        let output =
            self.execute(
                context,
                input,
            );


        ToolResult {
            tool_call_id: call.id.clone(),

            content: serde_json::to_value(output)
                .expect("Tool output serialization failed"),
        }
    }


    fn as_chat_completion_tool(
        &self,
    ) -> ChatCompletionTool {
        Tool::as_chat_completion_tool(self)
    }
}