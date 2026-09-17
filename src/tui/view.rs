use super::{
    configuration::{Configuration, LABELS, SetupPage},
    markdown,
    session::Session,
};
use crate::config::ModelConfigStore;
use crate::orchestrator::workflow::session::WorkflowDefinition;
use crate::ui_interface::chat::{
    RenderColor, RenderMessageSection, RenderText, RenderToolCall, RenderToolGroup,
    ToolGroupColorScheme,
};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, BorderType, Borders, List, ListItem, ListState, Paragraph, Wrap},
};

const ACCENT: Color = Color::Cyan;
const MUTED: Color = Color::DarkGray;

/// Builds the rounded border shared by focused and unfocused input regions.
fn block(title: impl Into<Line<'static>>, active: bool) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(if active { ACCENT } else { MUTED }))
        .title(title)
}

/// Replaces the current view with a minimum-size notice when the terminal is too small.
pub(super) fn small_terminal(frame: &mut Frame) -> bool {
    if frame.area().width < 48 || frame.area().height < 16 {
        frame.render_widget(
            Paragraph::new("Cutlass — enlarge the terminal to at least 48 × 16. Ctrl+C to exit.")
                .wrap(Wrap { trim: false }),
            frame.area(),
        );
        true
    } else {
        false
    }
}

/// Renders the connection setup or workflow selection page.
pub(super) fn configuration(
    frame: &mut Frame,
    config: &Configuration,
    store: &ModelConfigStore,
    workflows: &[WorkflowDefinition],
) {
    if small_terminal(frame) {
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

    render_configuration_header(frame, config, header);
    let help = render_configuration_content(frame, config, store, workflows, content);
    render_configuration_feedback(frame, config, error);
    render_help(frame, help, footer);
}

/// Renders the setup title and indicates which setup phase is active.
fn render_configuration_header(frame: &mut Frame, config: &Configuration, area: Rect) {
    let phase = if matches!(config.page, SetupPage::Workflows) {
        "2 / 2 · Choose a workflow"
    } else {
        "1 / 2 · Connection"
    };
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(vec![
                Span::styled(
                    "✦ Cutlass",
                    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
                ),
                Span::raw(format!("    {phase}")),
            ]),
            Line::styled(
                "Configure your workspace, then start a conversation.",
                Style::default().fg(MUTED),
            ),
        ]),
        area,
    );
}

/// Renders the active setup page and returns its contextual key bindings.
fn render_configuration_content(
    frame: &mut Frame,
    config: &Configuration,
    store: &ModelConfigStore,
    workflows: &[WorkflowDefinition],
    area: Rect,
) -> &'static str {
    match config.page {
        SetupPage::Profiles => {
            render_profile_picker(frame, config, store, area);
            "↑↓ Select · Enter Continue · n New connection · d Delete · ctrl+C Quit"
        }
        SetupPage::Form => {
            render_connection_form(frame, config, area);
            "Tab / ↑↓ Fields · F2 Save · Esc Back · Ctrl+C Quit"
        }
        SetupPage::Workflows => {
            render_workflow_picker(frame, config, store, workflows, area);
            "↑↓ Select · Enter Start session · Esc Connections · Ctrl+C Quit"
        }
    }
}

/// Renders saved model connections and highlights the selected profile.
fn render_profile_picker(
    frame: &mut Frame,
    config: &Configuration,
    store: &ModelConfigStore,
    area: Rect,
) {
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
                    Style::default().fg(MUTED),
                ),
                Line::default(),
            ])
        })
        .collect::<Vec<_>>();

    frame.render_stateful_widget(
        List::new(items)
            .block(block(" Saved connections ", true))
            .highlight_style(Style::default().fg(ACCENT).add_modifier(Modifier::BOLD))
            .highlight_symbol("› "),
        area,
        &mut ListState::default().with_selected(Some(config.selected_profile)),
    );
}

/// Renders the visible connection fields and places the terminal cursor in the focused field.
fn render_connection_form(frame: &mut Frame, config: &Configuration, area: Rect) {
    let visible = (area.height / 3).max(1) as usize;
    let start = config.focused.saturating_sub(visible - 1);

    for index in start..(start + visible).min(LABELS.len()) {
        let field_area = Rect::new(area.x, area.y + ((index - start) * 3) as u16, area.width, 3);
        render_connection_field(frame, config, index, field_area);
    }
}

