use super::{configuration::Configuration, markdown, shared};
use crate::{config::ModelConfigStore, orchestrator::workflow::session::WorkflowDefinition};
use crossterm::event::{Event, KeyCode, KeyEventKind};
use ratatui::{
    Frame,
    layout::{Constraint, Layout},
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{List, ListItem, ListState, Paragraph, Wrap},
};
use std::{io, path::Path};

pub(super) struct WorkflowList {
    state: Option<Configuration>,
}
impl WorkflowList {
    pub(super) fn new(state: Configuration) -> Self {
        Self { state: Some(state) }
    }
}
impl super::view::Window for WorkflowList {
    fn render(
        &mut self,
        frame: &mut Frame,
        store: &ModelConfigStore,
        workflows: &[WorkflowDefinition],
        _: &Path,
    ) {
        render(frame, self.state.as_ref().unwrap(), store, workflows);
    }
    fn handle_event(
        &mut self,
        event: Event,
        store: &mut ModelConfigStore,
        workflows: &[WorkflowDefinition],
        workspace: &Path,
    ) -> io::Result<super::view::Transition> {
        if let Some(index) = handle_event(event, self.state.as_mut().unwrap(), workflows.len()) {
            let model = store
                .active_config()
                .map_err(|error| io::Error::other(error.to_string()))?
                .clone();
            match super::session::Session::start(
                workflows[index].clone(),
                model,
                workspace.to_path_buf(),
            ) {
                Ok(session) => {
                    return Ok(super::view::Transition::Replace(Box::new(
                        super::session::SessionWindow::new(session, self.state.take().unwrap()),
                    )));
                }
                Err(error) => self.state.as_mut().unwrap().error = Some(error.to_string()),
            }
        }
        if matches!(
            self.state.as_ref().unwrap().page,
            super::configuration::SetupPage::Profiles
        ) {
            Ok(super::view::Transition::Replace(Box::new(
                super::connection_list::ConnectionList::new(self.state.take().unwrap()),
            )))
        } else {
            Ok(super::view::Transition::Stay)
        }
    }
}

pub(super) fn render(
    frame: &mut Frame,
    config: &Configuration,
    store: &ModelConfigStore,
    workflows: &[WorkflowDefinition],
) {
    if shared::small_terminal(frame) {
        return;
    }
    let [header, content, error, footer] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(3),
        Constraint::Length(3),
        Constraint::Length(2),
    ])
    .margin(1)
    .areas(frame.area());
    shared::setup_header(frame, "2 / 2 · Choose a workflow", header);

    let items = workflows
        .iter()
        .map(|workflow| {
            ListItem::new(vec![
                Line::raw(workflow.name.clone()),
                Line::styled(
                    workflow.description.clone(),
                    Style::default().fg(shared::MUTED),
                ),
                Line::default(),
            ])
        })
        .collect::<Vec<_>>();
    let connection = store
        .active_config()
        .map(|connection| format!("{} · {}", connection.name(), connection.id()))
        .unwrap_or_default();
    frame.render_stateful_widget(
        List::new(items)
            .block(shared::block(
                format!(" Workflows — {} ", markdown::clean(&connection)),
                true,
            ))
            .highlight_style(
                Style::default()
                    .fg(shared::ACCENT)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("› "),
        content,
        &mut ListState::default().with_selected(Some(config.selected_workflow)),
    );
    if let Some(message) = &config.error {
        frame.render_widget(
            Paragraph::new(markdown::clean(message))
                .style(Style::default().fg(Color::Red))
                .wrap(Wrap { trim: false }),
            error,
        );
    }
    shared::help(
        frame,
        "↑↓ Select · Enter Start session · Esc Connections · Ctrl+C Quit",
        footer,
    );
}

pub(super) fn handle_event(
    event: Event,
    config: &mut Configuration,
    workflow_count: usize,
) -> Option<usize> {
    let Event::Key(key) = event else { return None };
    if key.kind == KeyEventKind::Release {
        return None;
    }
    config.error = None;
    match key.code {
        KeyCode::Esc => config.page = super::configuration::SetupPage::Profiles,
        KeyCode::Down => {
            config.selected_workflow =
                (config.selected_workflow + 1).min(workflow_count.saturating_sub(1))
        }
        KeyCode::Up => config.selected_workflow = config.selected_workflow.saturating_sub(1),
        KeyCode::Enter if workflow_count > 0 => return Some(config.selected_workflow),
        _ => {}
    }
    None
}
