//! Represents a workflow session.

use super::configuration::Configuration;
use super::{input::Input, markdown, shared};
use crate::agent::interactions::{PromptRequest, UserInteractionBroker};
use crate::agent::presets::registry::AgentPresetRegistry;
use crate::agent::sandbox::filesystem::{BindMount, SandboxedFilesystem};
use crate::agent::steering::SteeringInbox;
use crate::config::ModelConfig;
use crate::config::ModelConfigStore;
use crate::orchestrator::workflow::session::{WorkflowContext, WorkflowDefinition};
use crate::ui_interface::chat::RenderMessageSection;
use crate::ui_interface::chat::{
    RenderColor, RenderToolCall, RenderToolGroup, ToolGroupColorScheme,
};
use crate::utils::default_ro_binds::ro_binds;
use crossterm::event::{
    Event, KeyCode, KeyEventKind, KeyModifiers, MouseEventKind,
};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Wrap},
};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::io::Write as _;
use std::{io, thread};

const COMPOSER_BG: Color = Color::Rgb(45, 45, 48);
const COMPOSER_FG: Color = Color::Rgb(235, 235, 235);
const HISTORY_SCROLL_STEP: usize = 3;

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
                super::workflow_list::WorkflowList::new(
                    self.return_to.take().unwrap(),
                ),
            )))
        } else {
            Ok(super::view::Transition::Stay)
        }
    }
}

enum Update {
    Ready(Option<SteeringInbox>),
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
    prompt_requests: Receiver<PromptRequest>,
    pending_prompt: Option<PromptRequest>,
    steering_inbox: Option<SteeringInbox>,
}

impl Session {
    /// Handles session-local input. Returns true when the view should close.
    pub(super) fn handle_event(&mut self, event: Event) -> bool {
        match event {
            Event::Paste(text) => {
                let previous_len = self.composer.text.len();
                self.composer.insert(
                    &text.replace("\r\n", "\n").replace('\r', "\n"),
                    true,
                );
                if self.composer.text.len() != previous_len {
                    self.reset_prompt_timeout();
                }
            }

            Event::Mouse(mouse) => match mouse.kind {
                MouseEventKind::ScrollUp => {
                    self.scroll =
                        self.scroll.saturating_add(HISTORY_SCROLL_STEP);
                }

                MouseEventKind::ScrollDown => {
                    self.scroll =
                        self.scroll.saturating_sub(HISTORY_SCROLL_STEP);
                }

                _ => {}
            },

            Event::Key(key) if key.kind != KeyEventKind::Release => {
                match key.code {
                    KeyCode::Esc if self.busy && !self.starting => {
                        self.stop_turn();
                    }

                    KeyCode::Enter
                    if key
                        .modifiers
                        .intersects(
                            KeyModifiers::ALT | KeyModifiers::SHIFT,
                        ) =>
                        {
                            self.composer.insert("\n", true);
                            self.reset_prompt_timeout();
                        }

                    KeyCode::Enter => {
                        self.submit();
                    }

                    KeyCode::PageUp => {
                        self.scroll = self.scroll.saturating_add(10);
                    }

                    KeyCode::PageDown => {
                        self.scroll = self.scroll.saturating_sub(10);
                    }

                    KeyCode::End
                    if key
                        .modifiers
                        .contains(KeyModifiers::CONTROL) =>
                        {
                            self.scroll = 0;
                        }

                    _ => {
                        let resets_timeout = matches!(
                            key.code,
                            KeyCode::Backspace | KeyCode::Delete
                        ) || matches!(key.code, KeyCode::Char(_))
                            && !key.modifiers.intersects(
                                KeyModifiers::CONTROL | KeyModifiers::ALT,
                            );
                        self.composer.key(key, true);
                        if resets_timeout {
                            self.reset_prompt_timeout();
                        }
                    }
                }
            }

            _ => {}
        }

        false
    }

