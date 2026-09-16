use serde::Deserialize;
use serde_json::json;

use crate::agent::agent_session::AgentSession;
use crate::chat_completions::tools::{
    ChatCompletionTool, FunctionDefinition, ToolCall, ToolResult,
};
use crate::tools::tool::Tool;
use crate::ui_interface::chat::{RenderText, RenderToolCall};

#[derive(Debug, Deserialize)]
pub struct ReadFileInput {
    pub path: String,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ReadFileAction {
    Read,
}

const ALL_ACTIONS: [ReadFileAction; 1] = [ReadFileAction::Read];

pub struct ReadFileTool {
    actions: Vec<ReadFileAction>,
    deferred: bool,
}

impl ReadFileTool {
    /// Instantiate the tool with every supported action.
    pub fn all_actions(deferred: bool) -> Self {
        Self {
            actions: ALL_ACTIONS.to_vec(),
            deferred,
        }
    }

    /// Instantiate the tool with an explicit action subset.
    pub fn with_actions(actions: impl IntoIterator<Item = ReadFileAction>, deferred: bool) -> Self {
        let actions = actions.into_iter().fold(Vec::new(), |mut unique, action| {
            if !unique.contains(&action) {
                unique.push(action);
            }
            unique
        });
        Self { actions, deferred }
    }

    /// Instantiate the tool with every action except the supplied subset.
    pub fn except_actions(
        excluded: impl IntoIterator<Item = ReadFileAction>,
        deferred: bool,
    ) -> Self {
        let excluded = excluded.into_iter().collect::<Vec<_>>();
        let actions = ALL_ACTIONS
            .iter()
            .copied()
            .filter(|action| !excluded.contains(action))
            .collect();
        Self { actions, deferred }
    }
}

impl Tool for ReadFileTool {
    type Action = ReadFileAction;
    type Input = ReadFileInput;
    type Config = ();

    fn id(&self) -> &str {
        "filesystem.read_file"
    }

    fn name(&self) -> &str {
        "read_file"
    }

    fn description(&self) -> &str {
        "Reads a file from the sandbox filesystem."
    }

    fn actions(&self) -> &[Self::Action] {
        &self.actions
    }

    fn deferred(&self) -> bool {
        self.deferred
    }

    fn execute(&self, call: &ToolCall, context: &AgentSession, input: Self::Input) -> ToolResult {
        let path = std::path::Path::new(&input.path);

        match context.sandbox.workspace().read_file(path) {
            Ok(content) => {
                let line_count = content.lines().count();
                let title = format!("Read file {} ({} lines)", input.path, line_count);
                ToolResult::success(
                    call,
                    json!({
                        "path": input.path,
                        "content": content,
                    }),
                    RenderToolCall::new(RenderText::plain(title)),
                )
            }
            Err(err) => ToolResult::failure(call, err.to_string()),
        }
    }

    fn as_chat_completion_tool(&self) -> ChatCompletionTool {
        ChatCompletionTool {
            tool_type: "function",
            function: FunctionDefinition {
                name: self.name().to_string(),
                description: self.description().to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Absolute path to the file in the sandbox filesystem."
                        }
                    },
                    "required": ["path"],
                    "additionalProperties": false
                }),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stores_all_actions_and_deferred_state() {
        let tool = ReadFileTool::all_actions(true);

        assert_eq!(tool.actions(), [ReadFileAction::Read]);
        assert!(tool.deferred());
    }

    #[test]
    fn stores_the_action_set_and_deferred_state_supplied_at_instantiation() {
        let tool = ReadFileTool::with_actions([], false);

        assert!(tool.actions().is_empty());
        assert!(!tool.deferred());
    }

    #[test]
    fn instantiates_with_all_actions_except_the_excluded_set() {
        let tool = ReadFileTool::except_actions([ReadFileAction::Read], true);

        assert!(tool.actions().is_empty());
        assert!(tool.deferred());
    }

    #[test]
    fn declares_the_path_parameter() {
        let declaration =
            serde_json::to_value(ReadFileTool::all_actions(false).as_chat_completion_tool())
                .unwrap();

        assert_eq!(declaration["type"], "function");
        assert_eq!(declaration["function"]["name"], "read_file");
        assert_eq!(
            declaration["function"]["parameters"]["required"],
            json!(["path"]),
        );
    }
}
