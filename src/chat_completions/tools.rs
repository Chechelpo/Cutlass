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

/// Tool call result. Contains:
///
/// 1. tool call id
/// 2. content
#[derive(Debug, Serialize)]
pub struct ToolResult {
    pub tool_call_id: String,
    pub content: serde_json::Value,

    #[serde(skip_serializing)]
    pub log_header: String,

    #[serde(skip_serializing)]
    pub log_body: Option<String>,
}

impl ToolResult {
    pub fn success(
        call: &ToolCall,
        content: serde_json::Value,
        log_header: impl Into<String>,
        log_body: Option<String>,
    ) -> Self {
        Self {
            tool_call_id: call.id.clone(),
            content,
            log_header: log_header.into(),
            log_body,
        }
    }

    pub fn failure(call: &ToolCall, message: impl Into<String>) -> Self {
        let message = message.into();
        Self {
            tool_call_id: call.id.clone(),
            content: json!({"error": message}),
            log_header: format!("Tool call {} failed", call.id),
            log_body: Some(message),
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
            "Read file",
            Some("read 2 bytes".into()),
        );

        assert_eq!(result.log_header, "Read file");
        assert_eq!(result.log_body.as_deref(), Some("read 2 bytes"));
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
        assert_eq!(result.log_body.as_deref(), Some("denied"));
    }
}
