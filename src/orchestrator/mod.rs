//! Agent workflow orchestration.
//!
//! The MVP supports a single-agent workflow. Agent spawning and delegation are
//! intentionally outside the active orchestration API.

mod agent_orchestrator;
mod workflows;

pub use workflows::BasicWorkflow;
