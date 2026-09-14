struct RetryCase {
    status: usize,
    message_match: Option<String>,
}

pub fn is_retry_case(status: usize, message: &str) -> bool {
    RETRY_CASES.iter().any(|case| {
        case.status == status
            && (case.message_match.is_none() || case.message_match.as_deref() == Some(message))
    })
}
static RETRY_CASES: &[RetryCase] = &[
    RetryCase {
        status: 408, // Request Timeout
        message_match: None,
    },
    RetryCase {
        status: 425, // Too Early
        message_match: None,
    },
    RetryCase {
        status: 429, // Too Many Requests
        message_match: None,
    },
    RetryCase {
        status: 500, // Internal Server Error
        message_match: None,
    },
    RetryCase {
        status: 502, // Bad Gateway
        message_match: None,
    },
    RetryCase {
        status: 503, // Service Unavailable
        message_match: None,
    },
    RetryCase {
        status: 504, // Gateway Timeout
        message_match: None,
    },
];
