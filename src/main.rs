mod agent;
pub mod chat_completions;
pub mod config;
pub mod orchestrator;
pub mod tools;
pub mod tui;
pub mod ui;
pub mod ui_interface;
pub mod utils;

use crate::agent::presets::registry::AgentPresetRegistry;
use crate::agent::sandbox::filesystem::{BindMount, SandboxedFilesystem};
use crate::config::ModelConfigStore;
use crate::orchestrator::BasicWorkflow;
use crate::tui::{ensure_active_profile, run_tui};
use crate::ui::UserSessionViewport;
use crate::utils::default_ro_binds::ro_binds;
use std::env;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let workspace = env::current_dir()?;
    let agent_registry = AgentPresetRegistry::new();
    let mut model_configs = ModelConfigStore::new()?;
    if !ensure_active_profile(&mut model_configs)? {
        return Ok(());
    }

    let workflow = BasicWorkflow::new(
        SandboxedFilesystem::new(
            ro_binds(),
            vec![BindMount {
                host: workspace.clone(),
                guest: workspace,
            }],
        ),
        model_configs.active_config()?,
        agent_registry
            .get_with_name("Coder")
            .expect("Failed to get Coder agent"),
    );
    let user_session = UserSessionViewport::new(workflow);
    run_tui(user_session)?;
    Ok(())
}