    pub(super) fn render(
        &mut self,
        frame: &mut Frame,
        workspace: &str,
    ) {
        if shared::small_terminal(frame) {
            return;
        }

        /*
         * One empty grey row sits above the prompt.
         *
         * The composer therefore has:
         *
         *   row 0        top padding
         *   row 1        prompt / first input line
         *   row 2+       additional input lines
         */
        let composer_height =
            (self.composer.text.lines().count() + 2).clamp(3, 6) as u16;

        let prompt_height = self
            .pending_prompt
            .as_ref()
            .map_or(0, |prompt| (prompt.options.len() as u16 + 4).clamp(4, 12));

        let [header, history, status, prompt, composer, footer] =
            Layout::vertical([
                Constraint::Length(2),
                Constraint::Min(1),
                Constraint::Length(
                    if self.error.is_some() { 3 } else { 1 },
                ),
                Constraint::Length(prompt_height),
                Constraint::Length(composer_height),
                Constraint::Length(1),
            ])
                .margin(1)
                .areas(frame.area());

        self.render_header(frame, header);
        self.render_history(frame, history);
        self.render_status(frame, status);
        self.render_prompt(frame, prompt);
        self.render_composer(frame, composer);
        self.render_footer(frame, workspace, footer);
    }

    fn render_header(
        &self,
        frame: &mut Frame,
        area: Rect,
    ) {
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(
                    "✦ Cutlass",
                    Style::default()
                        .fg(shared::ACCENT)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("   "),
                Span::styled(
                    markdown::clean(&self.workflow),
                    Style::default().fg(shared::MUTED),
                ),
            ])),
            area,
        );
    }

    fn render_history(
        &mut self,
        frame: &mut Frame,
        area: Rect,
    ) {
        let lines = if self.sections.is_empty() {
            Vec::new()
        } else {
            transcript(&self.sections)
        };

        let paragraph =
            Paragraph::new(Text::from(lines))
                .wrap(Wrap { trim: false });

        let bottom = paragraph
            .line_count(area.width)
            .saturating_sub(area.height as usize);

        self.scroll = self.scroll.min(bottom);

        frame.render_widget(
            paragraph.scroll((
                bottom
                    .saturating_sub(self.scroll)
                    .min(u16::MAX as usize)
                    as u16,
                0,
            )),
            area,
        );
    }

    fn render_status(
        &self,
        frame: &mut Frame,
        area: Rect,
    ) {
        let (message, color) =
            if let Some(error) = &self.error {
                (markdown::clean(error), Color::Red)
            } else if self.starting {
                (
                    "Starting workflow…".into(),
                    shared::ACCENT,
                )
            } else if self.pending_prompt.is_some() {
                let seconds = self
                    .pending_prompt
                    .as_ref()
                    .map(|prompt| prompt.remaining().as_secs().saturating_add(1))
                    .unwrap_or_default();
                (
                    format!("Waiting for your answer… ({seconds}s of inactivity left)"),
                    shared::ACCENT,
                )
            } else if self.busy {
                (
                    format!(
                        "{} Working…",
                        [
                            "⠋", "⠙", "⠹", "⠸",
                            "⠼", "⠴", "⠦", "⠧",
                        ][self.tick / 2 % 8]
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

    fn render_composer(
        &self,
        frame: &mut Frame,
        area: Rect,
    ) {
        /*
         * Flat composer surface.
         *
         * No borders.
         * No title.
         * One blank grey line above the actual input.
         */
        frame.render_widget(
            Block::default().style(
                Style::default()
                    .bg(COMPOSER_BG)
                    .fg(COMPOSER_FG),
            ),
            area,
        );

        if area.width == 0 || area.height <= 1 {
            return;
        }

        const TOP_PADDING: u16 = 1;

        /*
         * Prompt marker.
         */
        if area.width >= 2 {
            frame.render_widget(
                Paragraph::new(
                    Span::styled(
                        "›",
                        Style::default()
                            .fg(shared::ACCENT)
                            .bg(COMPOSER_BG)
                            .add_modifier(Modifier::BOLD),
                    ),
                ),
                Rect {
                    x: area.x.saturating_add(1),
                    y: area.y.saturating_add(TOP_PADDING),
                    width: 1,
                    height: 1,
                },
            );
        }

        /*
         * Editable text starts after the prompt and spacing.
         */
        let input_area = Rect {
            x: area.x.saturating_add(3),
            y: area.y.saturating_add(TOP_PADDING),
            width: area.width.saturating_sub(4),
            height: area.height.saturating_sub(TOP_PADDING),
        };

        if input_area.width == 0 || input_area.height == 0 {
            return;
        }

        let (row, column) = self.composer.position();

        let vertical = row
            .saturating_sub(
                input_area.height.saturating_sub(1) as usize,
            )
            .min(u16::MAX as usize)
            as u16;

        let horizontal = column
            .saturating_sub(
                input_area.width.saturating_sub(1) as usize,
            )
            .min(u16::MAX as usize)
            as u16;

        /*
         * Empty-input placeholder.
         *
         * Terminals do not support true alpha transparency, so shared::MUTED
         * provides the subdued/translucent visual treatment.
         */
        if self.composer.text.is_empty() {
            let placeholder = if self.pending_prompt.is_some() {
                "Type your answer here."
            } else {
                "Ask a question or describe a change to your workspace."
            };
            frame.render_widget(
                Paragraph::new(
                    placeholder,
                )
                    .style(
                        Style::default()
                            .fg(shared::MUTED)
                            .bg(COMPOSER_BG),
                    ),
                input_area,
            );
        } else {
            frame.render_widget(
                Paragraph::new(self.composer.text.as_str())
                    .style(
                        Style::default()
                            .fg(COMPOSER_FG)
                            .bg(COMPOSER_BG),
                    )
                    .scroll((vertical, horizontal)),
                input_area,
            );
        }

        let cursor_x = input_area.x
            + (column.saturating_sub(horizontal as usize) as u16)
            .min(input_area.width.saturating_sub(1));

        let cursor_y = input_area.y
            + (row.saturating_sub(vertical as usize) as u16)
            .min(input_area.height.saturating_sub(1));

        frame.set_cursor_position((cursor_x, cursor_y));
    }

    fn render_prompt(&self, frame: &mut Frame, area: Rect) {
        let Some(prompt) = &self.pending_prompt else {
            return;
        };
        if area.width == 0 || area.height == 0 {
            return;
        }

        let mut lines = vec![Line::styled(
            markdown::clean(&prompt.question),
            Style::default().fg(COMPOSER_FG).add_modifier(Modifier::BOLD),
        )];
        if prompt.options.is_empty() {
            lines.push(Line::styled(
                "Type a response below.",
                Style::default().fg(shared::MUTED),
            ));
        } else {
            for (index, option) in prompt.options.iter().enumerate() {
                let description = option
                    .description
                    .as_deref()
                    .map(|value| format!(" — {}", markdown::clean(value)))
                    .unwrap_or_default();
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("{}. ", index + 1),
                        Style::default().fg(shared::ACCENT),
                    ),
                    Span::raw(format!("{}{}", markdown::clean(&option.label), description)),
                ]));
            }
            lines.push(Line::styled(
                "Enter a number or type a custom response.",
                Style::default().fg(shared::MUTED),
            ));
        }

        frame.render_widget(
            Paragraph::new(lines)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" Question ")
                        .border_style(Style::default().fg(shared::ACCENT)),
                )
                .wrap(Wrap { trim: false }),
            area,
        );
    }

    fn render_footer(
        &self,
        frame: &mut Frame,
        workspace: &str,
        area: Rect,
    ) {
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(
                    markdown::clean(&self.model),
                    Style::default().fg(shared::MUTED),
                ),
                Span::styled(
                    " · ",
                    Style::default().fg(shared::MUTED),
                ),
                Span::styled(
                    markdown::clean(workspace),
                    Style::default().fg(shared::MUTED),
                ),
            ])),
            area,
        );
    }

    pub fn start(
        definition: WorkflowDefinition,
        model: ModelConfig,
        workspace: PathBuf,
    ) -> io::Result<Self> {
        let (commands, requests) =
            mpsc::channel::<String>();

        let (output, updates) = mpsc::channel();
        let (user_interactions, prompt_requests) =
            UserInteractionBroker::channel();

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
                        vec![
                            BindMount::path(workspace),
                        ],
                    ),
                    model: &model,
                    agents: &agents,
                    user_interactions: Some(user_interactions),
                };

                let mut workflow =
                    match (definition.create)(context) {
                        Ok(workflow) => workflow,

                        Err(error) => {
                            let _ = output.send(
                                Update::Failed(
                                    error.message,
                                ),
                            );
                            return;
                        }
                    };

                if output
                    .send(Update::Ready(
                        workflow.steering_inbox(),
                    ))
                    .is_err()
                {
                    return;
                }

                while let Ok(prompt) = requests.recv() {
                    let result = workflow.submit(
                        prompt,
                        &mut |section| {
                            let _ = output.send(
                                Update::Section(section),
                            );
                        },
                    );

                    if output
                        .send(Update::Finished(
                            result.map_err(
                                |e| e.message,
                            ),
                        ))
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
            prompt_requests,
            pending_prompt: None,
            steering_inbox: None,
        })
    }

    pub fn poll(&mut self) {
        self.tick = self.tick.wrapping_add(1);

        if self.disconnected {
            return;
        }

        while let Ok(request) = self.prompt_requests.try_recv() {
            if let Some(previous) = self.pending_prompt.replace(request) {
                previous.dismiss();
            }
            self.scroll = 0;
        }

        if self
            .pending_prompt
            .as_ref()
            .is_some_and(|request| request.remaining().is_zero())
        {
            self.pending_prompt.take();
        }

        if self
            .pending_prompt
            .as_mut()
            .is_some_and(PromptRequest::take_reminder_due)
        {
            terminal_bell();
        }

        loop {
            match self.updates.try_recv() {
                Ok(Update::Ready(steering_inbox)) => {
                    self.steering_inbox =
                        steering_inbox;

                    self.starting = false;
                    self.busy = false;
                }

                Ok(Update::Section(section)) => {
                    self.sections.push(section);
                }

                Ok(Update::Finished(result)) => {
                    if let Some(prompt) = self.pending_prompt.take() {
                        prompt.dismiss();
                    }
                    self.busy = false;
                    self.error = result.err();
                }

                Ok(Update::Failed(error)) => {
                    if let Some(prompt) = self.pending_prompt.take() {
                        prompt.dismiss();
                    }
                    self.error = Some(error);
                    self.disconnected = true;
                    self.busy = false;
                    self.starting = false;
                    break;
                }

                Err(TryRecvError::Empty) => {
                    break;
                }

                Err(TryRecvError::Disconnected) => {
                    self.error = Some(
                        "The workflow worker stopped. Press Ctrl+C to quit."
                            .into(),
                    );

                    self.disconnected = true;
                    self.busy = false;
                    self.starting = false;
                    break;
                }
            }
        }
    }

    pub fn submit(&mut self) {
        if self.disconnected
            || self.composer.text.trim().is_empty()
        {
            return;
        }

        if let Some(prompt) = self.pending_prompt.take() {
            let answer = self.composer.take();
            prompt.respond(answer);
            self.scroll = 0;
            return;
        }

        if self.busy {
            if self.starting {
                return;
            }

            if let Some(inbox) =
                &self.steering_inbox
            {
                inbox.add_message(
                    self.composer.take(),
                );

                self.scroll = 0;
            }

            return;
        }

        if self
            .commands
            .send(self.composer.text.clone())
            .is_ok()
        {
            self.composer.take();
            self.busy = true;
            self.error = None;
            self.scroll = 0;
        } else {
            self.error = Some(
                "The workflow is no longer running."
                    .into(),
            );

            self.disconnected = true;
        }
    }

    fn stop_turn(&mut self) {
        if let Some(prompt) = self.pending_prompt.take() {
            prompt.dismiss();
        }
        if let Some(inbox) = &self.steering_inbox {
            inbox.cancel_turn();
        }
    }

    fn reset_prompt_timeout(&mut self) {
        if let Some(prompt) = self.pending_prompt.as_mut() {
            prompt.activity();
        }
    }
}

