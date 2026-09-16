//! Terminal frontend: connection configuration → workflow selection → session.

mod configuration;
mod input;
mod markdown;
mod session;
mod view;

use crate::config::ModelConfigStore;
use crate::orchestrator::workflow::session::WorkflowDefinition;
use configuration::{Configuration, SetupPage};
use crossterm::{
    event::{
        self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyCode, KeyEventKind,
        KeyModifiers,
    },
    execute,
};
use session::Session;
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
    let mut config = Configuration::new(store);
    let mut session: Option<Session> = None;
    loop {
        if let Some(active) = &mut session {
            active.poll();
        }
        terminal.draw(|frame| match &mut session {
            Some(active) => view::session(frame, active, &workspace.display().to_string()),
            None => view::configuration(frame, &config, store, &workflows),
        })?;
        if !event::poll(Duration::from_millis(50))? {
            continue;
        }
        match event::read()? {
            Event::Key(key) if key.kind != KeyEventKind::Release => {
                if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
                    return Ok(());
                }
                if let Some(active) = &mut session {
                    match key.code {
                        KeyCode::Esc if !active.busy => {
                            session = None;
                            config.page = SetupPage::Workflows;
                        }
                        KeyCode::Enter
                            if key
                                .modifiers
                                .intersects(KeyModifiers::ALT | KeyModifiers::SHIFT) =>
                        {
                            active.composer.insert("\n", true)
                        }
                        KeyCode::Enter => active.submit(),
                        KeyCode::PageUp => active.scroll = active.scroll.saturating_add(10),
                        KeyCode::PageDown => active.scroll = active.scroll.saturating_sub(10),
                        KeyCode::Up => active.scroll = active.scroll.saturating_add(1),
                        KeyCode::Down => active.scroll = active.scroll.saturating_sub(1),
                        KeyCode::End if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            active.scroll = 0
                        }
                        _ => active.composer.key(key, true),
                    }
                } else if let Some(index) = config.key(key, store, workflows.len()) {
                    match store.active_config() {
                        Ok(model) => match Session::start(
                            workflows[index].clone(),
                            model.clone(),
                            workspace.clone(),
                        ) {
                            Ok(started) => session = Some(started),
                            Err(error) => config.error = Some(error.to_string()),
                        },
                        Err(error) => config.error = Some(error.to_string()),
                    }
                }
            }
            Event::Paste(text) => {
                let text = text.replace("\r\n", "\n").replace('\r', "\n");
                if let Some(active) = &mut session {
                    active.composer.insert(&text, true);
                } else if matches!(config.page, SetupPage::Form) {
                    config.fields[config.focused].insert(&text, false);
                }
            }
            _ => {}
        }
    }
}
