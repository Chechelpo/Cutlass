use super::{
    configuration::{Configuration, LABELS},
    markdown, shared,
};
use crate::config::ModelConfigStore;
use crate::orchestrator::workflow::session::WorkflowDefinition;
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Style},
    widgets::{Paragraph, Wrap},
};
use std::{io, path::Path};

pub(super) struct ConnectionConfiguration {
    state: Option<Configuration>,
}
impl ConnectionConfiguration {
    pub(super) fn new(state: Configuration) -> Self {
        Self { state: Some(state) }
    }
}
impl super::view::Window for ConnectionConfiguration {
    fn render(
        &mut self,
        frame: &mut Frame,
        _: &ModelConfigStore,
        _: &[WorkflowDefinition],
        _: &Path,
    ) {
        render(frame, self.state.as_ref().unwrap());
    }
    fn handle_event(
        &mut self,
        event: Event,
        store: &mut ModelConfigStore,
        _: &[WorkflowDefinition],
        _: &Path,
    ) -> io::Result<super::view::Transition> {
        handle_event(event, self.state.as_mut().unwrap(), store);
        let state = self.state.as_ref().unwrap();
        if matches!(state.page, super::configuration::SetupPage::Profiles) {
            Ok(super::view::Transition::Replace(Box::new(
                super::connection_list::ConnectionList::new(self.state.take().unwrap()),
            )))
        } else if matches!(state.page, super::configuration::SetupPage::Workflows) {
            Ok(super::view::Transition::Replace(Box::new(
                super::workflow_list::WorkflowList::new(self.state.take().unwrap()),
            )))
        } else {
            Ok(super::view::Transition::Stay)
        }
    }
}

pub(super) fn render(frame: &mut Frame, config: &Configuration) {
    if shared::small_terminal(frame) {
        return;
    }
    let [header, content, feedback, footer] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(3),
        Constraint::Length(3),
        Constraint::Length(2),
    ])
    .margin(1)
    .areas(frame.area());
    shared::setup_header(frame, "1 / 2 · Connection", header);
    render_fields(frame, config, content);

    if let Some(message) = &config.error {
        frame.render_widget(
            Paragraph::new(markdown::clean(message))
                .style(Style::default().fg(Color::Red))
                .wrap(Wrap { trim: false }),
            feedback,
        );
    } else {
        frame.render_widget(
            Paragraph::new("API key is masked and stored encrypted. The connection is used when you send your first prompt.")
                .style(Style::default().fg(shared::MUTED))
                .wrap(Wrap { trim: false }),
            feedback,
        );
    }
    shared::help(
        frame,
        "Tab / ↑↓ Fields · F2 Save · Esc Back · Ctrl+C Quit",
        footer,
    );
}

pub(super) fn handle_event(event: Event, config: &mut Configuration, store: &mut ModelConfigStore) {
    match event {
        Event::Paste(text) => {
            config.fields[config.focused]
                .insert(&text.replace("\r\n", "\n").replace('\r', "\n"), false);
        }
        Event::Key(key) if key.kind != KeyEventKind::Release => {
            config.error = None;
            match key.code {
                KeyCode::Esc if !store.configs().is_empty() => {
                    config.fields[3] = super::input::Input::default();
                    config.page = super::configuration::SetupPage::Profiles;
                }
                KeyCode::Tab | KeyCode::Down => {
                    config.focused = (config.focused + 1) % LABELS.len()
                }
                KeyCode::BackTab | KeyCode::Up => {
                    config.focused = (config.focused + LABELS.len() - 1) % LABELS.len()
                }
                KeyCode::F(2) => config.save(store),
                KeyCode::Enter
                    if key.modifiers.contains(KeyModifiers::CONTROL)
                        || config.focused == LABELS.len() - 1 =>
                {
                    config.save(store)
                }
                KeyCode::Enter => config.focused += 1,
                _ => config.fields[config.focused].key(key, false),
            }
        }
        _ => {}
    }
}

fn render_fields(frame: &mut Frame, config: &Configuration, area: Rect) {
    let visible = (area.height / 3).max(1) as usize;
    let start = config.focused.saturating_sub(visible - 1);
    for index in start..(start + visible).min(LABELS.len()) {
        let field_area = Rect::new(area.x, area.y + ((index - start) * 3) as u16, area.width, 3);
        render_field(frame, config, index, field_area);
    }
}

fn render_field(frame: &mut Frame, config: &Configuration, index: usize, area: Rect) {
    let focused = config.focused == index;
    let field = &config.fields[index];
    let is_api_key = index == 3;
    let text = if is_api_key {
        "•".repeat(field.text.chars().count())
    } else {
        field.text.clone()
    };
    let column = if is_api_key {
        field.text[..field.cursor].chars().count()
    } else {
        field.position().1
    };
    let inner_width = area.width.saturating_sub(3);
    let scroll = column
        .saturating_sub(inner_width as usize)
        .min(u16::MAX as usize) as u16;
    frame.render_widget(
        Paragraph::new(text)
            .block(shared::block(format!(" {} ", LABELS[index]), focused))
            .scroll((0, scroll)),
        area,
    );
    if focused {
        frame.set_cursor_position((
            area.x + 1 + (column.saturating_sub(scroll as usize) as u16).min(inner_width),
            area.y + 1,
        ));
    }
}