fn terminal_bell() {
    let mut stdout = io::stdout();
    let _ = stdout.write_all(b"\x07");
    let _ = stdout.flush();
}

fn transcript(
    sections: &[RenderMessageSection],
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();

    for section in sections {
        match section {
            RenderMessageSection::Message {
                speaker,
                content,
            } => {
                lines.push(
                    Line::styled(
                        markdown::clean(speaker),
                        Style::default()
                            .fg(shared::ACCENT)
                            .add_modifier(
                                Modifier::BOLD,
                            ),
                    ),
                );

                lines.extend(
                    markdown::render(content),
                );
            }

            RenderMessageSection::ToolGroup(
                group,
            ) => {
                lines.extend(
                    render_tool_group(group),
                );
            }
        }

        lines.push(Line::default());
    }

    lines
}

fn render_tool_group(
    group: &RenderToolGroup,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();

    for mut line in markdown::render(
        &group.header,
    ) {
        apply_fallback_color(
            &mut line,
            render_color(
                group.color_scheme.header,
            ),
        );

        lines.push(line);
    }

    for (index, call) in
        group.calls.iter().enumerate()
    {
        lines.extend(
            render_tool_call(
                call,
                index + 1
                    == group.calls.len(),
                group.color_scheme,
            ),
        );
    }

    lines
}

