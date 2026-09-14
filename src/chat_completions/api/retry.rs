use std::thread;
use std::time::{Duration, SystemTime};

use rand::RngExt;

use super::model::ApiError;

/// Timeout, retry, and API-key rotation settings.
#[derive(Clone, Debug)]
pub struct RetryConfig {
    /// Total requests permitted, including the initial request.
    pub max_attempts: u32,
    pub request_timeout: Duration,
    pub initial_backoff: Duration,
    pub max_backoff: Duration,
    /// Move to the next configured key after this many failed requests.
    pub failures_per_api_key: u32,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 4,
            request_timeout: Duration::from_secs(120),
            initial_backoff: Duration::from_millis(500),
            max_backoff: Duration::from_secs(30),
            failures_per_api_key: 3,
        }
    }
}

impl RetryConfig {
    pub(super) fn validate(&self) -> Result<(), ApiError> {
        if self.max_attempts == 0 {
            return Err(ApiError::Configuration(
                "max_attempts must be at least one".into(),
            ));
        }
        if self.request_timeout.is_zero() {
            return Err(ApiError::Configuration(
                "request_timeout must be greater than zero".into(),
            ));
        }
        if self.failures_per_api_key == 0 {
            return Err(ApiError::Configuration(
                "failures_per_api_key must be at least one".into(),
            ));
        }
        if self.initial_backoff > self.max_backoff {
            return Err(ApiError::Configuration(
                "initial_backoff cannot exceed max_backoff".into(),
            ));
        }
        Ok(())
    }
}

pub(super) fn is_retryable_status(status: u16) -> bool {
    matches!(status, 408 | 409 | 421 | 423 | 424 | 425 | 429 | 500..=599)
}

pub(super) fn retry_delay(config: &RetryConfig, attempt: u32) -> Duration {
    let exponent = attempt.saturating_sub(1).min(31);
    let base = config
        .initial_backoff
        .saturating_mul(1_u32 << exponent)
        .min(config.max_backoff);
    let jitter = rand::rng().random_range(0.0..=0.25);
    base.saturating_add(base.mul_f64(jitter))
}

pub(super) fn parse_retry_after(value: &str) -> Option<Duration> {
    if let Ok(seconds) = value.trim().parse::<f64>() {
        return (seconds.is_finite() && seconds >= 0.0).then(|| Duration::from_secs_f64(seconds));
    }
    let retry_at = httpdate::parse_http_date(value).ok()?;
    Some(
        retry_at
            .duration_since(SystemTime::now())
            .unwrap_or_default(),
    )
}

pub(super) fn ensure_not_interrupted(interrupt: Option<&dyn Fn() -> bool>) -> Result<(), ApiError> {
    if interrupt.is_some_and(|check| check()) {
        Err(ApiError::Interrupted)
    } else {
        Ok(())
    }
}

pub(super) fn interruptible_sleep(
    delay: Duration,
    interrupt: Option<&dyn Fn() -> bool>,
) -> Result<(), ApiError> {
    let deadline = std::time::Instant::now() + delay;
    loop {
        ensure_not_interrupted(interrupt)?;
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            return Ok(());
        }
        thread::sleep(remaining.min(Duration::from_millis(250)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_retryable_statuses_conservatively() {
        assert!(is_retryable_status(429));
        assert!(is_retryable_status(503));
        assert!(!is_retryable_status(400));
        assert!(!is_retryable_status(404));
    }

    #[test]
    fn parses_retry_after_seconds_and_date() {
        assert_eq!(parse_retry_after("1.5"), Some(Duration::from_millis(1500)));
        assert!(parse_retry_after("Wed, 21 Oct 2099 07:28:00 GMT").unwrap() > Duration::ZERO);
        assert_eq!(parse_retry_after("not a date"), None);
    }
}
