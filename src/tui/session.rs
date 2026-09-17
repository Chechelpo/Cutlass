//! Represents a workflow session.
use super::configuration::Configuration;
use super::{input::Input, markdown, shared};
use crate::agent::presets::registry::AgentPresetRegistry;
use crate::agent::sandbox::filesystem::{BindMount, SandboxedFilesystem};
use crate::config::ModelConfig;
use crate::config::ModelConfigStore;
use crate::orchestrator::workflow::session::{WorkflowContext, WorkflowDefinition};
use crate::ui_interface::chat::RenderMessageSection;
use crate::ui_interface::chat::{
    RenderColor, RenderToolCall, RenderToolGroup, ToolGroupColorScheme,
};
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Paragraph, Wrap},
};
use std::path::Path;

pub(super) struct SessionWindow {
    session: Session,
    return_to: Option<Configuration>,
}

impl SessionWindow {
    pub(super) fn new(session: Session, return_to: Configuration) -> Self {
        Self {
            session,
            return_to: Some(return_to),
        }
    }
}

impl super::view::Window for SessionWindow {
    fn poll(&mut self) {
        self.session.poll();
    }
    fn render(
        &mut self,
        frame: &mut Frame,
        _: &ModelConfigStore,
        _: &[WorkflowDefinition],
        workspace: &Path,
    ) {
        self.session.render(frame, &workspace.display().to_string());
    }
    fn handle_event(
        &mut self,
        event: Event,
        _: &mut ModelConfigStore,
        _: &[WorkflowDefinition],
        _: &Path,
    ) -> io::Result<super::view::Transition> {
        if self.session.handle_event(event) {
            Ok(super::view::Transition::Replace(Box::new(
                super::workflow_list::WorkflowList::new(self.return_to.take().unwrap()),
            )))
        } else {
            Ok(super::view::Transition::Stay)
        }
    }
}
use crate::utils::default_ro_binds::ro_binds;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::{io, thread};

enum Update {
    Ready,
    Section(RenderMessageSection),
    Finished(Result<(), String>),
    Failed(String),
}

pub(super) struct Session {
    pub workflow: String,
    pub model: String,
    pub sections: Vec<RenderMessageSection>,
    pub composer: Input,
    pub busy: bool,
    pub starting: bool,
    pub disconnected: bool,
    pub error: Option<String>,
    /// Number of wrapped lines above the bottom; zero follows incoming output.
    pub scroll: usize,
    pub tick: usize,
    commands: Sender<String>,
    updates: Receiver<Update>,
}

impl Session {
    /// Handles session-local input. Returns true when the view should close.
    pub(super) fn handle_event(&mut self, event: Event) -> bool {
        match event {
            Event::Paste(text) => {
                self.composer
                    .insert(&text.replace("\r\n", "\n").replace('\r', "\n"), true);
            }
            Event::Key(key) if key.kind != KeyEventKind::Release => match key.code {
                KeyCode::Esc if !self.busy => return true,
                KeyCode::Enter
                    if key
                        .modifiers
                        .intersects(KeyModifiers::ALT | KeyModifiers::SHIFT) =>
                {
                    self.composer.insert("\n", true)
                }
                KeyCode::Enter => self.submit(),
                KeyCode::PageUp => self.scroll = self.scroll.saturating_add(10),
                KeyCode::PageDown => self.scroll = self.scroll.saturating_sub(10),
                KeyCode::Up => self.scroll = self.scroll.saturating_add(1),
                KeyCode::Down => self.scroll = self.scroll.saturating_sub(1),
                KeyCode::End if key.modifiers.contains(KeyModifiers::CONTROL) => self.scroll = 0,
                _ => self.composer.key(key, true),
            },
            _ => {}
        }
        false
    }

    pub(super) fn render(&mut self, frame: &mut Frame, workspace: &str) {
        if shared::small_terminal(frame) {
            return;
        }
        let composer_height = (self.composer.text.lines().count() + 1).clamp(2, 5) as u16 + 2;
        let [header, history, status, composer, footer] = Layout::vertical([
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(if self.error.is_some() { 3 } else { 1 }),
            Constraint::Length(composer_height),
            Constraint::Length(2),
        ])
        .margin(1)
        .areas(frame.area());
        self.render_header(frame, workspace, header);
        self.render_history(frame, history);
        self.render_status(frame, status);
        self.render_composer(frame, composer);
        shared::help(
            frame,
            "Enter Send · Alt+Enter Newline · PgUp/PgDn Scroll\nEsc End session (when idle) · Ctrl+C Quit",
            footer,
        );
    }