fn render_tool_call(
    call: &RenderToolCall,
    last: bool,
    scheme: ToolGroupColorScheme,
) -> Vec<Line<'static>> {
    let first_prefix = if last {
        "    └─ "
    } else {
        "    ├─ "
    };

    let continuation_prefix = if last {
        "       "
    } else {
        "    │  "
    };

    let connector_color =
        render_color(scheme.connector);

    let mut lines =
        markdown::render(&call.title);

    if lines.is_empty() {
        lines.push(Line::default());
    }

    for (index, line) in
        lines.iter_mut().enumerate()
    {
        apply_fallback_color(
            line,
            render_color(scheme.title),
        );

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
        for mut line in
            markdown::render(body)
        {
            apply_fallback_color(
                &mut line,
                render_color(scheme.body),
            );

            prepend_connector(
                &mut line,
                continuation_prefix,
                connector_color,
            );

            lines.push(line);
        }
    }

    lines
}

fn prepend_connector(
    line: &mut Line<'static>,
    connector: &str,
    color: Color,
) {
    line.spans.insert(
        0,
        Span::styled(
            connector.to_owned(),
            Style::default().fg(color),
        ),
    );
}

fn apply_fallback_color(
    line: &mut Line<'static>,
    color: Color,
) {
    for span in &mut line.spans {
        if span.style.fg.is_none() {
            span.style =
                span.style.fg(color);
        }
    }
}

