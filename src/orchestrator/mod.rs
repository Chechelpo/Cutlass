//! Agent workflow orchestration.
//!
//! The MVP supports a single-agent workflow. Agent spawning and delegation are
//! intentionally outside the active orchestration API.

mod agent_orchestrator;
pub mod workflow;

pub use workflow::BasicWorkflow;
