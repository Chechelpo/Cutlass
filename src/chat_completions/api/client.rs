use std::error::Error;
use reqwest::blocking::Client;
use serde_json::{Value, json};
use std::time::Instant;
use tracing::{debug, error, warn};
use crate::chat_completions::api::retry_cases::is_retry_case;
use crate::chat_completions::messages::{AssistantMessage, Message};
use crate::chat_completions::tools::ChatCompletionTool;

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
            client: Client::builder()
                .connect_timeout(std::time::Duration::from_secs(10))
                .timeout(std::time::Duration::from_secs(120))
                .build()
                .expect("could not initialize the HTTP client"),
        }
    }

    pub fn call(
        &self,
        messages: &[Message],
        model: &str,
        host: &str,
        api_key: &str,
        max_output_tokens: usize,
        tools: &[ChatCompletionTool],
    ) -> Result<AssistantMessage, ApiError> {
        let endpoint = format!("{}/chat/completions", host.trim_end_matches('/'));

        let started_at = Instant::now();

        debug!(
        model = %model,
        message_count = messages.len(),
        tool_count = tools.len(),
        max_output_tokens,
        "sending chat completion request"
    );

        let response = self
            .client
            .post(&endpoint)
            .bearer_auth(api_key)
            .json(&json!({
            "model": model,
            "messages": messages,
            "max_completion_tokens": max_output_tokens,
            "tools": tools,
        }))
            .send()
            .map_err(|error| {
                let message = describe_reqwest_error(&error);

                // There is no HTTP response here, therefore is_retry_case()
                // cannot apply. These are transport-level failures.
                let is_retryable = error.is_timeout() || error.is_connect();

                error!(
                error = ?error,
                %message,
                retryable = is_retryable,
                elapsed_ms = started_at.elapsed().as_millis(),
                "chat completion request failed before receiving an HTTP response"
            );

                ApiError {
                    is_retryable,
                    status: 0,
                    message,
                }
            })?;

        // From this point onward we have an actual HTTP response.
        let status = response.status().as_u16() as usize;

        let raw_body = response.text().map_err(|error| {
            let message = describe_reqwest_error(&error);

            // The HTTP response started successfully, but reading the body failed.
            // This is still a transport/body failure rather than an HTTP-status
            // retry case.
            let is_retryable = error.is_timeout() || error.is_connect();

            error!(
            status,
            error = ?error,
            %message,
            retryable = is_retryable,
            elapsed_ms = started_at.elapsed().as_millis(),
            "could not read chat completion response body"
        );

            ApiError {
                is_retryable,
                status,
                message,
            }
        })?;

        if !(200..300).contains(&status) {
            let message = extract_error_message(&raw_body);

            // THIS is where your RETRY_CASES table applies.
            let is_retryable = is_retry_case(status, &message);

            warn!(
            status,
            retryable = is_retryable,
            error = %message,
            elapsed_ms = started_at.elapsed().as_millis(),
            "chat completion API returned an HTTP error"
        );

            return Err(ApiError {
                is_retryable,
                status,
                message,
            });
        }

        debug!(
        status,
        elapsed_ms = started_at.elapsed().as_millis(),
        response_bytes = raw_body.len(),
        "chat completion request succeeded"
    );

        let body: Value = serde_json::from_str(&raw_body).map_err(|error| {
            error!(
            status,
            response_bytes = raw_body.len(),
            error = ?error,
            "could not parse chat completion response"
        );

            ApiError {
                is_retryable: false,
                status,
                message: format!("Invalid API response: {error}"),
            }
        })?;

        let message = body
            .pointer("/choices/0/message")
            .cloned()
            .ok_or_else(|| {
                error!(
                status,
                "chat completion response contains no assistant message"
            );

                ApiError {
                    is_retryable: false,
                    status,
                    message: "API response contains no assistant message".into(),
                }
            })?;

        serde_json::from_value(message).map_err(|error| {
            error!(
            status,
            error = ?error,
            "could not deserialize assistant message"
        );

            ApiError {
                is_retryable: false,
                status,
                message: format!("Invalid assistant message: {error}"),
            }
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
fn describe_reqwest_error(error: &reqwest::Error) -> String {
    let kind = if error.is_timeout() {
        "timeout"
    } else if error.is_connect() {
        "connection"
    } else if error.is_request() {
        "request"
    } else if error.is_body() {
        "body"
    } else if error.is_decode() {
        "decode"
    } else {
        "unknown"
    };

    let mut message = format!("{kind} error");

    if let Some(url) = error.url() {
        message.push_str(&format!(" for {url}"));
    }

    if let Some(status) = error.status() {
        message.push_str(&format!(" (HTTP {})", status.as_u16()));
    }

    // Walk the actual underlying error chain.
    let mut source = error.source();
    let mut depth = 0;

    while let Some(cause) = source {
        depth += 1;
        message.push_str(&format!("; caused by[{depth}]: {cause}"));
        source = cause.source();
    }

    message
}