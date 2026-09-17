use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct SteeringInbox {
    state: Arc<Mutex<SteeringState>>,
}

#[derive(Default)]
struct SteeringState {
    steering: VecDeque<String>,
    cancel_turn: bool,
}

impl SteeringInbox {
    pub(crate) fn new() -> Self {
        SteeringInbox {
            state: Arc::new(Mutex::new(SteeringState::default())),
        }
    }

    pub fn drain_steering_message(&self) -> Option<String> {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        if state.steering.is_empty() {
            return None;
        }

        Some(state.steering.drain(..).collect::<Vec<String>>().join("\n"))
    }

    pub fn add_message(&self, message: String) {
        self.state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .steering
            .push_back(message);
    }

    pub fn end_turn_called(&self) -> bool {
        self.state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .cancel_turn
    }
    pub fn cancel_turn(&self) {
        self.state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .cancel_turn = true;
    }

    /// Consumes a turn-stop request and discards steering queued for that turn.
    pub fn take_end_turn(&self) -> bool {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        if !state.cancel_turn {
            return false;
        }
        state.cancel_turn = false;
        state.steering.clear();
        true
    }

    pub fn acknowledge_end_turn(&self) {
        self.state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .cancel_turn = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clones_share_messages_and_turn_cancellation() {
        let inbox = SteeringInbox::new();
        let sender = inbox.clone();

        sender.add_message("first".into());
        sender.add_message("second".into());
        sender.cancel_turn();

        assert_eq!(inbox.drain_steering_message().as_deref(), Some("first\nsecond"));
        assert!(inbox.end_turn_called());
        assert!(inbox.take_end_turn());
        assert!(!sender.end_turn_called());
        assert!(inbox.drain_steering_message().is_none());
    }
}
