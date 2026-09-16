use super::{
    configuration::{Configuration, LABELS, SetupPage},
    markdown,
    session::Session,
};
use crate::config::ModelConfigStore;
use crate::orchestrator::workflow::session::WorkflowDefinition;
use crate::ui_interface::chat::RenderMessageSection;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, BorderType, Borders, List, ListItem, ListState, Paragraph, Wrap},
};

const ACCENT: Color = Color::Cyan;
const MUTED: Color = Color::DarkGray;

fn block(title: impl Into<Line<'static>>, active: bool) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(if active { ACCENT } else { MUTED }))
        .title(title)
}

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
        header,
    );

    let help = match config.page {
        SetupPage::Profiles => {
            let items = store
                .configs()
                .iter()
                .map(|c| {
                    ListItem::new(vec![
                        Line::raw(markdown::clean(&format!("{}  ·  {}", c.name(), c.id()))),
                        Line::styled(markdown::clean(c.host_url()), Style::default().fg(MUTED)),
                        Line::default(),
                    ])
                })
                .collect::<Vec<_>>();
            frame.render_stateful_widget(
                List::new(items)
                    .block(block(" Saved connections ", true))
                    .highlight_style(Style::default().fg(ACCENT).add_modifier(Modifier::BOLD))
                    .highlight_symbol("› "),
                content,
                &mut ListState::default().with_selected(Some(config.selected_profile)),
            );
            "↑↓ Select · Enter Continue · n New connection · Ctrl+C Quit"
        }
        SetupPage::Form => {
            let visible = (content.height / 3).max(1) as usize;
            let start = config.focused.saturating_sub(visible - 1);
            for index in start..(start + visible).min(LABELS.len()) {
                let area = Rect::new(
                    content.x,
                    content.y + ((index - start) * 3) as u16,
                    content.width,
                    3,
                );
                let focused = config.focused == index;
                let field = &config.fields[index];
                let text = if index == 3 {
                    "•".repeat(field.text.chars().count())
                } else {
                    field.text.clone()
                };
                let column = if index == 3 {
                    field.text[..field.cursor].chars().count()
                } else {
                    field.position().1
                };
                let scroll = column
                    .saturating_sub(area.width.saturating_sub(3) as usize)
                    .min(u16::MAX as usize) as u16;
                frame.render_widget(
                    Paragraph::new(text)
                        .block(block(format!(" {} ", LABELS[index]), focused))
                        .scroll((0, scroll)),
                    area,
                );
                if focused {
                    frame.set_cursor_position((
                        area.x
                            + 1
                            + (column.saturating_sub(scroll as usize) as u16).min(area.width - 3),
                        area.y + 1,
                    ));
                }
            }
            "Tab / ↑↓ Fields · F2 Save · Esc Back · Ctrl+C Quit"
        }
        SetupPage::Workflows => {
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
                .map(|c| format!("{} · {}", c.name(), c.id()))
                .unwrap_or_default();
            frame.render_stateful_widget(
                List::new(items)
                    .block(block(
                        format!(" Workflows — {} ", markdown::clean(&connection)),
                        true,
                    ))
                    .highlight_style(Style::default().fg(ACCENT).add_modifier(Modifier::BOLD))
                    .highlight_symbol("› "),
                content,
                &mut ListState::default().with_selected(Some(config.selected_workflow)),
            );
            "↑↓ Select · Enter Start session · Esc Connections · Ctrl+C Quit"
        }
    };
    if let Some(message) = &config.error {
        frame.render_widget(
            Paragraph::new(markdown::clean(message))
                .style(Style::default().fg(Color::Red))
                .wrap(Wrap { trim: false }),
            error,
        );
    } else if matches!(config.page, SetupPage::Form) {
        frame.render_widget(Paragraph::new("API key is masked and stored encrypted. The connection is used when you send your first prompt.").style(Style::default().fg(MUTED)).wrap(Wrap { trim: false }), error);
    }
    frame.render_widget(
        Paragraph::new(help)
            .style(Style::default().fg(MUTED))
            .wrap(Wrap { trim: false }),
        footer,
    );
}

pub(super) fn transcript(sections: &[RenderMessageSection]) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    for section in sections {
        match section {
            RenderMessageSection::Message { speaker, content } => {
                lines.push(Line::styled(
                    markdown::clean(speaker),
                    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
                ));
                lines.extend(markdown::render(content));
            }
            RenderMessageSection::ToolGroup(group) => {
                for mut line in markdown::render(&group.header) {
                    line.spans
                        .insert(0, Span::styled("┌ ", Style::default().fg(MUTED)));
                    lines.push(line);
                }
                for call in &group.calls {
                    for mut line in markdown::render(&call.title) {
                        line.spans
                            .insert(0, Span::styled("│ • ", Style::default().fg(MUTED)));
                        lines.push(line);
                    }
                    if let Some(body) = &call.body {
                        for mut line in markdown::render(body) {
                            line.spans
                                .insert(0, Span::styled("│   ", Style::default().fg(MUTED)));
                            lines.push(line);
                        }
                    }
                }
                lines.push(Line::styled("└", Style::default().fg(MUTED)));
            }
        }
        lines.push(Line::default());
    }
    lines
}

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
        header,
    );
    let lines = if session.sections.is_empty() {
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
    };
    let paragraph = Paragraph::new(Text::from(lines)).wrap(Wrap { trim: false });
    let bottom = paragraph
        .line_count(history.width)
        .saturating_sub(history.height as usize);
    session.scroll = session.scroll.min(bottom);
    frame.render_widget(
        paragraph.scroll((
            bottom.saturating_sub(session.scroll).min(u16::MAX as usize) as u16,
            0,
        )),
        history,
    );
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
        status,
    );
    let (row, column) = session.composer.position();
    let vertical = row
        .saturating_sub(composer.height.saturating_sub(3) as usize)
        .min(u16::MAX as usize) as u16;
    let horizontal = column
        .saturating_sub(composer.width.saturating_sub(3) as usize)
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
        composer,
    );
    frame.set_cursor_position((
        composer.x
            + 1
            + (column.saturating_sub(horizontal as usize) as u16)
                .min(composer.width.saturating_sub(3)),
        composer.y
            + 1
            + (row.saturating_sub(vertical as usize) as u16).min(composer.height.saturating_sub(3)),
    ));
    frame.render_widget(Paragraph::new("Enter Send · Alt+Enter Newline · PgUp/PgDn Scroll\nEsc End session (when idle) · Ctrl+C Quit").style(Style::default().fg(MUTED)), footer);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui_interface::chat::{RenderText, RenderToolCall, RenderToolGroup};

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
}
