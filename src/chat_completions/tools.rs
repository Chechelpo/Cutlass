use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Debug, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,

    #[serde(rename = "type")]
    pub tool_type: String,

    pub function: FunctionCall,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String,
}


#[derive(Debug, Serialize, Deserialize)]
pub struct ChatCompletionTool {
    #[serde(rename = "type")]
    pub tool_type: &'static str,

    pub function: FunctionDefinition,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FunctionDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}


#[derive(Debug, Serialize)]
pub struct ToolResult {
    pub tool_call_id: String,
    pub content: serde_json::Value,
}

impl ToolResult {
    pub fn failure(call:&ToolCall, message:&str) -> ToolResult {
        ToolResult {
            tool_call_id: call.id.clone(),
            content: json!({"error": message}),
        }
    }
}
