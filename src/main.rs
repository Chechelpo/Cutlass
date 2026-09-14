mod agent;
pub mod config;
pub mod tools;
pub mod chat_completions;
pub mod utils;

use std::env;
use crate::agent::agent::Agent;
use crate::agent::sandbox::filesystem::{BindMount, SandboxedFilesystem};
use crate::utils::default_ro_binds::{ro_binds};
use crate::agent::presets::registry::AgentPresetRegistry;
use crate::config::ModelConfigStore;

fn main() {
    let workspace = env::current_dir()
        .expect("Failed to get current directory");
    let agent_registry = AgentPresetRegistry::new();

    let agent = Agent::new (
        SandboxedFilesystem::new(
            ro_binds(),
            vec![
                BindMount {
                host: workspace.clone(),
                guest: workspace
                }
            ]
        ),
        ModelConfigStore::new().expect("Couldn't get active").active_config().unwrap(),
        agent_registry.get_with_name("Coder").expect("Failed to get Coder agent")
    );
}