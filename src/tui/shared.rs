use ratatui::{
    Frame,
    style::{Color, Style},
    text::Line,
    widgets::{Block, BorderType, Borders, Paragraph, Wrap},
};

pub(super) const ACCENT: Color = Color::Cyan;
pub(super) const MUTED: Color = Color::DarkGray;

pub(super) fn block(title: impl Into<Line<'static>>, active: bool) -> Block<'static> {
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

pub(super) fn help(frame: &mut Frame, text: &str, area: ratatui::layout::Rect) {
    frame.render_widget(
        Paragraph::new(text)
            .style(Style::default().fg(MUTED))
            .wrap(Wrap { trim: false }),
        area,
    );
}

pub(super) fn setup_header(frame: &mut Frame, phase: &str, area: ratatui::layout::Rect) {
    use ratatui::{style::Modifier, text::Span};

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
