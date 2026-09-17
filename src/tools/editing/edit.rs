use std::path::Path;

use serde::Deserialize;
use serde_json::json;
use similar::TextDiff;
use tracing::{error, info, warn};

use crate::agent::agent_session::AgentSession;
use crate::chat_completions::tools::{
    ChatCompletionTool, FunctionDefinition, ToolCall, ToolResult,
};
use crate::tools::tool::Tool;
use crate::ui_interface::chat::{RenderText, RenderToolCall};

#[derive(Debug, Deserialize)]
pub struct EditFileInput {
    /// Absolute guest path inside the sandbox.
    pub path: String,

    /// Exact text that must already exist in the file.
    pub old_string: String,

    /// Replacement text.
    pub new_string: String,

    /// Replace every occurrence instead of requiring a unique occurrence.
    #[serde(default)]
    pub replace_all: bool,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum EditFileAction {
    Edit,
}

const ALL_ACTIONS: [EditFileAction; 1] = [EditFileAction::Edit];

fn unified_edit_diff(path: &str, before: &str, after: &str) -> String {
    TextDiff::from_lines(before, after)
        .unified_diff()
        .header(&format!("a/{path}"), &format!("b/{path}"))
        .to_string()
        .trim_end()
        .to_string()
}

pub struct EditFileTool {
    actions: Vec<EditFileAction>,
    deferred: bool,
}

impl EditFileTool {
    /// Instantiate the tool with every supported action.
    pub fn all_actions(deferred: bool) -> Self {
        Self {
            actions: ALL_ACTIONS.to_vec(),
            deferred,
        }
    }

