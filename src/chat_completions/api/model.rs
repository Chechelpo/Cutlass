use std::fmt;

use serde::{Deserialize, Serialize};

use crate::chat_completions::messages::Message;
use crate::chat_completions::tools::{ChatCompletionTool, ToolCall};

/// Every wire-level input needed for one logical model request.
///
/// Borrowing the messages and tools prevents this layer from changing the
/// agent's live state. Callers should snapshot those collections first if
/// another thread can mutate them.
#[derive(Debug, Serialize)]
pub struct ModelCall<'a> {
    pub model: &'a str,
    pub messages: &'a [Message],

    #[serde(skip_serializing_if = "slice_is_empty")]
    pub tools: &'a [ChatCompletionTool],

    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<&'a str>,
}

fn slice_is_empty<T>(value: &[T]) -> bool {
    value.is_empty()
}

/// Provider-independent subset of a Chat Completions response.
#[derive(Debug, Deserialize)]
pub struct ModelResponse {
    pub choices: Vec<ResponseChoice>,

    #[serde(default)]
    pub usage: ModelUsage,
}

#[derive(Debug, Deserialize)]
pub struct ResponseChoice {
    pub message: AssistantResponse,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AssistantResponse {
    pub content: Option<String>,

    #[serde(default)]
    pub tool_calls: Vec<ToolCall>,
}

#[derive(Default, Debug, Deserialize)]
pub struct ModelUsage {
    #[serde(alias = "input_tokens")]
    pub prompt_tokens: Option<u64>,

    #[serde(alias = "output_tokens")]
    pub completion_tokens: Option<u64>,

    pub total_tokens: Option<u64>,
}

/// Error surfaced by the complete model API boundary.
#[derive(Debug)]
pub enum ApiError {
    Configuration(String),
    Interrupted,
    RequestEncoding(serde_json::Error),
    Transport(String),
    Http { status: u16, detail: String },
    InvalidResponse(String),
    AttemptsExhausted { attempts: u32, last_error: String },
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Configuration(message) => write!(f, "invalid API configuration: {message}"),
            Self::Interrupted => write!(f, "model request interrupted"),
            Self::RequestEncoding(error) => write!(f, "could not encode model request: {error}"),
            Self::Transport(message) => write!(f, "model transport failed: {message}"),
            Self::Http { status, detail } => {
                write!(f, "model API returned HTTP {status}: {detail}")
            }
            Self::InvalidResponse(message) => write!(f, "invalid model response: {message}"),
            Self::AttemptsExhausted {
                attempts,
                last_error,
            } => {
                write!(
                    f,
                    "model API failed after {attempts} attempts: {last_error}"
                )
            }
        }
    }
}

impl std::error::Error for ApiError {}

pub(super) fn parse_response(body: &str) -> Result<ModelResponse, ApiError> {
    let response: ModelResponse =
        serde_json::from_str(body).map_err(|error| ApiError::InvalidResponse(error.to_string()))?;
    let choice = response
        .choices
        .first()
        .ok_or_else(|| ApiError::InvalidResponse("response contains no choices".into()))?;
    let has_content = choice
        .message
        .content
        .as_deref()
        .is_some_and(|content| !content.trim().is_empty());
    if !has_content && choice.message.tool_calls.is_empty() {
        return Err(ApiError::InvalidResponse(
            "first choice contains neither content nor tool calls".into(),
        ));
    }
    Ok(response)
}
