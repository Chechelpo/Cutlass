//! Window contract and the deliberately small TUI coordinator.

use crate::{config::ModelConfigStore, orchestrator::workflow::session::WorkflowDefinition};
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::Frame;
use std::{io, path::Path};

pub(super) enum Transition {
    Stay,
    Replace(Box<dyn Window>),
}

pub(super) trait Window {
    fn poll(&mut self) {}
    fn render(
        &mut self,
        frame: &mut Frame,
        store: &ModelConfigStore,
        workflows: &[WorkflowDefinition],
        workspace: &Path,
    );
    fn handle_event(
        &mut self,
        event: Event,
        store: &mut ModelConfigStore,
        workflows: &[WorkflowDefinition],
        workspace: &Path,
    ) -> io::Result<Transition>;
}

pub(super) struct Coordinator {
    open: Box<dyn Window>,
}

impl Coordinator {
    pub(super) fn new(open: Box<dyn Window>) -> Self {
        Self { open }
    }
    pub(super) fn poll(&mut self) {
        self.open.poll();
    }
    pub(super) fn render(
        &mut self,
        frame: &mut Frame,
        store: &ModelConfigStore,
        workflows: &[WorkflowDefinition],
        workspace: &Path,
    ) {
        self.open.render(frame, store, workflows, workspace);
    }
    pub(super) fn handle_event(
        &mut self,
        event: Event,
        store: &mut ModelConfigStore,
        workflows: &[WorkflowDefinition],
        workspace: &Path,
    ) -> io::Result<bool> {
        if matches!(event, Event::Key(key) if key.kind != KeyEventKind::Release && key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL))
        {
            return Ok(false);
        }
        match self.open.handle_event(event, store, workflows, workspace)? {
            Transition::Stay => Ok(true),
            Transition::Replace(next) => {
                self.open = next;
                Ok(true)
            }
        }
    }
}
