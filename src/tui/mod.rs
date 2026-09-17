//! Terminal frontend: connection configuration → workflow selection → session.

mod configuration;
mod connection_configuration;
mod connection_list;
mod input;
mod markdown;
mod session;
mod shared;
mod view;
mod workflow_list;

use crate::config::ModelConfigStore;
use crate::orchestrator::workflow::session::WorkflowDefinition;
use crossterm::{
    event::{self, DisableBracketedPaste, EnableBracketedPaste},
    execute,
};
use std::{io, path::PathBuf, time::Duration};

struct RestoreTerminal;
impl Drop for RestoreTerminal {
    fn drop(&mut self) {
        let _ = execute!(io::stdout(), DisableBracketedPaste);
        let _ = ratatui::try_restore();
    }
}

/// Configuration owns no workflow instance. Only confirming a factory in the
/// workflow picker starts a session, constructed entirely on its worker thread.
pub fn run_tui(
    store: &mut ModelConfigStore,
    workflows: Vec<WorkflowDefinition>,
    workspace: PathBuf,
) -> io::Result<()> {
    if workflows.is_empty() {
        return Err(io::Error::other("No executable workflows are registered"));
    }
    let _restore = RestoreTerminal;
    let mut terminal = ratatui::try_init()?;
    execute!(io::stdout(), EnableBracketedPaste)?;
    let configuration = configuration::Configuration::new(store);
    let initial: Box<dyn view::Window> = if store.configs().is_empty() {
        Box::new(connection_configuration::ConnectionConfiguration::new(
            configuration,
        ))
    } else {
        Box::new(connection_list::ConnectionList::new(configuration))
    };
    let mut coordinator = view::Coordinator::new(initial);
    loop {
        coordinator.poll();
        terminal.draw(|frame| coordinator.render(frame, store, &workflows, &workspace))?;
        if !event::poll(Duration::from_millis(50))? {
            continue;
        }
        if !coordinator.handle_event(event::read()?, store, &workflows, &workspace)? {
            return Ok(());
        }
    }
}