    fn render_header(&self, frame: &mut Frame, workspace: &str, area: Rect) {
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(vec![
                    Span::styled(
                        "✦ Cutlass",
                        Style::default()
                            .fg(shared::ACCENT)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(format!(
                        "   {} · {}",
                        markdown::clean(&self.workflow),
                        markdown::clean(&self.model)
                    )),
                ]),
                Line::styled(
                    markdown::clean(workspace),
                    Style::default().fg(shared::MUTED),
                ),
            ]),
            area,
        );
    }

    fn render_history(&mut self, frame: &mut Frame, area: Rect) {
        let lines = if self.sections.is_empty() {
            vec![
                Line::default(),
                Line::raw("What would you like to work on?"),
                Line::styled(
                    "Ask a question or describe a change to your workspace.",
                    Style::default().fg(shared::MUTED),
                ),
            ]
        } else {
            transcript(&self.sections)
        };
        let paragraph = Paragraph::new(Text::from(lines)).wrap(Wrap { trim: false });
        let bottom = paragraph
            .line_count(area.width)
            .saturating_sub(area.height as usize);
        self.scroll = self.scroll.min(bottom);
        frame.render_widget(
            paragraph.scroll((
                bottom.saturating_sub(self.scroll).min(u16::MAX as usize) as u16,
                0,
            )),
            area,
        );
    }

    fn render_status(&self, frame: &mut Frame, area: Rect) {
        let (message, color) = if let Some(error) = &self.error {
            (markdown::clean(error), Color::Red)
        } else if self.starting {
            ("Starting workflow…".into(), shared::ACCENT)
        } else if self.busy {
            (
                format!(
                    "{} Working…",
                    ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧"][self.tick / 2 % 8]
                ),
                shared::ACCENT,
            )
        } else {
            ("Ready".into(), shared::MUTED)
        };
        frame.render_widget(
            Paragraph::new(message)
                .style(Style::default().fg(color))
                .wrap(Wrap { trim: false }),
            area,
        );
    }

    fn render_composer(&self, frame: &mut Frame, area: Rect) {
        let (row, column) = self.composer.position();
        let vertical = row
            .saturating_sub(area.height.saturating_sub(3) as usize)
            .min(u16::MAX as usize) as u16;
        let horizontal = column
            .saturating_sub(area.width.saturating_sub(3) as usize)
            .min(u16::MAX as usize) as u16;
        frame.render_widget(
            Paragraph::new(self.composer.text.as_str())
                .block(shared::block(
                    if self.busy {
                        " Draft next prompt "
                    } else {
                        " › Prompt "
                    },
                    true,
                ))
                .scroll((vertical, horizontal)),
            area,
        );
        frame.set_cursor_position((
            area.x
                + 1
                + (column.saturating_sub(horizontal as usize) as u16)
                    .min(area.width.saturating_sub(3)),
            area.y
                + 1
                + (row.saturating_sub(vertical as usize) as u16).min(area.height.saturating_sub(3)),
        ));
    }

    pub fn start(
        definition: WorkflowDefinition,
        model: ModelConfig,
        workspace: PathBuf,
    ) -> io::Result<Self> {
        let (commands, requests) = mpsc::channel::<String>();
        let (output, updates) = mpsc::channel();
        let workflow_name = definition.name.clone();
        let model_name = model.id().to_owned();
        thread::Builder::new()
            .name("cutlass-workflow".into())
            .spawn(move || {
                let agents = AgentPresetRegistry::new();
                let context = WorkflowContext {
                    workspace: SandboxedFilesystem::new(
                        workspace.clone(),
                        ro_binds(),
                        vec![BindMount::path(workspace)],
                    ),
                    model: &model,
                    agents: &agents,
                };
                let mut workflow = match (definition.create)(context) {
                    Ok(workflow) => workflow,
                    Err(error) => {
                        let _ = output.send(Update::Failed(error.message));
                        return;
                    }
                };
                if output.send(Update::Ready).is_err() {
                    return;
                }
                while let Ok(prompt) = requests.recv() {
                    let result = workflow.submit(prompt, &mut |section| {
                        let _ = output.send(Update::Section(section));
                    });
                    if output
                        .send(Update::Finished(result.map_err(|e| e.message)))
                        .is_err()
                    {
                        break;
                    }
                }
            })?;
        Ok(Self {
            workflow: workflow_name,
            model: model_name,
            sections: Vec::new(),
            composer: Input::default(),
            busy: true,
            starting: true,
            disconnected: false,
            error: None,
            scroll: 0,
            tick: 0,
            commands,
            updates,
        })
    }

    pub fn poll(&mut self) {
        self.tick = self.tick.wrapping_add(1);
        if self.disconnected {
            return;
        }
        loop {
            match self.updates.try_recv() {
                Ok(Update::Ready) => {
                    self.starting = false;
                    self.busy = false;
                }
                Ok(Update::Section(section)) => self.sections.push(section),
                Ok(Update::Finished(result)) => {
                    self.busy = false;
                    self.error = result.err();
                }
                Ok(Update::Failed(error)) => {
                    self.error = Some(error);
                    self.disconnected = true;
                    self.busy = false;
                    self.starting = false;
                    break;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.error =
                        Some("The workflow worker stopped. Press Esc to return to setup.".into());
                    self.disconnected = true;
                    self.busy = false;
                    self.starting = false;
                    break;
                }
            }
        }
    }

    pub fn submit(&mut self) {
        if self.busy || self.disconnected || self.composer.text.trim().is_empty() {
            return;
        }
        if self.commands.send(self.composer.text.clone()).is_ok() {
            self.composer.take();
            self.busy = true;
            self.error = None;
            self.scroll = 0;
        } else {
            self.error = Some("The workflow is no longer running.".into());
            self.disconnected = true;
        }
    }
}

