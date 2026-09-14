//! Typed OpenAI-compatible Chat Completions API boundary.
//!
//! Agent orchestration supplies an immutable [`ModelCall`] and receives a
//! validated [`ModelResponse`]. This module owns wire serialization, HTTPS,
//! authentication, API-key rotation, and bounded retry behavior.
//!
//! The implementation is divided by responsibility:
//!
//! - [`client`] coordinates one logical model call across attempts.
//! - [`model`] defines request, response, usage, and error contracts.
//! - [`retry`] classifies transient failures and calculates backoff.
//! - [`transport`] contains the replaceable HTTP boundary.
//!
//! HTTP 408/409/421/423/424/425/429 and 5xx responses are retried. Other 4xx
//! responses fail immediately. Transport implementations explicitly classify
//! their failures as retryable or permanent. Backoff is exponential, bounded,
//! positively jittered, and interruptible so newer user steering can cancel a
//! stale request.

mod client;
mod model;
mod retry;
mod transport;

pub use client::ChatCompletionsClient;
pub use model::{
    ApiError, AssistantResponse, ModelCall, ModelResponse, ModelUsage, ResponseChoice,
};
pub use retry::RetryConfig;
pub use transport::{HttpRequest, HttpResponse, HttpTransport, ReqwestTransport, TransportError};
