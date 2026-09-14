use std::collections::VecDeque;

pub struct SteeringInbox{
    steering: VecDeque<String>,
    cancel_turn: bool
}

impl SteeringInbox {
    pub(crate) fn new() -> Self {
        SteeringInbox {
            steering: VecDeque::new(),
            cancel_turn: false,
        }
    }

    fn drain_steering_message(&mut self) -> Option<String> {
        if self.steering.is_empty() {
            return None;
        }

        Some(
            self.steering
                .drain(..)
                .collect::<Vec<String>>()
                .join("\n")
        )
    }

    fn add_message(&mut self, message: String) {
        self.steering.push_back(message);
    }

    fn cancel_turn(&mut self) {
        self.cancel_turn = true;
    }

    fn acknowledge_end_turn(&mut self) {
        self.cancel_turn = false;
    }
}