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
pub struct BashInput {
    pub command: String,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum BashAction {
    Run,
}

const ALL_ACTIONS: [BashAction; 1] = [BashAction::Run];

pub struct BashTool {
    actions: Vec<BashAction>,
    deferred: bool,
}

impl BashTool {
    pub fn all_actions(deferred: bool) -> Self {
        Self {
            actions: ALL_ACTIONS.to_vec(),
            deferred,
        }
    }

    pub fn with_actions(actions: impl IntoIterator<Item = BashAction>, deferred: bool) -> Self {
        let actions = actions.into_iter().fold(Vec::new(), |mut unique, action| {
            if !unique.contains(&action) {
                unique.push(action);
            }
            unique
        });

        Self { actions, deferred }
    }

    pub fn except_actions(excluded: impl IntoIterator<Item = BashAction>, deferred: bool) -> Self {
        let excluded = excluded.into_iter().collect::<Vec<_>>();
        let actions = ALL_ACTIONS
            .iter()
            .copied()
            .filter(|action| !excluded.contains(action))
            .collect();

        Self { actions, deferred }
    }
}

impl Tool for BashTool {
    type Action = BashAction;
    type Input = BashInput;
    type Config = ();

    fn id(&self) -> &str {
        "running.bash"
    }

    fn name(&self) -> &str {
        "bash"
    }

    fn description(&self) -> &str {
        "Runs a Bash command in the sandbox and returns its output and exit code."
    }

    fn actions(&self) -> &[Self::Action] {
        &self.actions
    }

    fn deferred(&self) -> bool {
        self.deferred
    }

    fn execute(&self, call: &ToolCall, context: &AgentSession, input: Self::Input) -> ToolResult {
        let args = vec!["-lc".to_string(), input.command.clone()];

        match context.sandbox.execute("/bin/bash", &args) {
            Ok(output) => {
                info!(
                    exit_code = output.exit_code,
                    command = %input.command,
                    "Ran Bash command in sandbox"
                );

                let body = match (output.stdout.is_empty(), output.stderr.is_empty()) {
                    (true, true) => None,
                    (false, true) => Some(output.stdout.clone()),
                    (true, false) => Some(output.stderr.clone()),
                    (false, false) => {
                        Some(format!("{}\n[stderr]\n{}", output.stdout, output.stderr,))
                    }
                };
                let title = format!(
                    "**Ran** Bash command (exit {})",
                    output
                        .exit_code
                        .map_or_else(|| "signal".to_string(), |code| code.to_string()),
                );
                let render = body.into_iter().fold(
                    RenderToolCall::new(RenderText::plain(title)),
                    |render, body| render.with_body(RenderText::plain(body)),
                );

                ToolResult::success(
                    call,
                    json!({
                        "command": input.command,
                        "stdout": output.stdout,
                        "stderr": output.stderr,
                        "exit_code": output.exit_code,
                    }),
                    render,
                )
            }
            Err(err) => {
                error!(command = %input.command, error = %err, "Failed to run Bash command");
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
                        "command": {
                            "type": "string",
                            "description": "The Bash command to run in the sandbox."
                        }
                    },
                    "required": ["command"],
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
    fn stores_actions_and_deferred_state() {
        let tool = BashTool::all_actions(true);

        assert_eq!(tool.actions(), [BashAction::Run]);
        assert!(tool.deferred());
    }

    #[test]
    fn filters_actions_at_instantiation() {
        let tool = BashTool::except_actions([BashAction::Run], false);

        assert!(tool.actions().is_empty());
        assert!(!tool.deferred());
    }

    #[test]
    fn declares_the_command_parameter() {
        let declaration =
            serde_json::to_value(BashTool::all_actions(false).as_chat_completion_tool()).unwrap();

        assert_eq!(declaration["function"]["name"], "bash");
        assert_eq!(
            declaration["function"]["parameters"]["required"],
            json!(["command"]),
        );
    }
}
