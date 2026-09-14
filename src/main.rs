mod agent;
pub mod chat_completions;
pub mod config;
pub mod orchestrator;
pub mod tools;
pub mod ui_interface;
pub mod utils;

use crate::agent::presets::registry::AgentPresetRegistry;
use crate::agent::sandbox::filesystem::{BindMount, SandboxedFilesystem};
use crate::config::ModelConfigStore;
use crate::orchestrator::BasicWorkflow;
use crate::ui_interface::UserSessionViewport;
use crate::utils::default_ro_binds::ro_binds;
use std::env;

fn main() {
    let workspace = env::current_dir().expect("Failed to get current directory");
    let agent_registry = AgentPresetRegistry::new();
    let model_configs = ModelConfigStore::new().expect("Couldn't load model configurations");

    let workflow = BasicWorkflow::new(
        SandboxedFilesystem::new(
            ro_binds(),
            vec![BindMount {
                host: workspace.clone(),
                guest: workspace,
            }],
        ),
        model_configs
            .active_config()
            .expect("No active model configuration"),
        agent_registry
            .get_with_name("Coder")
            .expect("Failed to get Coder agent"),
    );
    let _user_session = UserSessionViewport::new(workflow);
}