fn transcript(sections: &[RenderMessageSection]) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    for section in sections {
        match section {
            RenderMessageSection::Message { speaker, content } => {
                lines.push(Line::styled(
                    markdown::clean(speaker),
                    Style::default()
                        .fg(shared::ACCENT)
                        .add_modifier(Modifier::BOLD),
                ));
                lines.extend(markdown::render(content));
            }
            RenderMessageSection::ToolGroup(group) => lines.extend(render_tool_group(group)),
        }
        lines.push(Line::default());
    }
    lines
}

fn render_tool_group(group: &RenderToolGroup) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    for mut line in markdown::render(&group.header) {
        apply_fallback_color(&mut line, render_color(group.color_scheme.header));
        lines.push(line);
    }
    for (index, call) in group.calls.iter().enumerate() {
        lines.extend(render_tool_call(
            call,
            index + 1 == group.calls.len(),
            group.color_scheme,
        ));
    }
    lines
}

fn render_tool_call(
    call: &RenderToolCall,
    last: bool,
    scheme: ToolGroupColorScheme,
) -> Vec<Line<'static>> {
    let first_prefix = if last { "    └─ " } else { "    ├─ " };
    let continuation_prefix = if last { "       " } else { "    │  " };
    let connector_color = render_color(scheme.connector);
    let mut lines = markdown::render(&call.title);
    if lines.is_empty() {
        lines.push(Line::default());
    }
    for (index, line) in lines.iter_mut().enumerate() {
        apply_fallback_color(line, render_color(scheme.title));
        prepend_connector(
            line,
            if index == 0 {
                first_prefix
            } else {
                continuation_prefix
            },
            connector_color,
        );
    }
    if let Some(body) = &call.body {
        for mut line in markdown::render(body) {
            apply_fallback_color(&mut line, render_color(scheme.body));
            prepend_connector(&mut line, continuation_prefix, connector_color);
            lines.push(line);
        }
    }
    lines
}

fn prepend_connector(line: &mut Line<'static>, connector: &str, color: Color) {
    line.spans.insert(
        0,
        Span::styled(connector.to_owned(), Style::default().fg(color)),
    );
}

fn apply_fallback_color(line: &mut Line<'static>, color: Color) {
    for span in &mut line.spans {
        if span.style.fg.is_none() {
            span.style = span.style.fg(color);
        }
    }
}

