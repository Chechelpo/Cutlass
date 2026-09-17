//! Synchronous user interaction requests shared by agents and frontends.

use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

pub const USER_UNAVAILABLE_MESSAGE: &str = "user-unavailable: no response was received within the timeout period. The user may be away. Pick the best answer yourself based on the question you asked.";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PromptOption {
    pub label: String,
    pub description: Option<String>,
}

impl PromptOption {
    pub fn new(label: impl Into<String>, description: Option<String>) -> Self {
        Self {
            label: label.into(),
            description,
        }
    }
}

/// A pending question. The response channel deliberately carries `Option` so
/// a frontend can dismiss a question immediately when a turn is cancelled.
#[derive(Debug)]
pub struct PromptRequest {
    pub question: String,
    pub options: Vec<PromptOption>,
    last_activity: Instant,
    timeout: Duration,
    reminder_interval: Duration,
    next_reminder: Option<Instant>,
    response: Sender<PromptResponse>,
}

impl PromptRequest {
    pub fn expires_at(&self) -> Option<Instant> {
        self.last_activity.checked_add(self.timeout)
    }

    pub fn remaining(&self) -> Duration {
        self.expires_at()
            .map(|deadline| deadline.saturating_duration_since(Instant::now()))
            .unwrap_or(self.timeout)
    }

    pub fn respond(self, answer: impl Into<String>) -> bool {
        self.response.send(PromptResponse::Answer(answer.into())).is_ok()
    }

    pub fn dismiss(self) -> bool {
        self.response.send(PromptResponse::Dismiss).is_ok()
    }

    /// Resets both the inactivity timeout and the reminder cadence.
    pub fn activity(&mut self) -> bool {
        self.last_activity = Instant::now();
        self.next_reminder = self.last_activity.checked_add(self.reminder_interval);
        self.response.send(PromptResponse::Activity).is_ok()
    }

    /// Returns true once per reminder interval while the request is idle.
    pub fn take_reminder_due(&mut self) -> bool {
        let now = Instant::now();
        let Some(next_reminder) = self.next_reminder else {
            return false;
        };
        if now < next_reminder {
            return false;
        }
        self.next_reminder = now.checked_add(self.reminder_interval);
        true
    }
}

#[derive(Debug)]
enum PromptResponse {
    Answer(String),
    Activity,
    Dismiss,
}

#[derive(Clone, Debug)]
pub struct UserInteractionBroker {
    requests: mpsc::Sender<PromptRequest>,
}

impl UserInteractionBroker {
    pub fn channel() -> (Self, Receiver<PromptRequest>) {
        let (requests, receiver) = mpsc::channel();
        (Self { requests }, receiver)
    }

    pub fn ask(
        &self,
        question: String,
        options: Vec<PromptOption>,
        timeout: Duration,
    ) -> Option<String> {
        self.ask_with_reminders(
            question,
            options,
            timeout,
            Duration::from_secs(30),
        )
    }

    pub fn ask_with_reminders(
        &self,
        question: String,
        options: Vec<PromptOption>,
        timeout: Duration,
        reminder_interval: Duration,
    ) -> Option<String> {
        let (response, answers) = mpsc::channel();
        let now = Instant::now();
        let request = PromptRequest {
            question,
            options,
            last_activity: now,
            timeout,
            reminder_interval,
            next_reminder: now.checked_add(reminder_interval),
            response,
        };

        if self.requests.send(request).is_err() {
            return None;
        }

        loop {
            match answers.recv_timeout(timeout) {
                Ok(PromptResponse::Answer(answer)) => return Some(answer),
                Ok(PromptResponse::Activity) => continue,
                Ok(PromptResponse::Dismiss) | Err(_) => return None,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn carries_a_response_back_to_the_waiting_caller() {
        let (broker, requests) = UserInteractionBroker::channel();
        let responder = thread::spawn(move || {
            let request = requests.recv().unwrap();
            assert_eq!(request.question, "Which one?");
            assert_eq!(request.options[0].label, "First");
            assert!(request.respond("First"));
        });

        let answer = broker.ask(
            "Which one?".into(),
            vec![PromptOption::new("First", None)],
            Duration::from_secs(1),
        );

        assert_eq!(answer.as_deref(), Some("First"));
        responder.join().unwrap();
    }

    #[test]
    fn returns_none_after_the_timeout() {
        let (broker, _requests) = UserInteractionBroker::channel();
        assert_eq!(
            broker.ask("Still there?".into(), Vec::new(), Duration::from_millis(1)),
            None,
        );
    }

    #[test]
    fn activity_completely_resets_the_inactivity_timeout() {
        let (broker, requests) = UserInteractionBroker::channel();
        let responder = thread::spawn(move || {
            let mut request = requests.recv().unwrap();
            thread::sleep(Duration::from_millis(25));
            assert!(request.take_reminder_due());
            assert!(request.activity());
            assert!(!request.take_reminder_due());
            thread::sleep(Duration::from_millis(25));
            assert!(request.respond("back"));
        });

        assert_eq!(
            broker
                .ask_with_reminders(
                    "Still there?".into(),
                    Vec::new(),
                    Duration::from_millis(40),
                    Duration::from_millis(10),
                )
                .as_deref(),
            Some("back"),
        );
        responder.join().unwrap();
    }
}
