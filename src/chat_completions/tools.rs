use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::ui_interface::chat::{RenderText, RenderToolCall};

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

/// Tool call result. Contains:
///
/// 1. tool call id
/// 2. content
#[derive(Debug, Serialize)]
pub struct ToolResult {
    pub tool_call_id: String,
    pub content: serde_json::Value,

    #[serde(skip_serializing)]
    pub render: RenderToolCall,
}

impl ToolResult {
    pub fn success(call: &ToolCall, content: serde_json::Value, render: RenderToolCall) -> Self {
        Self {
            tool_call_id: call.id.clone(),
            content,
            render,
        }
    }

    pub fn failure(call: &ToolCall, message: impl Into<String>) -> Self {
        let message = message.into();
        Self {
            tool_call_id: call.id.clone(),
            content: json!({"error": message}),
            render: RenderToolCall::new(RenderText::plain(format!("Tool call {} failed", call.id)))
                .with_body(RenderText::plain(message)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool_call() -> ToolCall {
        ToolCall {
            id: "call-1".into(),
            tool_type: "function".into(),
            function: FunctionCall {
                name: "read_file".into(),
                arguments: "{}".into(),
            },
        }
    }

    #[test]
    fn result_serializes_only_the_provider_fields() {
        let result = ToolResult::success(
            &tool_call(),
            json!({"content": "ok"}),
            RenderToolCall::new(RenderText::plain("Read file"))
                .with_body(RenderText::plain("read 2 bytes")),
        );

        assert_eq!(result.render.title.as_str(), "Read file");
        assert_eq!(
            result.render.body.as_ref().map(RenderText::as_str),
            Some("read 2 bytes"),
        );
        assert_eq!(
            serde_json::to_value(&result).unwrap(),
            json!({
                "tool_call_id": "call-1",
                "content": {"content": "ok"},
            }),
        );
    }

    #[test]
    fn failure_uses_the_same_tool_result_type() {
        let result = ToolResult::failure(&tool_call(), "denied");

        assert_eq!(result.content, json!({"error": "denied"}));
        assert_eq!(
            result.render.body.as_ref().map(RenderText::as_str),
            Some("denied"),
        );
    }
}
