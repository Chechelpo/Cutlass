use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::chat_completions::tools::{ToolCall, ToolResult};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatCompletionRole {
    User,
    Assistant,
    System,
    Tool,
}

/// An assistant message returned by a Chat Completions API.
///
/// Missing, null, and empty `tool_calls` are normalized to an empty vector, so
/// a message is final exactly when it has no tool calls.
#[derive(Debug)]
pub struct AssistantMessage {
    content: Option<String>,
    tool_calls: Vec<ToolCall>,
}

/// The two actions an agent can take after receiving an assistant message.
#[derive(Debug)]
pub enum AssistantMessageOutcome {
    Final {
        content: Option<String>,
    },
    ToolCalls {
        content: Option<String>,
        tool_calls: Vec<ToolCall>,
    },
}

impl AssistantMessage {
    pub fn content(&self) -> Option<&str> {
        self.content.as_deref()
    }

    pub fn tool_calls(&self) -> &[ToolCall] {
        &self.tool_calls
    }

    pub fn is_final(&self) -> bool {
        self.tool_calls.is_empty()
    }

    pub fn has_tool_calls(&self) -> bool {
        !self.is_final()
    }

    pub fn into_tool_calls(self) -> Option<Vec<ToolCall>> {
        (!self.tool_calls.is_empty()).then_some(self.tool_calls)
    }

    pub fn into_outcome(self) -> AssistantMessageOutcome {
        if self.tool_calls.is_empty() {
            AssistantMessageOutcome::Final {
                content: self.content,
            }
        } else {
            AssistantMessageOutcome::ToolCalls {
                content: self.content,
                tool_calls: self.tool_calls,
            }
        }
    }
}

impl From<AssistantMessage> for Message {
    fn from(message: AssistantMessage) -> Self {
        Self::Assistant {
            content: message.content,
            tool_calls: message.tool_calls,
        }
    }
}

impl From<ToolResult> for Message {
    fn from(result: ToolResult) -> Self {
        let render = ToolMessageRender {
            header: result.log_header,
            body: result.log_body,
        };
        let content = match result.content {
            serde_json::Value::String(content) => content,
            content => content.to_string(),
        };

        Self::Tool {
            tool_call_id: result.tool_call_id,
            content,
            render: Some(render),
        }
    }
}

#[derive(Serialize, Deserialize)]
struct AssistantMessageWire {
    role: ChatCompletionRole,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<ToolCall>>,
}

#[derive(Serialize)]
struct AssistantMessageWireRef<'a> {
    role: ChatCompletionRole,
    content: &'a Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<&'a [ToolCall]>,
}

impl<'de> Deserialize<'de> for AssistantMessage {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = AssistantMessageWire::deserialize(deserializer)?;
        if wire.role != ChatCompletionRole::Assistant {
            return Err(serde::de::Error::custom("expected an assistant message"));
        }

        Ok(Self {
            content: wire.content,
            tool_calls: wire.tool_calls.unwrap_or_default(),
        })
    }
}

impl Serialize for AssistantMessage {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        AssistantMessageWireRef {
            role: ChatCompletionRole::Assistant,
            content: &self.content,
            tool_calls: (!self.tool_calls.is_empty()).then_some(self.tool_calls.as_slice()),
        }
        .serialize(serializer)
    }
}

/// Local presentation data retained alongside a tool message but never sent
/// to a Chat Completions provider.
#[derive(Debug)]
pub struct ToolMessageRender {
    pub header: String,
    pub body: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "role", rename_all = "lowercase")]
pub enum Message {
    User {
        content: String,
    },
    Assistant {
        content: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        tool_calls: Vec<ToolCall>,
    },
    System {
        content: String,
    },
    Tool {
        tool_call_id: String,
        content: String,
        #[serde(skip)]
        render: Option<ToolMessageRender>,
    },
}