fn render_color(
    color: RenderColor,
) -> Color {
    match color {
        RenderColor::Default => {
            Color::Reset
        }
        RenderColor::Muted => {
            shared::MUTED
        }
        RenderColor::Red => {
            Color::Red
        }
        RenderColor::Yellow => {
            Color::Yellow
        }
        RenderColor::Green => {
            Color::Green
        }
        RenderColor::Cyan => {
            Color::Cyan
        }
        RenderColor::Blue => {
            Color::Blue
        }
        RenderColor::Magenta => {
            Color::Magenta
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::steering::SteeringInbox;
    use crate::orchestrator::workflow::{
        session::WorkflowSession,
        workflow::WorkflowError,
    };
    use crate::ui_interface::chat::{
        RenderText,
        RenderToolCall,
        RenderToolGroup,
    };
    use crossterm::event::{
        KeyModifiers,
        MouseEvent,
    };
    use std::time::{
        Duration,
        Instant,
    };

    struct AlternateWorkflow;

    impl WorkflowSession
    for AlternateWorkflow
    {
        fn submit(
            &mut self,
            prompt: String,
            emit: &mut dyn FnMut(
                RenderMessageSection,
            ),
        ) -> Result<(), WorkflowError> {
            if prompt == "fail" {
                return Err(
                    WorkflowError {
                        message:
                        "Provider rejected request"
                            .into(),
                        retryable: false,
                    },
                );
            }

            emit(
                RenderMessageSection::Message {
                    speaker:
                    "Alternate".into(),
                    content:
                    RenderText::markdown(
                        prompt,
                    ),
                },
            );

            emit(
                RenderMessageSection::ToolGroup(
                    RenderToolGroup::new(
                        RenderText::plain(
                            "Files",
                        ),
                        vec![
                            RenderToolCall::new(
                                RenderText::plain(
                                    "Read",
                                ),
                            ),
                        ],
                    ),
                ),
            );

            Ok(())
        }
    }

    struct ControllableWorkflow {
        inbox: SteeringInbox,
    }

    struct PromptingWorkflow {
        broker: UserInteractionBroker,
    }

    impl WorkflowSession for PromptingWorkflow {
        fn submit(
            &mut self,
            _: String,
            emit: &mut dyn FnMut(RenderMessageSection),
        ) -> Result<(), WorkflowError> {
            let answer = self.broker.ask(
                "Choose a path".into(),
                vec![
                    crate::agent::interactions::PromptOption::new(
                        "Fast",
                        Some("Less validation".into()),
                    ),
                    crate::agent::interactions::PromptOption::new(
                        "Safe",
                        Some("More validation".into()),
                    ),
                ],
                Duration::from_secs(2),
            );
            emit(RenderMessageSection::Message {
                speaker: "Prompt test".into(),
                content: RenderText::plain(answer.unwrap_or_else(|| "unavailable".into())),
            });
            Ok(())
        }
    }

    impl WorkflowSession
    for ControllableWorkflow
    {
        fn submit(
            &mut self,
            _: String,
            emit: &mut dyn FnMut(
                RenderMessageSection,
            ),
        ) -> Result<(), WorkflowError> {
            let deadline =
                Instant::now()
                    + Duration::from_secs(2);

            while Instant::now()
                < deadline
            {
                if self
                    .inbox
                    .end_turn_called()
                {
                    self.inbox
                        .acknowledge_end_turn();

                    return Ok(());
                }

                if let Some(message) = self
                    .inbox
                    .drain_steering_message()
                {
                    emit(
                        RenderMessageSection::Message {
                            speaker:
                            "Steering".into(),
                            content:
                            RenderText::plain(
                                message,
                            ),
                        },
                    );

                    return Ok(());
                }

                thread::sleep(
                    Duration::from_millis(5),
                );
            }

            Err(WorkflowError {
                message:
                "Timed out waiting for session control"
                    .into(),
                retryable: false,
            })
        }

        fn steering_inbox(
            &self,
        ) -> Option<SteeringInbox> {
            Some(self.inbox.clone())
        }
    }

    fn wait_for_idle(
        session: &mut Session,
    ) {
        let deadline =
            Instant::now()
                + Duration::from_secs(3);

        while session.busy
            && Instant::now() < deadline
        {
            session.poll();

            thread::sleep(
                Duration::from_millis(5),
            );
        }

        assert!(!session.busy);
    }

    fn model() -> ModelConfig {
        serde_json::from_value(
            serde_json::json!({
                "name": "test",
                "host_url": "http://localhost/v1",
                "id": "test",
                "encrypted_keys": [],
                "max_input_tokens": 100,
                "max_output_tokens": 50
            }),
        )
            .unwrap()
    }

    fn alternate() -> Session {
        let definition =
            WorkflowDefinition {
                name:
                "Alternate".into(),
                description:
                "test".into(),
                create: |_| {
                    Ok(Box::new(
                        AlternateWorkflow,
                    ))
                },
            };

        Session::start(
            definition,
            model(),
            PathBuf::from("/tmp"),
        )
            .unwrap()
    }

    fn controllable() -> Session {
        let definition =
            WorkflowDefinition {
                name:
                "Controllable".into(),
                description:
                "test".into(),
                create: |_| {
                    Ok(Box::new(
                        ControllableWorkflow {
                            inbox:
                            SteeringInbox::new(),
                        },
                    ))
                },
            };

        Session::start(
            definition,
            model(),
            PathBuf::from("/tmp"),
        )
            .unwrap()
    }

    fn prompting() -> Session {
        let definition = WorkflowDefinition {
            name: "Prompting".into(),
            description: "test".into(),
            create: |context| {
                Ok(Box::new(PromptingWorkflow {
                    broker: context.user_interactions.unwrap(),
                }))
            },
        };

        Session::start(definition, model(), PathBuf::from("/tmp")).unwrap()
    }

    #[test]
    fn routes_prompt_answers_without_turning_them_into_steering() {
        let mut session = prompting();
        wait_for_idle(&mut session);
        session.composer.insert("begin", true);
        session.submit();

        let deadline = Instant::now() + Duration::from_secs(1);
        while session.pending_prompt.is_none() && Instant::now() < deadline {
            session.poll();
            thread::sleep(Duration::from_millis(5));
        }

        let prompt = session.pending_prompt.as_ref().expect("pending prompt");
        assert_eq!(prompt.question, "Choose a path");
        assert_eq!(prompt.options[1].label, "Safe");
        let initial_expiration = prompt.expires_at().unwrap();

        thread::sleep(Duration::from_millis(5));
        session.handle_event(Event::Key(KeyCode::Char('x').into()));
        assert!(
            session
                .pending_prompt
                .as_ref()
                .unwrap()
                .expires_at()
                .unwrap()
                > initial_expiration
        );
        session.handle_event(Event::Key(KeyCode::Backspace.into()));

        let mut terminal = ratatui::Terminal::new(
            ratatui::backend::TestBackend::new(80, 24),
        )
            .unwrap();
        terminal
            .draw(|frame| session.render(frame, "/workspace"))
            .unwrap();
        let screen = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(screen.contains("Question"));
        assert!(screen.contains("Choose a path"));
        assert!(screen.contains("1. Fast — Less validation"));
        assert!(screen.contains("custom response"));

        session.composer.insert("2", true);
        session.submit();
        assert!(session.pending_prompt.is_none());
        wait_for_idle(&mut session);

        assert!(matches!(
            session.sections.last(),
            Some(RenderMessageSection::Message { content: RenderText::Plain(answer), .. })
                if answer == "2"
        ));
    }

    #[test]
    fn starts_selected_factory_and_delivers_live_sections(
    ) {
        let mut session = alternate();

        wait_for_idle(&mut session);

        assert!(session.error.is_none());

        session.composer.insert(
            "hello",
            true,
        );

        session.submit();

        assert!(session.busy);

        wait_for_idle(&mut session);

        assert!(session.error.is_none());

        assert_eq!(
            session.sections.len(),
            2,
        );

        assert!(
            matches!(
                &session.sections[0],
                RenderMessageSection::Message {
                    speaker,
                    ..
                } if speaker == "Alternate"
            )
        );

        assert!(matches!(
            &session.sections[1],
            RenderMessageSection::ToolGroup(_)
        ));
    }

    #[test]
    fn worker_errors_allow_recovery_without_losing_history(
    ) {
        let mut session = alternate();

        wait_for_idle(&mut session);

        session.composer.insert(
            "hello",
            true,
        );

        session.submit();

        wait_for_idle(&mut session);

        session.composer.insert(
            "fail",
            true,
        );

        session.submit();

        wait_for_idle(&mut session);

        assert_eq!(
            session.error.as_deref(),
            Some(
                "Provider rejected request",
            ),
        );

        assert_eq!(
            session.sections.len(),
            2,
        );

        assert!(!session.disconnected);

        session.composer.insert(
            "recover",
            true,
        );

        session.submit();

        wait_for_idle(&mut session);

        assert!(session.error.is_none());

        assert_eq!(
            session.sections.len(),
            4,
        );
    }

    #[test]
    fn enter_during_a_turn_appends_to_the_steering_inbox(
    ) {
        let mut session =
            controllable();

        wait_for_idle(&mut session);

        session.composer.insert(
            "start",
            true,
        );

        session.submit();

        session.composer.insert(
            "change direction",
            true,
        );

        assert!(
            !session.handle_event(
                Event::Key(
                    KeyCode::Enter.into(),
                ),
            )
        );

        assert!(
            session
                .composer
                .text
                .is_empty()
        );

        wait_for_idle(&mut session);

        assert!(session.error.is_none());

        assert_eq!(
            session.sections.len(),
            1,
        );
    }

    #[test]
    fn escape_during_a_turn_stops_it_without_closing_the_session(
    ) {
        let mut session =
            controllable();

        wait_for_idle(&mut session);

        session.composer.insert(
            "start",
            true,
        );

        session.submit();

        assert!(
            !session.handle_event(
                Event::Key(
                    KeyCode::Esc.into(),
                ),
            )
        );

        wait_for_idle(&mut session);

        assert!(session.error.is_none());

        assert!(
            session.sections.is_empty()
        );
    }

    #[test]
    fn escape_no_longer_closes_an_idle_session(
    ) {
        let mut session = alternate();

        wait_for_idle(&mut session);

        assert!(
            !session.handle_event(
                Event::Key(
                    KeyCode::Esc.into(),
                ),
            )
        );
    }

    #[test]
    fn transcript_scroll_uses_page_keys(
    ) {
        let mut session = alternate();

        wait_for_idle(&mut session);

        session.scroll = 5;

        session.handle_event(
            Event::Key(
                KeyCode::PageUp.into(),
            ),
        );

        assert_eq!(
            session.scroll,
            15,
        );

        session.handle_event(
            Event::Key(
                KeyCode::PageDown.into(),
            ),
        );

        assert_eq!(
            session.scroll,
            5,
        );
    }

    #[test]
    fn transcript_scroll_uses_mouse_wheel(
    ) {
        let mut session = alternate();

        wait_for_idle(&mut session);

        session.scroll = 5;

        session.handle_event(
            Event::Mouse(MouseEvent {
                kind:
                MouseEventKind::ScrollUp,
                column: 0,
                row: 0,
                modifiers:
                KeyModifiers::NONE,
            }),
        );

        assert_eq!(
            session.scroll,
            5 + HISTORY_SCROLL_STEP,
        );

        session.handle_event(
            Event::Mouse(MouseEvent {
                kind:
                MouseEventKind::ScrollDown,
                column: 0,
                row: 0,
                modifiers:
                KeyModifiers::NONE,
            }),
        );

        assert_eq!(
            session.scroll,
            5,
        );
    }

    #[test]
    fn factory_failure_is_visible_and_session_layout_handles_resize(
    ) {
        let definition =
            WorkflowDefinition {
                name:
                "Unavailable".into(),
                description:
                "test".into(),
                create: |_| {
                    Err(WorkflowError {
                        message:
                        "Missing preset"
                            .into(),
                        retryable: false,
                    })
                },
            };

        let mut session =
            Session::start(
                definition,
                model(),
                PathBuf::from("/tmp"),
            )
                .unwrap();

        wait_for_idle(&mut session);

        assert_eq!(
            session.error.as_deref(),
            Some("Missing preset"),
        );

        assert!(session.disconnected);

        session.composer.insert(
            "long draft\nwith\nseveral\nlines\nand 界🙂",
            true,
        );

        for (width, height) in [
            (100, 32),
            (48, 16),
            (20, 5),
        ] {
            let mut terminal =
                ratatui::Terminal::new(
                    ratatui::backend::TestBackend::new(
                        width,
                        height,
                    ),
                )
                    .unwrap();

            terminal
                .draw(|frame| {
                    session.render(
                        frame,
                        "/workspace",
                    );
                })
                .unwrap();

            let screen = terminal
                .backend()
                .buffer()
                .content
                .iter()
                .map(|cell| cell.symbol())
                .collect::<String>();

            if width >= 48 {
                assert!(
                    screen.contains(
                        "Missing preset",
                    )
                );

                assert!(
                    screen.contains(
                        "test · /workspace",
                    )
                );
            }
        }
    }
}
