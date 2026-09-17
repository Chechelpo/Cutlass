mod agent;
pub mod chat_completions;
pub mod config;
pub mod orchestrator;
pub mod tools;
pub mod tui;
pub mod ui_interface;
pub mod utils;

use crate::config::ModelConfigStore;
use crate::orchestrator::workflow::session::built_in_workflows;
use crate::tui::run_tui;
use crate::utils::logger::init_logging;
use std::env;
use tracing::info;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let workspace = env::current_dir()?;
    let log_directory = workspace.join("logs");
    info!(
        "Initialized workspace at {}.\nLogs stored at {}",
        workspace.display(),
        log_directory.display()
    );

    std::fs::create_dir_all(&log_directory)?;
    let _logging_guard = init_logging(&log_directory);

    info!(
        workspace = %workspace.display(),
        log_directory = %log_directory.display(),
        "starting Cutlass"
    );

    let mut model_configs = ModelConfigStore::new()?;
    run_tui(&mut model_configs, built_in_workflows(), workspace)?;
    Ok(())
}
