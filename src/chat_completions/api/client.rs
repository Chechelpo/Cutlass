use super::model::{ApiError, ModelCall, ModelResponse, parse_response};
use super::retry::{
    RetryConfig, ensure_not_interrupted, interruptible_sleep, is_retryable_status, retry_delay,
};
use super::transport::{HttpRequest, HttpTransport, ReqwestTransport};

/// OpenAI-compatible client that turns one [`ModelCall`] into a validated
/// [`ModelResponse`] using bounded, interruptible retries.
pub struct ChatCompletionsClient<T: HttpTransport = ReqwestTransport> {
    endpoint: String,
    api_keys: Vec<String>,
    retry: RetryConfig,
    transport: T,
}

impl ChatCompletionsClient<ReqwestTransport> {
    /// Construct a client using the default blocking HTTPS transport.
    pub fn new(
        endpoint: impl Into<String>,
        api_keys: Vec<String>,
        retry: RetryConfig,
    ) -> Result<Self, ApiError> {
        Self::with_transport(endpoint, api_keys, retry, ReqwestTransport::default())
    }
}

impl<T: HttpTransport> ChatCompletionsClient<T> {
    /// Construct a client with a custom transport.
    pub fn with_transport(
        endpoint: impl Into<String>,
        api_keys: Vec<String>,
        retry: RetryConfig,
        transport: T,
    ) -> Result<Self, ApiError> {
        retry.validate()?;
        let endpoint = endpoint.into();
        if endpoint.trim().is_empty() {
            return Err(ApiError::Configuration("endpoint cannot be empty".into()));
        }
        if api_keys.is_empty() || api_keys.iter().any(|key| key.is_empty()) {
            return Err(ApiError::Configuration(
                "at least one non-empty API key is required".into(),
            ));
        }
        Ok(Self {
            endpoint,
            api_keys,
            retry,
            transport,
        })
    }

    /// Submit one logical request.
    ///
    /// `interrupt` is checked before every attempt and at most every 250 ms
    /// during backoff. Return `true` from it when newer user steering makes the
    /// in-flight request obsolete.
    pub fn call(
        &self,
        model_call: &ModelCall<'_>,
        interrupt: Option<&dyn Fn() -> bool>,
    ) -> Result<ModelResponse, ApiError> {
        let body = serde_json::to_vec(model_call).map_err(ApiError::RequestEncoding)?;
        let mut last_error = String::new();

        for attempt in 1..=self.retry.max_attempts {
            ensure_not_interrupted(interrupt)?;
            let key_index =
                ((attempt - 1) / self.retry.failures_per_api_key) as usize % self.api_keys.len();
            let response = self.transport.send(HttpRequest {
                url: &self.endpoint,
                api_key: &self.api_keys[key_index],
                body: &body,
                timeout: self.retry.request_timeout,
            });

            let retry_after = match response {
                Ok(response) if (200..300).contains(&response.status) => {
                    match parse_response(&response.body) {
                        Ok(parsed) => return Ok(parsed),
                        Err(error) => {
                            last_error = error.to_string();
                            None
                        }
                    }
                }
                Ok(response) => {
                    let error = ApiError::Http {
                        status: response.status,
                        detail: response.body.trim().chars().take(500).collect(),
                    };
                    if !is_retryable_status(response.status) {
                        return Err(error);
                    }
                    last_error = error.to_string();
                    response.retry_after
                }
                Err(error) => {
                    if !error.retryable {
                        return Err(ApiError::Transport(error.message));
                    }
                    last_error = error.message;
                    None
                }
            };

            if attempt == self.retry.max_attempts {
                break;
            }
            let delay = retry_after.unwrap_or_else(|| retry_delay(&self.retry, attempt));
            interruptible_sleep(delay, interrupt)?;
        }

        Err(ApiError::AttemptsExhausted {
            attempts: self.retry.max_attempts,
            last_error,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use super::*;
    use crate::chat_completions::api::{HttpResponse, TransportError};

    struct FakeTransport {
        responses: Mutex<VecDeque<Result<HttpResponse, TransportError>>>,
        keys: Arc<Mutex<Vec<String>>>,
    }

    impl HttpTransport for FakeTransport {
        fn send(&self, request: HttpRequest<'_>) -> Result<HttpResponse, TransportError> {
            self.keys.lock().unwrap().push(request.api_key.to_string());
            self.responses.lock().unwrap().pop_front().unwrap()
        }
    }

    fn response(status: u16, body: &str) -> Result<HttpResponse, TransportError> {
        Ok(HttpResponse {
            status,
            body: body.into(),
            retry_after: None,
        })
    }

    fn retry_config(max_attempts: u32) -> RetryConfig {
        RetryConfig {
            max_attempts,
            request_timeout: Duration::from_secs(1),
            initial_backoff: Duration::ZERO,
            max_backoff: Duration::ZERO,
            failures_per_api_key: 1,
        }
    }

    fn call() -> ModelCall<'static> {
        ModelCall {
            model: "test-model",
            messages: &[],
            tools: &[],
            max_tokens: None,
            reasoning_effort: None,
        }
    }

    #[test]
    fn retries_transient_status_and_rotates_key() {
        let keys = Arc::new(Mutex::new(Vec::new()));
        let client = ChatCompletionsClient::with_transport(
            "https://example.test/v1/chat/completions",
            vec!["first".into(), "second".into()],
            retry_config(2),
            FakeTransport {
                responses: Mutex::new(VecDeque::from([
                    response(503, "busy"),
                    response(
                        200,
                        r#"{"choices":[{"message":{"content":"ok"},"finish_reason":"stop"}]}"#,
                    ),
                ])),
                keys: Arc::clone(&keys),
            },
        )
        .unwrap();

        let result = client.call(&call(), None).unwrap();

        assert_eq!(result.choices[0].message.content.as_deref(), Some("ok"));
        assert_eq!(*keys.lock().unwrap(), ["first", "second"]);
    }

    #[test]
    fn permanent_client_error_is_not_retried() {
        let keys = Arc::new(Mutex::new(Vec::new()));
        let client = ChatCompletionsClient::with_transport(
            "https://example.test/v1/chat/completions",
            vec!["key".into()],
            retry_config(2),
            FakeTransport {
                responses: Mutex::new(VecDeque::from([
                    response(400, "invalid request"),
                    response(200, r#"{"choices":[{"message":{"content":"unused"}}]}"#),
                ])),
                keys: Arc::clone(&keys),
            },
        )
        .unwrap();

        assert!(matches!(
            client.call(&call(), None),
            Err(ApiError::Http { status: 400, .. })
        ));
        assert_eq!(keys.lock().unwrap().len(), 1);
    }
}