fn render_color(color: RenderColor) -> Color {
    match color {
        RenderColor::Default => Color::Reset,
        RenderColor::Muted => shared::MUTED,
        RenderColor::Red => Color::Red,
        RenderColor::Yellow => Color::Yellow,
        RenderColor::Green => Color::Green,
        RenderColor::Cyan => Color::Cyan,
        RenderColor::Blue => Color::Blue,
        RenderColor::Magenta => Color::Magenta,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestrator::workflow::{session::WorkflowSession, workflow::WorkflowError};
    use crate::ui_interface::chat::{RenderText, RenderToolCall, RenderToolGroup};
    use std::time::{Duration, Instant};

    struct AlternateWorkflow;
    impl WorkflowSession for AlternateWorkflow {
        fn submit(
            &mut self,
            prompt: String,
            emit: &mut dyn FnMut(RenderMessageSection),
        ) -> Result<(), WorkflowError> {
            if prompt == "fail" {
                return Err(WorkflowError {
                    message: "Provider rejected request".into(),
                    retryable: false,
                });
            }
            emit(RenderMessageSection::Message {
                speaker: "Alternate".into(),
                content: RenderText::markdown(prompt),
            });
            emit(RenderMessageSection::ToolGroup(RenderToolGroup::new(
                RenderText::plain("Files"),
                vec![RenderToolCall::new(RenderText::plain("Read"))],
            )));
            Ok(())
        }
    }

    fn wait_for_idle(session: &mut Session) {
        let deadline = Instant::now() + Duration::from_secs(3);
        while session.busy && Instant::now() < deadline {
            session.poll();
            thread::sleep(Duration::from_millis(5));
        }
        assert!(!session.busy);
    }

    fn model() -> ModelConfig {
        serde_json::from_value(serde_json::json!({
            "name": "test", "host_url": "http://localhost/v1", "id": "test", "encrypted_keys": [],
            "max_input_tokens": 100, "max_output_tokens": 50
        }))
        .unwrap()
    }

    fn alternate() -> Session {
        let definition = WorkflowDefinition {
            name: "Alternate".into(),
            description: "test".into(),
            create: |_| Ok(Box::new(AlternateWorkflow)),
        };
        Session::start(definition, model(), PathBuf::from("/tmp")).unwrap()
    }

    #[test]
    fn starts_selected_factory_and_delivers_live_sections() {
        let mut session = alternate();
        wait_for_idle(&mut session);
        assert!(session.error.is_none());
        session.composer.insert("hello", true);
        session.submit();
        assert!(session.busy);
        wait_for_idle(&mut session);
        assert!(session.error.is_none());
        assert_eq!(session.sections.len(), 2);
        assert!(
            matches!(&session.sections[0], RenderMessageSection::Message { speaker, .. } if speaker == "Alternate")
        );
        assert!(matches!(
            &session.sections[1],
            RenderMessageSection::ToolGroup(_)
        ));
    }

    #[test]
    fn worker_errors_allow_recovery_without_losing_history() {
        let mut session = alternate();
        wait_for_idle(&mut session);
        session.composer.insert("hello", true);
        session.submit();
        wait_for_idle(&mut session);
        session.composer.insert("fail", true);
        session.submit();
        wait_for_idle(&mut session);
        assert_eq!(session.error.as_deref(), Some("Provider rejected request"));
        assert_eq!(session.sections.len(), 2);
        assert!(!session.disconnected);
        session.composer.insert("recover", true);
        session.submit();
        wait_for_idle(&mut session);
        assert!(session.error.is_none());
        assert_eq!(session.sections.len(), 4);
    }

    #[test]
    fn factory_failure_is_visible_and_session_layout_handles_resize() {
        let definition = WorkflowDefinition {
            name: "Unavailable".into(),
            description: "test".into(),
            create: |_| {
                Err(WorkflowError {
                    message: "Missing preset".into(),
                    retryable: false,
                })
            },
        };
        let mut session = Session::start(definition, model(), PathBuf::from("/tmp")).unwrap();
        wait_for_idle(&mut session);
        assert_eq!(session.error.as_deref(), Some("Missing preset"));
        assert!(session.disconnected);
        session
            .composer
            .insert("long draft\nwith\nseveral\nlines\nand 界🙂", true);
        for (width, height) in [(100, 32), (48, 16), (20, 5)] {
            let mut terminal =
                ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
            terminal
                .draw(|frame| session.render(frame, "/workspace"))
                .unwrap();
            let screen = terminal
                .backend()
                .buffer()
                .content
                .iter()
                .map(|cell| cell.symbol())
                .collect::<String>();
            if width >= 48 {
                assert!(screen.contains("Missing preset"));
            }
        }
    }
}
