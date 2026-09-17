use serde::Deserialize;
use serde_json::json;
use std::path::Path;

use crate::agent::agent_session::AgentSession;
use crate::chat_completions::tools::{
    ChatCompletionTool,
    FunctionDefinition,
    ToolCall,
    ToolResult,
};
use crate::utils::directory_tree::render_tree;
use crate::tools::tool::Tool;
use crate::ui_interface::chat::{
    RenderText,
    RenderToolCall,
};

#[derive(Debug, Deserialize)]
pub struct TreeInput {
    #[serde(default = "default_path")]
    pub path: String,

    #[serde(default = "default_depth")]
    pub max_depth: usize,

    #[serde(default)]
    pub directories_only: bool,

    #[serde(default)]
    pub hidden: bool,

    #[serde(default = "default_limit")]
    pub limit: usize,

    #[serde(default)]
    pub skip: Vec<String>,

    #[serde(default = "default_true")]
    pub use_default_skips: bool,
}


fn default_path() -> String {
    ".".to_string()
}


fn default_depth() -> usize {
    3
}


fn default_limit() -> usize {
    200
}


fn default_true() -> bool {
    true
}



#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum TreeAction {
    Render,
}


const ALL_ACTIONS: [TreeAction; 1] = [
    TreeAction::Render,
];



pub struct TreeTool {
    actions: Vec<TreeAction>,
    deferred: bool,
}


impl TreeTool {

    pub fn all_actions(deferred: bool) -> Self {
        Self {
            actions: ALL_ACTIONS.to_vec(),
            deferred,
        }
    }


    pub fn with_actions(
        actions: impl IntoIterator<Item = TreeAction>,
        deferred: bool,
    ) -> Self {

        let actions = actions.into_iter()
            .fold(Vec::new(), |mut unique, action| {

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
        excluded: impl IntoIterator<Item = TreeAction>,
        deferred: bool,
    ) -> Self {

        let excluded = excluded.into_iter()
            .collect::<Vec<_>>();

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



impl Tool for TreeTool {

    type Action = TreeAction;
    type Input = TreeInput;
    type Config = ();



    fn id(&self) -> &str {
        "filesystem.tree"
    }


    fn name(&self) -> &str {
        "tree"
    }


    fn description(&self) -> &str {
        "Renders a bounded deterministic directory tree from the sandbox filesystem."
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

        let guest_path = Path::new(&input.path);


        let host_path = match context
            .sandbox
            .workspace()
            .resolve_read_path(guest_path)
        {
            Ok(path) => path,

            Err(error) => {
                return ToolResult::failure(
                    call,
                    error.to_string(),
                );
            }
        };


        match render_tree(
            host_path,
            input.max_depth,
            input.directories_only,
            &input.skip,
            input.hidden,
            input.limit,
            input.use_default_skips,
        ) {

            Ok(tree) => {

                ToolResult::success(
                    call,

                    json!({
                        "path": input.path,
                        "tree": tree,
                    }),

                    RenderToolCall::new(
                        RenderText::plain(
                            format!(
                                "Rendered tree for {}",
                                input.path
                            ),
                        ),
                    ),
                )
            }


            Err(error) => {

                ToolResult::failure(
                    call,
                    error.to_string(),
                )
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
                                "Directory path inside the sandbox filesystem."
                        },

                        "max_depth": {
                            "type": "integer",
                            "description":
                                "Maximum recursion depth.",
                            "default": 3
                        },

                        "directories_only": {
                            "type": "boolean",
                            "description":
                                "Only include directories.",
                            "default": false
                        },

                        "hidden": {
                            "type": "boolean",
                            "description":
                                "Include hidden files and directories.",
                            "default": false
                        },

                        "limit": {
                            "type": "integer",
                            "description":
                                "Maximum number of emitted entries.",
                            "default": 200
                        },

                        "skip": {
                            "type": "array",
                            "items": {
                                "type": "string"
                            },
                            "description":
                                "Glob patterns to ignore."
                        },

                        "use_default_skips": {
                            "type": "boolean",
                            "description":
                                "Enable default ignored directories.",
                            "default": true
                        }
                    },

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

        let tool = TreeTool::all_actions(true);

        assert_eq!(
            tool.actions(),
            [TreeAction::Render],
        );

        assert!(tool.deferred());
    }


    #[test]
    fn stores_explicit_actions_and_deferred_state() {

        let tool = TreeTool::with_actions(
            [],
            false,
        );

        assert!(tool.actions().is_empty());
        assert!(!tool.deferred());
    }


    #[test]
    fn excludes_actions() {

        let tool = TreeTool::except_actions(
            [TreeAction::Render],
            true,
        );

        assert!(tool.actions().is_empty());
        assert!(tool.deferred());
    }


    #[test]
    fn declares_tree_parameter_schema() {

        let declaration =
            serde_json::to_value(
                TreeTool::all_actions(false)
                    .as_chat_completion_tool(),
            )
                .unwrap();


        assert_eq!(
            declaration["function"]["name"],
            "tree",
        );


        assert_eq!(
            declaration["function"]["parameters"]["properties"]["path"]["type"],
            "string",
        );
    }
}