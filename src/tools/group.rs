use crate::agent::agent::Agent;
use crate::chat_completions::tools::{ToolCall, ToolResult};
use crate::tools::tool::{DynTool, Tool};

pub struct ToolGroup{
    tools:Vec<Box<dyn DynTool>>
}

impl ToolGroup {
    pub fn owns<T:Tool>(
        &self,
        tool: &T
    ) -> bool
    {
        self.tools
            .iter()
            .any(|other| other.id() == tool.id())
    }

    pub fn tools(&self) -> &Vec<Box<dyn DynTool>> {
        &self.tools
    }

    pub fn execute_calls(
        &self,
        agent: &Agent,
        tool_calls: Vec<ToolCall>,
    ) -> Vec<ToolResult> {
        tool_calls
            .iter()
            .map(|call| {
                self.tools
                    .iter()
                    .find(|tool| {
                        tool.name() == call.function.name
                    })
                    .map(|tool| tool.run(agent, call))
                    .unwrap_or_else(|| {
                        ToolResult::failure(
                            call,
                            &format!("No such tool: {}", call.function.name),
                        )
                    })
            })
            .collect()
    }
}