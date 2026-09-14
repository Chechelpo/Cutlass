use std::hash::Hash;
use serde::Serialize;
use serde::de::DeserializeOwned;
use crate::chat_completions::tools::{ChatCompletionTool, ToolCall, ToolResult};

    ///
    /// Typed trait tool
    ///
    pub trait Tool {
        type Action: Eq + Hash + Clone;
        type Input: DeserializeOwned;
        type Output: Serialize;
        type Config;

        fn id(&self) -> &str;
        fn name(&self) -> &str;

        fn description(&self) -> &str;
        fn all_actions(&self) -> &[Self::Action];
        fn execute(&self, input: Self::Input) -> Self::Output;

        fn as_chat_completion_tool(&self) -> ChatCompletionTool;
    }
    /// Type erased tool.
    pub trait DynTool {
        fn id(&self) -> &str;
        fn name(&self) -> &str;

        fn description(&self) -> &str;

        fn run(
            &self,
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


        fn run(
            &self,
            call: &ToolCall,
        ) -> ToolResult {
            let input: T::Input =
                serde_json::from_str(
                    &call.function.arguments
                )
                    .expect("Invalid arguments");


            let output = self.execute(input);


            ToolResult {
                tool_call_id: call.id.clone(),
                content: serde_json::to_value(output)
                    .expect("Serialization failed"),
            }
        }


        fn as_chat_completion_tool(&self)
                                   -> ChatCompletionTool
        {
            Tool::as_chat_completion_tool(self)
        }
    }