//! Shared interface used by graphical, terminal, and other frontends.

mod user_view;

pub use user_view::{SessionState, UiError, UserSessionViewport};