impl Message {
    pub fn tool_render(&self) -> Option<&ToolMessageRender> {
        match self {
            Self::Tool { render, .. } => render.as_ref(),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn final_message_has_no_tool_calls() {
        let message: AssistantMessage = serde_json::from_value(json!({
            "role": "assistant", "content": "done"
        }))
        .unwrap();

        assert!(message.is_final());
        assert!(!message.has_tool_calls());
        assert_eq!(message.content(), Some("done"));
        assert!(message.tool_calls().is_empty());
    }

    #[test]
    fn message_with_tool_calls_is_not_final() {
        let message: AssistantMessage = serde_json::from_value(json!({
            "role": "assistant",
            "content": null,
            "tool_calls": [{
                "id": "call-1", "type": "function",
                "function": {"name": "read_file", "arguments": "{}"}
            }]
        }))
        .unwrap();

        assert!(!message.is_final());
        assert!(message.has_tool_calls());
        assert_eq!(message.tool_calls()[0].id, "call-1");
    }

    #[test]
    fn outcome_supports_exhaustive_tool_call_handling() {
        let message: AssistantMessage = serde_json::from_value(json!({
            "role": "assistant",
            "content": "checking",
            "tool_calls": [{
                "id": "call-1", "type": "function",
                "function": {"name": "read_file", "arguments": "{}"}
            }]
        }))
        .unwrap();

        match message.into_outcome() {
            AssistantMessageOutcome::ToolCalls {
                content,
                tool_calls,
            } => {
                assert_eq!(content.as_deref(), Some("checking"));
                assert_eq!(tool_calls.len(), 1);
            }
            AssistantMessageOutcome::Final { .. } => panic!("expected tool calls"),
        }
    }

    #[test]
    fn null_and_empty_tool_calls_are_both_final() {
        for tool_calls in [serde_json::Value::Null, json!([])] {
            let message: AssistantMessage = serde_json::from_value(json!({
                "role": "assistant", "content": "done", "tool_calls": tool_calls
            }))
            .unwrap();
            assert!(message.is_final());
        }
    }

    #[test]
    fn rejects_a_non_assistant_role() {
        let result = serde_json::from_value::<AssistantMessage>(json!({
            "role": "orchestrator", "content": "hello"
        }));
        assert!(result.is_err());
    }

    #[test]
    fn conversion_to_history_is_chat_completions_compatible() {
        let response: AssistantMessage = serde_json::from_value(json!({
            "role": "assistant", "content": null,
            "tool_calls": [{
                "id": "call-1", "type": "function",
                "function": {"name": "read_file", "arguments": "{}"}
            }]
        }))
        .unwrap();

        assert_eq!(
            serde_json::to_value(Message::from(response)).unwrap(),
            json!({
                "role": "assistant", "content": null,
                "tool_calls": [{
                    "id": "call-1", "type": "function",
                    "function": {"name": "read_file", "arguments": "{}"}
                }]
            })
        );
    }

    #[test]
    fn tool_result_converts_to_a_chat_completions_tool_message() {
        let result = ToolResult {
            tool_call_id: "call-1".into(),
            content: json!({"answer": 42}),
            log_header: "Calculated answer".into(),
            log_body: None,
        };

        let message = Message::from(result);
        let render = message.tool_render().unwrap();
        assert_eq!(render.header, "Calculated answer");
        assert_eq!(render.body, None);
        assert_eq!(
            serde_json::to_value(message).unwrap(),
            json!({
                "role": "tool",
                "tool_call_id": "call-1",
                "content": "{\"answer\":42}"
            })
        );
    }

    #[test]
    fn string_tool_result_is_not_double_encoded() {
        let result = ToolResult {
            tool_call_id: "call-1".into(),
            content: json!("plain output"),
            log_header: "Output".into(),
            log_body: None,
        };

        assert_eq!(
            serde_json::to_value(Message::from(result)).unwrap(),
            json!({
                "role": "tool",
                "tool_call_id": "call-1",
                "content": "plain output"
            })
        );
    }
}