/// Renders one connection field, masking the API key and horizontally following its cursor.
fn render_connection_field(frame: &mut Frame, config: &Configuration, index: usize, area: Rect) {
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
            .block(block(format!(" {} ", LABELS[index]), focused))
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

/// Renders available workflows for the currently active model connection.
fn render_workflow_picker(
    frame: &mut Frame,
    config: &Configuration,
    store: &ModelConfigStore,
    workflows: &[WorkflowDefinition],
    area: Rect,
) {
    let items = workflows
        .iter()
        .map(|workflow| {
            ListItem::new(vec![
                Line::raw(workflow.name.clone()),
                Line::styled(workflow.description.clone(), Style::default().fg(MUTED)),
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
            .block(block(
                format!(" Workflows — {} ", markdown::clean(&connection)),
                true,
            ))
            .highlight_style(Style::default().fg(ACCENT).add_modifier(Modifier::BOLD))
            .highlight_symbol("› "),
        area,
        &mut ListState::default().with_selected(Some(config.selected_workflow)),
    );
}

/// Renders configuration errors or the API-key storage notice beneath the active page.
fn render_configuration_feedback(frame: &mut Frame, config: &Configuration, area: Rect) {
    if let Some(message) = &config.error {
        frame.render_widget(
            Paragraph::new(markdown::clean(message))
                .style(Style::default().fg(Color::Red))
                .wrap(Wrap { trim: false }),
            area,
        );
    } else if matches!(config.page, SetupPage::Form) {
        frame.render_widget(
            Paragraph::new("API key is masked and stored encrypted. The connection is used when you send your first prompt.")
                .style(Style::default().fg(MUTED))
                .wrap(Wrap { trim: false }),
            area,
        );
    }
}

/// Renders contextual keyboard shortcuts in a muted footer.
fn render_help(frame: &mut Frame, help: &str, area: Rect) {
    frame.render_widget(
        Paragraph::new(help)
            .style(Style::default().fg(MUTED))
            .wrap(Wrap { trim: false }),
        area,
    );
}
/// Converts the ordered conversation sections into terminal lines.
pub(super) fn transcript(sections: &[RenderMessageSection]) -> Vec<Line<'static>> {
    let mut lines = Vec::new();

    for section in sections {
        match section {
            RenderMessageSection::Message { speaker, content } => {
                lines.extend(render_message(speaker, content));
            }
            RenderMessageSection::ToolGroup(group) => {
                lines.extend(render_tool_group(group));
            }
        }

        lines.push(Line::default());
    }

    lines
}

/// Renders a speaker label followed by its plain-text or Markdown content.
fn render_message(speaker: &str, content: &RenderText) -> Vec<Line<'static>> {
    let mut lines = vec![Line::styled(
        markdown::clean(speaker),
        Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
    )];
    lines.extend(markdown::render(content));
    lines
}

/// Renders tool groups:
///
/// ```terminaloutput
/// <Tool group name/title>
///     ├─ Read <path> (X lines)
///     └─ Rendered tree at <path> (X results, Y depth)
/// ```
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

/// Renders one tool call with a tree connector, Markdown title, and optional body.
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

/// Prepends one structural tree connector without altering the content style.
fn prepend_connector(line: &mut Line<'static>, connector: &str, color: Color) {
    line.spans.insert(
        0,
        Span::styled(connector.to_owned(), Style::default().fg(color)),
    );
}

/// Applies a group colour only to spans that do not already carry Markdown colouring.
fn apply_fallback_color(line: &mut Line<'static>, color: Color) {
    for span in &mut line.spans {
        if span.style.fg.is_none() {
            span.style = span.style.fg(color);
        }
    }
}

/// Maps the frontend-neutral render palette to Ratatui terminal colours.
fn render_color(color: RenderColor) -> Color {
    match color {
        RenderColor::Default => Color::Reset,
        RenderColor::Muted => MUTED,
        RenderColor::Red => Color::Red,
        RenderColor::Yellow => Color::Yellow,
        RenderColor::Green => Color::Green,
        RenderColor::Cyan => Color::Cyan,
        RenderColor::Blue => Color::Blue,
        RenderColor::Magenta => Color::Magenta,
    }
}

/// Renders an active workflow session and delegates each region to its domain renderer.
pub(super) fn session(frame: &mut Frame, session: &mut Session, workspace: &str) {
    if small_terminal(frame) {
        return;
    }
    let composer_height = (session.composer.text.lines().count() + 1).clamp(2, 5) as u16 + 2;
    let [header, history, status, composer, footer] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(1),
        Constraint::Length(if session.error.is_some() { 3 } else { 1 }),
        Constraint::Length(composer_height),
        Constraint::Length(2),
    ])
    .margin(1)
    .areas(frame.area());

    render_session_header(frame, session, workspace, header);
    render_session_history(frame, session, history);
    render_session_status(frame, session, status);
    render_session_composer(frame, session, composer);
    render_session_footer(frame, footer);
}

/// Renders the active workflow, model, and workspace identity.
fn render_session_header(frame: &mut Frame, session: &Session, workspace: &str, area: Rect) {
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(vec![
                Span::styled(
                    "✦ Cutlass",
                    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
                ),
                Span::raw(format!(
                    "   {} · {}",
                    markdown::clean(&session.workflow),
                    markdown::clean(&session.model)
                )),
            ]),
            Line::styled(markdown::clean(workspace), Style::default().fg(MUTED)),
        ]),
        area,
    );
}

