use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::agent::agent::Agent;
use crate::chat_completions::tools::{ChatCompletionTool, FunctionDefinition};
use crate::tools::tool::Tool;


#[derive(Debug, Deserialize)]
pub struct ReadFileInput {
    pub path: String,
}


#[derive(Debug, Serialize)]
pub struct ReadFileOutput {
    pub path: String,
    pub content: String,
}


pub struct ReadFileTool;


impl ReadFileTool {
    pub fn new() -> Self {
        Self
    }
}


impl Tool for ReadFileTool {
    type Action = String;
    type Input = ReadFileInput;
    type Output = ReadFileOutput;
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

    fn deferred(&self) -> bool {
        false
    }

    fn all_actions(&self) -> &[Self::Action] {
        &[]
    }

    fn execute(
        &self,
        context: &Agent,
        input: Self::Input,
    ) -> Self::Output {
        let path = std::path::Path::new(&input.path);

        match context.sandbox.workspace().read_file(path) {
            Ok(content) => ReadFileOutput {
                path: input.path,
                content,
            },

            Err(err) => ReadFileOutput {
                path: input.path,
                content: format!("Error reading file: {}", err),
            },
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
    fn declares_the_path_parameter() {
        let declaration = serde_json::to_value(
            ReadFileTool::new().as_chat_completion_tool(),
        )
        .unwrap();

        assert_eq!(declaration["type"], "function");
        assert_eq!(declaration["function"]["name"], "read_file");
        assert_eq!(
            declaration["function"]["parameters"]["required"],
            json!(["path"]),
        );
    }
}
