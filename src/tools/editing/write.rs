use serde::Deserialize;
use serde_json::json;
use tracing::{error, info};

use crate::agent::agent_session::AgentSession;
use crate::chat_completions::tools::{
    ChatCompletionTool, FunctionDefinition, ToolCall, ToolResult,
};
use crate::tools::tool::Tool;
use crate::ui_interface::chat::{RenderText, RenderToolCall};

#[derive(Debug, Deserialize)]
pub struct CreateFileInput {
    pub path: String,
    pub content: String,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CreateFileAction {
    Create,
}

const ALL_ACTIONS: [CreateFileAction; 1] = [CreateFileAction::Create];

pub struct CreateFileTool {
    actions: Vec<CreateFileAction>,
    deferred: bool,
}

impl CreateFileTool {
    pub fn all_actions(deferred: bool) -> Self {
        Self {
            actions: ALL_ACTIONS.to_vec(),
            deferred,
        }
    }

    pub fn with_actions(
        actions: impl IntoIterator<Item = CreateFileAction>,
        deferred: bool,
    ) -> Self {
        let actions = actions.into_iter().fold(Vec::new(), |mut unique, action| {
            if !unique.contains(&action) {
                unique.push(action);
            }
            unique
        });

        Self {
            actions,
            deferred,
        }
    }

    pub fn except_actions(
        excluded: impl IntoIterator<Item = CreateFileAction>,
        deferred: bool,
    ) -> Self {
        let excluded = excluded.into_iter().collect::<Vec<_>>();

        let actions = ALL_ACTIONS
            .iter()
            .copied()
            .filter(|action| !excluded.contains(action))
            .collect();

        Self {
            actions,
            deferred,
        }
    }
}

impl Tool for CreateFileTool {
    type Action = CreateFileAction;
    type Input = CreateFileInput;
    type Config = ();

    fn id(&self) -> &str {
        "filesystem.create_file"
    }

    fn name(&self) -> &str {
        "create_file"
    }

    fn description(&self) -> &str {
        "Creates a new file in the sandbox filesystem."
    }

    fn actions(&self) -> &[Self::Action] {
        &self.actions
    }

    fn deferred(&self) -> bool {
        self.deferred
    }

    fn execute(
        &self,
        call: &ToolCall,
        context: &AgentSession,
        input: Self::Input,
    ) -> ToolResult {
        let path = std::path::Path::new(&input.path);

        match context
            .sandbox
            .workspace()
            .create_file(path, &input.content)
        {
            Ok(()) => {
                let title = format!(
                    "**Created** file _{}_ ({} bytes)",
                    input.path,
                    input.content.len()
                );

                info!(path = %input.path, call_id = %call.id, bytes = input.content.len(), "created file");

                ToolResult::success(
                    call,
                    json!({
                        "path": input.path,
                        "bytes": input.content.len(),
                    }),
                    RenderToolCall::new(RenderText::plain(title)),
                )
            }

            Err(err) => {
                error!(path = %path.display(), call_id = %call.id, error = %err, "could not create file");

                ToolResult::failure(call, err.to_string())
            }
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
                            "description":
                                "Absolute path where the new file should be created."
                        },
                        "content": {
                            "type": "string",
                            "description":
                                "Initial contents of the created file."
                        }
                    },
                    "required": [
                        "path",
                        "content"
                    ],
                    "additionalProperties": false
                }),
            },
        }
    }
}
