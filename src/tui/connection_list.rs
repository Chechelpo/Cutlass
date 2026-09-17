use super::{configuration::Configuration, markdown, shared};
use crate::config::ModelConfigStore;
use crate::orchestrator::workflow::session::WorkflowDefinition;
use crossterm::event::{Event, KeyCode, KeyEventKind};
use std::{io, path::Path};

pub(super) struct ConnectionList {
    state: Option<Configuration>,
}

impl ConnectionList {
    pub(super) fn new(state: Configuration) -> Self {
        Self { state: Some(state) }
    }
}

impl super::view::Window for ConnectionList {
    fn render(
        &mut self,
        frame: &mut Frame,
        store: &ModelConfigStore,
        _: &[WorkflowDefinition],
        _: &Path,
    ) {
        render(frame, self.state.as_ref().unwrap(), store);
    }
    fn handle_event(
        &mut self,
        event: Event,
        store: &mut ModelConfigStore,
        _: &[WorkflowDefinition],
        _: &Path,
    ) -> io::Result<super::view::Transition> {
        handle_event(event, self.state.as_mut().unwrap(), store);
        if matches!(
            self.state.as_ref().unwrap().page,
            super::configuration::SetupPage::Form
        ) {
            Ok(super::view::Transition::Replace(Box::new(
                super::connection_configuration::ConnectionConfiguration::new(
                    self.state.take().unwrap(),
                ),
            )))
        } else if matches!(
            self.state.as_ref().unwrap().page,
            super::configuration::SetupPage::Workflows
        ) {
            Ok(super::view::Transition::Replace(Box::new(
                super::workflow_list::WorkflowList::new(self.state.take().unwrap()),
            )))
        } else {
            Ok(super::view::Transition::Stay)
        }
    }
}
use ratatui::{
    Frame,
    layout::{Constraint, Layout},
    style::{Modifier, Style},
    text::Line,
    widgets::{List, ListItem, ListState, Paragraph, Wrap},
};

pub(super) fn render(frame: &mut Frame, config: &Configuration, store: &ModelConfigStore) {
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

    shared::setup_header(frame, "1 / 2 · Connection", header);
    let items = store
        .configs()
        .iter()
        .map(|connection| {
            ListItem::new(vec![
                Line::raw(markdown::clean(&format!(
                    "{}  ·  {}",
                    connection.name(),
                    connection.id()
                ))),
                Line::styled(
                    markdown::clean(connection.host_url()),
                    Style::default().fg(shared::MUTED),
                ),
                Line::default(),
            ])
        })
        .collect::<Vec<_>>();
    frame.render_stateful_widget(
        List::new(items)
            .block(shared::block(" Saved connections ", true))
            .highlight_style(
                Style::default()
                    .fg(shared::ACCENT)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("› "),
        content,
        &mut ListState::default().with_selected(Some(config.selected_profile)),
    );
    if let Some(message) = &config.error {
        frame.render_widget(
            Paragraph::new(markdown::clean(message))
                .style(Style::default().fg(ratatui::style::Color::Red))
                .wrap(Wrap { trim: false }),
            error,
        );
    }
    shared::help(
        frame,
        "↑↓ Select · Enter Continue · n New connection · d Delete · ctrl+C Quit",
        footer,
    );
}

pub(super) fn handle_event(event: Event, config: &mut Configuration, store: &mut ModelConfigStore) {
    let Event::Key(key) = event else { return };
    if key.kind == KeyEventKind::Release {
        return;
    }
    config.error = None;
    match key.code {
        KeyCode::Down => {
            config.selected_profile =
                (config.selected_profile + 1).min(store.configs().len().saturating_sub(1))
        }
        KeyCode::Up => config.selected_profile = config.selected_profile.saturating_sub(1),
        KeyCode::Char('n') => {
            config.page = super::configuration::SetupPage::Form;
            config.focused = 0;
        }
        KeyCode::Char('d') => {
            if let Some(profile) = store.configs().get(config.selected_profile) {
                let name = profile.name().to_owned();
                match store.remove(&name) {
                    Ok(_) => {
                        config.selected_profile = config
                            .selected_profile
                            .min(store.configs().len().saturating_sub(1));
                        if store.configs().is_empty() {
                            config.page = super::configuration::SetupPage::Form;
                            config.focused = 0;
                        }
                    }
                    Err(error) => config.error = Some(error.to_string()),
                }
            }
        }
        KeyCode::Enter => {
            if let Some(profile) = store.configs().get(config.selected_profile) {
                let name = profile.name().to_owned();
                if profile.encrypted_keys().is_empty() {
                    config.error = Some(
                        "This profile has no API key. Press n to create a complete connection."
                            .into(),
                    );
                } else if let Err(error) = store.set_active(&name) {
                    config.error = Some(error.to_string());
                } else {
                    config.page = super::configuration::SetupPage::Workflows;
                }
            }
        }
        _ => {}
    }
}
