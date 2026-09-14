use reqwest::blocking::Client;
use serde_json::{Value, json};

use crate::chat_completions::api::retry_cases::is_retry_case;
use crate::chat_completions::messages::{AssistantMessage, Message};

#[derive(Debug)]
pub struct ApiError {
    pub is_retryable: bool,
    pub status: usize,
    pub message: String,
}

pub struct ApiClient {
    client: Client,
}

impl ApiClient {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }

    pub fn call(
        &self,
        messages: &[Message],
        model: &str,
        host: &str,
        api_key: &str,
        max_output_tokens: usize,
    ) -> Result<AssistantMessage, ApiError> {
        let endpoint = format!("{}/chat/completions", host.trim_end_matches('/'));

        let response = self
            .client
            .post(endpoint)
            .bearer_auth(api_key)
            .json(&json!({
                "model": model,
                "messages": messages,
                "max_completion_tokens": max_output_tokens,
            }))
            .send()
            .map_err(|error| ApiError {
                is_retryable: false,
                status: 0,
                message: error.to_string(),
            })?;

        let status = response.status().as_u16() as usize;

        let raw_body = response.text().map_err(|error| ApiError {
            is_retryable: false,
            status,
            message: error.to_string(),
        })?;

        if !(200..300).contains(&status) {
            let message = extract_error_message(&raw_body);

            return Err(ApiError {
                is_retryable: is_retry_case(status, &message),
                status,
                message,
            });
        }

        let body: Value = serde_json::from_str(&raw_body).map_err(|error| ApiError {
            is_retryable: false,
            status,
            message: format!("Invalid API response: {error}"),
        })?;

        let message = body
            .pointer("/choices/0/message")
            .cloned()
            .ok_or_else(|| ApiError {
                is_retryable: false,
                status,
                message: "API response contains no assistant message".into(),
            })?;

        serde_json::from_value(message).map_err(|error| ApiError {
            is_retryable: false,
            status,
            message: format!("Invalid assistant message: {error}"),
        })
    }
}

impl Default for ApiClient {
    fn default() -> Self {
        Self::new()
    }
}

fn extract_error_message(body: &str) -> String {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|body| {
            body.pointer("/error/message")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .unwrap_or_else(|| body.to_owned())
}
