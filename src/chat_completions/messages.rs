use serde::{Deserialize, Serialize};
use crate::chat_completions::tools::ToolCall;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatCompletionRole {
    User,
    Assistant,
    System,
    Tool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MessageBase {
    pub role: ChatCompletionRole,
    pub content: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AssistantMessage {
    #[serde(flatten)]
    pub base: MessageBase,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
}


#[derive(Debug, Serialize, Deserialize)]
pub struct ToolMessage {
    pub role: ChatCompletionRole,
    pub tool_call_id: String,
    pub content: String,
}


#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "role", rename_all = "lowercase")]
pub enum Message {
    User {
        content: String,
    },

    Assistant {
        content: Option<String>,
        tool_calls: Option<Vec<ToolCall>>,
    },

    System {
        content: String,
    },

    Tool {
        tool_call_id: String,
        content: String,
    },
}
