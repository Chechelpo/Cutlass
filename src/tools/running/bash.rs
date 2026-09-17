//! Bash sandbox tool.
//!
//! **Module:** `tools::running::bash`
//! **Created:** before 2026-09-17 (original author unrecorded)
//! **Author (latest edit): ** Claude
//! **Modification history:**
//! - 2026-09-17 · Codex · Render Bash calls as Markdown and raise the output
//!   limit to a generous character-based cap.
//!
//! **Synopsis:** Runs a Bash command inside the agent sandbox and returns its
//! exit code plus captured output, capped only to keep exceptionally large
//! commands from flooding the conversation.
//!
//! **Globals:** none accessed or modified (module-private constants only).

use serde::Deserialize;
use serde_json::json;
use tracing::{error, info, warn};

use crate::agent::agent_session::AgentSession;
use crate::chat_completions::tools::{
    ChatCompletionTool, FunctionDefinition, ToolCall, ToolResult,
};
use crate::tools::tool::Tool;
use crate::ui_interface::chat::{RenderText, RenderToolCall};

/// Maximum number of characters returned from each output stream.
const MAX_OUTPUT_CHARS: usize = 100_000;
/// Number of lines shown per stream in the compact rendered call.
const MAX_DISPLAYED_LINES: usize = 2;

fn preview_output(stream: &str, label: &str) -> Option<String> {
    if stream.trim().is_empty() {
        return None;
    }

    let total_lines = stream.lines().count();
    if total_lines <= MAX_DISPLAYED_LINES {
        return Some(stream.to_string());
    }

    let head = stream
        .lines()
        .take(MAX_DISPLAYED_LINES)
        .collect::<Vec<_>>()
        .join("\n");
    let hidden_lines = total_lines - MAX_DISPLAYED_LINES;
    Some(format!("{head}\n… +{hidden_lines} {label} lines"))
}

/// Condenses a captured output stream for display and model consumption.
///
/// # Parameters
/// - `stream`: raw captured output (stdout or stderr). May be empty.
/// - `label`: stream name used in the truncation marker (e.g. `"stdout"`).
///
/// # Return
/// The stream verbatim when it fits, otherwise its first `MAX_OUTPUT_CHARS`
/// characters followed by a truncation marker.
///
/// # Exceptions
/// None; never panics for any input string.
fn cap_output(stream: &str, label: &str) -> String {
    let mut chars = stream.chars();
    let head = chars.by_ref().take(MAX_OUTPUT_CHARS).collect::<String>();
    if chars.next().is_none() {
        return head;
    }

    format!("{head}\n[... {label} truncated after {MAX_OUTPUT_CHARS} characters]")
}