/// Renders conversation history and keeps its bottom-relative scroll position valid.
fn render_session_history(frame: &mut Frame, session: &mut Session, area: Rect) {
    let paragraph =
        Paragraph::new(Text::from(session_history_lines(session))).wrap(Wrap { trim: false });
    let bottom = paragraph
        .line_count(area.width)
        .saturating_sub(area.height as usize);
    session.scroll = session.scroll.min(bottom);

    frame.render_widget(
        paragraph.scroll((
            bottom.saturating_sub(session.scroll).min(u16::MAX as usize) as u16,
            0,
        )),
        area,
    );
}

/// Produces either the session transcript or the empty-session invitation.
fn session_history_lines(session: &Session) -> Vec<Line<'static>> {
    if session.sections.is_empty() {
        vec![
            Line::default(),
            Line::raw("What would you like to work on?"),
            Line::styled(
                "Ask a question or describe a change to your workspace.",
                Style::default().fg(MUTED),
            ),
        ]
    } else {
        transcript(&session.sections)
    }
}

/// Renders the current workflow state, including errors and the animated busy indicator.
fn render_session_status(frame: &mut Frame, session: &Session, area: Rect) {
    let (message, color) = if let Some(error) = &session.error {
        (markdown::clean(error), Color::Red)
    } else if session.starting {
        ("Starting workflow…".into(), ACCENT)
    } else if session.busy {
        (
            format!(
                "{} Working…",
                ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧"][session.tick / 2 % 8]
            ),
            ACCENT,
        )
    } else {
        ("Ready".into(), MUTED)
    };
    frame.render_widget(
        Paragraph::new(message)
            .style(Style::default().fg(color))
            .wrap(Wrap { trim: false }),
        area,
    );
}

/// Renders the prompt editor, maintains its viewport, and places the terminal cursor.
fn render_session_composer(frame: &mut Frame, session: &Session, area: Rect) {
    let (row, column) = session.composer.position();
    let vertical = row
        .saturating_sub(area.height.saturating_sub(3) as usize)
        .min(u16::MAX as usize) as u16;
    let horizontal = column
        .saturating_sub(area.width.saturating_sub(3) as usize)
        .min(u16::MAX as usize) as u16;
    frame.render_widget(
        Paragraph::new(session.composer.text.as_str())
            .block(block(
                if session.busy {
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
            + (column.saturating_sub(horizontal as usize) as u16).min(area.width.saturating_sub(3)),
        area.y
            + 1
            + (row.saturating_sub(vertical as usize) as u16).min(area.height.saturating_sub(3)),
    ));
}

/// Renders the session-specific keyboard shortcuts.
fn render_session_footer(frame: &mut Frame, area: Rect) {
    render_help(
        frame,
        "Enter Send · Alt+Enter Newline · PgUp/PgDn Scroll\nEsc End session (when idle) · Ctrl+C Quit",
        area,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui_interface::chat::{
        RenderText, RenderToolCall, RenderToolGroup, ToolGroupColorScheme,
    };

    #[test]
    fn renders_group_header_once_and_keeps_call_bodies() {
        let sections = vec![RenderMessageSection::ToolGroup(RenderToolGroup::new(
            RenderText::markdown("**Files**"),
            vec![
                RenderToolCall::new(RenderText::plain("first"))
                    .with_body(RenderText::markdown("body **one**")),
                RenderToolCall::new(RenderText::plain("second")),
            ],
        ))];
        let text = transcript(&sections)
            .iter()
            .map(Line::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(text.matches("Files").count(), 1);
        assert!(text.contains("body one"));
        assert!(text.find("first").unwrap() < text.find("second").unwrap());
    }

    #[test]
    fn renders_group_structure_with_its_declared_colors() {
        let group = RenderToolGroup::new(
            RenderText::plain("Explored"),
            vec![
                RenderToolCall::new(RenderText::plain("first"))
                    .with_body(RenderText::plain("details")),
                RenderToolCall::new(RenderText::plain("last")),
            ],
        )
        .with_color_scheme(ToolGroupColorScheme {
            header: RenderColor::Magenta,
            connector: RenderColor::Blue,
            title: RenderColor::Green,
            body: RenderColor::Yellow,
        });

        let lines = render_tool_group(&group);

        assert_eq!(lines[0].spans[0].style.fg, Some(Color::Magenta));
        assert_eq!(lines[1].spans[0].content, "    ├─ ");
        assert_eq!(lines[1].spans[0].style.fg, Some(Color::Blue));
        assert_eq!(lines[1].spans[1].style.fg, Some(Color::Green));
        assert_eq!(lines[2].spans[0].style.fg, Some(Color::Blue));
        assert_eq!(lines[2].spans[1].style.fg, Some(Color::Yellow));
        assert_eq!(lines[3].spans[0].content, "    └─ ");
    }
}
