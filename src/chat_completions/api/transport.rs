use std::time::Duration;

use reqwest::blocking::Client;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, RETRY_AFTER};

use super::retry::parse_retry_after;

/// Fully prepared request passed across the replaceable HTTP boundary.
pub struct HttpRequest<'a> {
    pub url: &'a str,
    pub api_key: &'a str,
    pub body: &'a [u8],
    pub timeout: Duration,
}

/// Response metadata consumed by parsing and retry policy.
pub struct HttpResponse {
    pub status: u16,
    pub body: String,
    pub retry_after: Option<Duration>,
}

/// Transport failure and whether repeating the request may succeed.
#[derive(Debug)]
pub struct TransportError {
    pub message: String,
    pub retryable: bool,
}

/// Injectable transport used to keep retry tests deterministic and offline.
pub trait HttpTransport: Send + Sync {
    fn send(&self, request: HttpRequest<'_>) -> Result<HttpResponse, TransportError>;
}

/// Default blocking HTTPS transport backed by Reqwest and Rustls.
pub struct ReqwestTransport {
    client: Client,
}

impl Default for ReqwestTransport {
    fn default() -> Self {
        Self {
            client: Client::new(),
        }
    }
}

impl HttpTransport for ReqwestTransport {
    fn send(&self, request: HttpRequest<'_>) -> Result<HttpResponse, TransportError> {
        let response = self
            .client
            .post(request.url)
            .header(CONTENT_TYPE, "application/json")
            .header(AUTHORIZATION, format!("Bearer {}", request.api_key))
            .timeout(request.timeout)
            .body(request.body.to_vec())
            .send()
            .map_err(|error| TransportError {
                retryable: error.is_timeout() || error.is_connect() || error.is_body(),
                message: error.to_string(),
            })?;

        let status = response.status().as_u16();
        let retry_after = response
            .headers()
            .get(RETRY_AFTER)
            .and_then(|value| value.to_str().ok())
            .and_then(parse_retry_after);
        let body = response.text().map_err(|error| TransportError {
            retryable: true,
            message: error.to_string(),
        })?;

        Ok(HttpResponse {
            status,
            body,
            retry_after,
        })
    }
}