#[derive(Debug, Deserialize)]
pub struct BashInput {
    pub command: String,
    #[serde(default)]
    pub write: bool,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum BashAction {
    Run,
    Write,
}

const ALL_ACTIONS: [BashAction; 2] = [BashAction::Run, BashAction::Write];

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
        "Runs a Bash command in the sandbox and returns its output and exit code. Output is capped at 100,000 characters per stream."
    }

    fn actions(&self) -> &[Self::Action] {
        &self.actions
    }

    fn deferred(&self) -> bool {
        self.deferred
    }

    fn execute(&self, call: &ToolCall, context: &AgentSession, input: Self::Input) -> ToolResult {
        let args = vec!["-lc".to_string(), input.command.clone()];
        let writable = input.write && self.actions.contains(&BashAction::Write);

        match context.sandbox.execute("/bin/bash", &args, writable) {
            Ok(output) => {
                let stdout_chars = output.stdout.chars().count();
                let stderr_chars = output.stderr.chars().count();
                let truncated = stdout_chars > MAX_OUTPUT_CHARS || stderr_chars > MAX_OUTPUT_CHARS;

                match output.exit_code {
                    Some(0) => info!(stdout_chars, stderr_chars, truncated, writable, command = %input.command, "sandbox command completed successfully"),
                    Some(exit_code) => warn!(exit_code, stdout_chars, stderr_chars, truncated, writable, command = %input.command, "sandbox command completed with a non-zero exit code"),
                    None => warn!(stdout_chars, stderr_chars, truncated, writable, command = %input.command, "sandbox command was terminated by a signal"),
                }

                let stdout = cap_output(&output.stdout, "stdout");
                let stderr = cap_output(&output.stderr, "stderr");

                let stdout_preview = preview_output(&output.stdout, "stdout");
                let stderr_preview = preview_output(&output.stderr, "stderr");
                let body = match (stdout_preview, stderr_preview) {
                    (Some(stdout), Some(stderr)) => Some(format!("{stdout}\n[stderr]\n{stderr}")),
                    (Some(stdout), None) => Some(stdout),
                    (None, Some(stderr)) => Some(stderr),
                    (None, None) => None,
                };

                let title = format!(
                    "**Ran** ```{}``` (exit {})",
                    input.command,
                    output
                        .exit_code
                        .map_or_else(|| "signal".to_string(), |code| code.to_string()),
                );
                let render = body.into_iter().fold(
                    RenderToolCall::new(RenderText::markdown(title)),
                    |render, body| render.with_body(RenderText::markdown(body)),
                );

                ToolResult::success(
                    call,
                    json!({
                        "command": input.command,
                        "stdout": stdout,
                        "stderr": stderr,
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
        let mut properties = serde_json::Map::from_iter([(
            "command".to_string(),
            json!({
                "type": "string",
                "description": "The Bash command to run in the sandbox."
            }),
        )]);
        if self.actions.contains(&BashAction::Write) {
            properties.insert(
                "write".to_string(),
                json!({
                    "type": "boolean",
                    "description": "Allow this command to write to workspace mounts. Defaults to false."
                }),
            );
        }

        ChatCompletionTool {
            tool_type: "function",
            function: FunctionDefinition {
                name: self.name().to_string(),
                description: self.description().to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": properties,
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

        assert_eq!(tool.actions(), [BashAction::Run, BashAction::Write]);
        assert!(tool.deferred());
    }

    #[test]
    fn filters_actions_at_instantiation() {
        let tool = BashTool::except_actions([BashAction::Run], false);

        assert_eq!(tool.actions(), [BashAction::Write]);
        assert!(!tool.deferred());
    }

    #[test]
    fn write_defaults_to_false() {
        let input: BashInput = serde_json::from_value(json!({ "command": "pwd" })).unwrap();

        assert!(!input.write);
    }

    #[test]
    fn caps_exceptionally_long_output() {
        let text = "a".repeat(MAX_OUTPUT_CHARS + 1);

        let capped = cap_output(&text, "stdout");

        assert!(capped.starts_with(&"a".repeat(MAX_OUTPUT_CHARS)));
        assert_eq!(capped.chars().nth(MAX_OUTPUT_CHARS), Some('\n'));
        assert!(capped.ends_with("[... stdout truncated after 100000 characters]"));
    }

    #[test]
    fn keeps_short_output_verbatim() {
        let text = "one\ntwo\nthree\nfour";

        assert_eq!(cap_output(text, "stdout"), text);
    }

    #[test]
    fn previews_only_the_first_two_lines() {
        assert_eq!(
            preview_output("one\ntwo\nthree\nfour", "stdout"),
            Some("one\ntwo\n… +2 stdout lines".to_string()),
        );
    }

    #[test]
    fn caps_on_character_boundaries() {
        let text = "🦀".repeat(MAX_OUTPUT_CHARS + 1);

        assert!(cap_output(&text, "stderr").starts_with(&"🦀".repeat(MAX_OUTPUT_CHARS)));
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
        assert_eq!(
            declaration["function"]["parameters"]["properties"]["write"]["type"],
            "boolean",
        );
    }

    #[test]
    fn omits_write_parameter_without_write_action() {
        let declaration = serde_json::to_value(
            BashTool::with_actions([BashAction::Run], false).as_chat_completion_tool(),
        )
        .unwrap();

        assert!(declaration["function"]["parameters"]["properties"]["write"].is_null());
    }
}