    /// Instantiate the tool with an explicit action subset.
    pub fn with_actions(
        actions: impl IntoIterator<Item = EditFileAction>,
        deferred: bool,
    ) -> Self {
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
        excluded: impl IntoIterator<Item = EditFileAction>,
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

impl Tool for EditFileTool {
    type Action = EditFileAction;
    type Input = EditFileInput;
    type Config = ();

    fn id(&self) -> &str {
        "filesystem.edit_file"
    }

    fn name(&self) -> &str {
        "edit_file"
    }

    fn description(&self) -> &str {
        "Edits an existing UTF-8 file in the sandbox by replacing exact text."
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
        if input.old_string.is_empty() {
            warn!(path = %input.path, call_id = %call.id, "rejected edit with empty search text");
            return ToolResult::failure(
                call,
                "old_string must not be empty".to_string(),
            );
        }

        let path = Path::new(&input.path);
        let workspace = context.sandbox.workspace();

        let content = match workspace.read_file(path) {
            Ok(content) => content,

            Err(err) => {
                error!(
                    path = %path.display(),
                    error = %err,
                    "Error reading file before edit"
                );

                return ToolResult::failure(call, err.to_string());
            }
        };

        let old_string = input.old_string.as_str();
        let occurrences = content.matches(old_string).count();

        if occurrences == 0 {
            warn!(path = %path.display(), call_id = %call.id, old_string_chars = old_string.chars().count(), "edit search text was not found");
            return ToolResult::failure(
                call,
                format!(
                    "old_string was not found in file {}",
                    path.display()
                ),
            );
        }

        if occurrences > 1 && !input.replace_all {
            warn!(path = %path.display(), call_id = %call.id, occurrences, "rejected ambiguous edit without replace_all");
            return ToolResult::failure(
                call,
                format!(
                    "old_string occurs {} times in {}; \
                     provide a more specific old_string or set replace_all to true",
                    occurrences,
                    path.display(),
                ),
            );
        }

        let replacements = if input.replace_all {
            occurrences
        } else {
            1
        };

        let updated = if input.replace_all {
            content.replace(old_string, input.new_string.as_str())
        } else {
            content.replacen(
                old_string,
                input.new_string.as_str(),
                1,
            )
        };

        let diff = unified_edit_diff(&input.path, &content, &updated);

        match workspace.write_file(path, &updated) {
            Ok(()) => {
                let plural = if replacements == 1 { "" } else { "s" };

                let title = format!(
                    "**Edited** file _{}_ ({} replacement{})",
                    input.path,
                    replacements,
                    plural,
                );

                info!(
                    path = %input.path,
                    replacements,
                    "Edited file"
                );

                ToolResult::success(
                    call,
                    json!({
                        "path": input.path,
                        "replacements": replacements,
                    }),
                    RenderToolCall::new(RenderText::markdown(title))
                        .with_body(RenderText::plain(diff)),
                )
            }

            Err(err) => {
                error!(
                    path = %path.display(),
                    error = %err,
                    "Error writing edited file"
                );

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
                                "Absolute path to the existing file in the sandbox filesystem."
                        },
                        "old_string": {
                            "type": "string",
                            "description":
                                "Exact text to replace. Unless replace_all is true, it must occur exactly once."
                        },
                        "new_string": {
                            "type": "string",
                            "description":
                                "Text that replaces old_string. May be empty to delete the matched text."
                        },
                        "replace_all": {
                            "type": "boolean",
                            "description":
                                "Replace every occurrence of old_string. Defaults to false.",
                            "default": false
                        }
                    },
                    "required": [
                        "path",
                        "old_string",
                        "new_string"
                    ],
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
        let tool = EditFileTool::all_actions(true);

        assert_eq!(tool.actions(), [EditFileAction::Edit]);
        assert!(tool.deferred());
    }

    #[test]
    fn stores_the_action_set_and_deferred_state_supplied_at_instantiation() {
        let tool = EditFileTool::with_actions([], false);

        assert!(tool.actions().is_empty());
        assert!(!tool.deferred());
    }

    #[test]
    fn instantiates_with_all_actions_except_the_excluded_set() {
        let tool =
            EditFileTool::except_actions([EditFileAction::Edit], true);

        assert!(tool.actions().is_empty());
        assert!(tool.deferred());
    }

    #[test]
    fn declares_edit_parameters() {
        let declaration =
            serde_json::to_value(
                EditFileTool::all_actions(false)
                    .as_chat_completion_tool(),
            )
                .unwrap();

        assert_eq!(declaration["type"], "function");
        assert_eq!(
            declaration["function"]["name"],
            "edit_file"
        );

        assert_eq!(
            declaration["function"]["parameters"]["required"],
            json!([
                "path",
                "old_string",
                "new_string"
            ]),
        );

        assert_eq!(
            declaration["function"]["parameters"]
                ["properties"]["replace_all"]["default"],
            json!(false),
        );
    }

    #[test]
    fn replace_all_defaults_to_false() {
        let input: EditFileInput = serde_json::from_value(json!({
            "path": "/workspace/file.txt",
            "old_string": "before",
            "new_string": "after"
        }))
            .unwrap();

        assert!(!input.replace_all);
    }

    #[test]
    fn renders_a_contextual_unified_diff() {
        let before = "one\ntwo\nthree\nfour\nfive\nsix\nseven\n";
        let after = "one\ntwo\nthree\nFOUR\nfive\nsix\nseven\n";

        assert_eq!(
            unified_edit_diff("src/file.txt", before, after),
            concat!(
                "--- a/src/file.txt\n",
                "+++ b/src/file.txt\n",
                "@@ -1,7 +1,7 @@\n",
                " one\n",
                " two\n",
                " three\n",
                "-four\n",
                "+FOUR\n",
                " five\n",
                " six\n",
                " seven",
            ),
        );
    }

    #[test]
    fn renders_separate_hunks_for_distant_edits() {
        let before = (1..=12)
            .map(|line| format!("line {line}\n"))
            .collect::<String>();
        let after = before
            .replace("line 2\n", "changed 2\n")
            .replace("line 11\n", "changed 11\n");
        let diff = unified_edit_diff("file.txt", &before, &after);

        assert_eq!(diff.matches("@@").count(), 4);
        assert!(diff.contains("-line 2\n+changed 2"));
        assert!(diff.contains("-line 11\n+changed 11"));
    }
}
