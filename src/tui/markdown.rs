use crate::ui_interface::chat::RenderText;
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

pub(super) fn clean(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
        .collect::<String>()
        .replace('\t', "    ")
}

/// Owned terminal spans; markdown never emits terminal escapes or executes HTML.
pub(super) fn render(text: &RenderText) -> Vec<Line<'static>> {
    let source = clean(text.as_str());
    if matches!(text, RenderText::Plain(_)) {
        return source
            .split('\n')
            .map(|line| Line::raw(line.to_owned()))
            .collect();
    }
    let mut lines = Vec::new();
    let mut current = Vec::<Span<'static>>::new();
    let mut styles = vec![Style::default()];
    let mut lists: Vec<Option<u64>> = Vec::new();
    let mut code_block = false;
    for event in Parser::new_ext(
        &source,
        Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS,
    ) {
        let style = *styles.last().unwrap();
        match event {
            Event::Start(tag) => {
                let next = match tag {
                    Tag::Heading { .. } => style.fg(Color::Cyan).add_modifier(Modifier::BOLD),
                    Tag::Strong => style.add_modifier(Modifier::BOLD),
                    Tag::Emphasis => style.add_modifier(Modifier::ITALIC),
                    Tag::Strikethrough => style.add_modifier(Modifier::CROSSED_OUT),
                    Tag::Link { .. } => style.fg(Color::Blue).add_modifier(Modifier::UNDERLINED),
                    Tag::CodeBlock(_) => {
                        code_block = true;
                        style.fg(Color::Green)
                    }
                    Tag::List(start) => {
                        lists.push(start);
                        style
                    }
                    Tag::Item => {
                        if !current.is_empty() {
                            lines.push(Line::from(std::mem::take(&mut current)));
                        }
                        let prefix = match lists.last_mut() {
                            Some(Some(n)) => {
                                let prefix = format!("{n}. ");
                                *n += 1;
                                prefix
                            }
                            _ => "• ".into(),
                        };
                        current.push(Span::raw(format!(
                            "{}{prefix}",
                            "  ".repeat(lists.len().saturating_sub(1))
                        )));
                        style
                    }
                    Tag::BlockQuote(_) => {
                        current.push(Span::raw("│ "));
                        style.fg(Color::DarkGray)
                    }
                    _ => style,
                };
                styles.push(next);
            }
            Event::End(tag) => {
                styles.pop();
                if matches!(tag, TagEnd::CodeBlock) {
                    code_block = false;
                }
                if matches!(tag, TagEnd::List(_)) {
                    lists.pop();
                }
                if matches!(
                    tag,
                    TagEnd::Paragraph
                        | TagEnd::Heading(_)
                        | TagEnd::CodeBlock
                        | TagEnd::Item
                        | TagEnd::BlockQuote(_)
                ) && !current.is_empty()
                {
                    lines.push(Line::from(std::mem::take(&mut current)));
                }
                if matches!(tag, TagEnd::Paragraph | TagEnd::CodeBlock | TagEnd::List(_))
                    && lists.is_empty()
                {
                    lines.push(Line::default());
                }
            }
            Event::Text(text) | Event::Html(text) | Event::InlineHtml(text) => {
                for (index, part) in text.split('\n').enumerate() {
                    if index > 0 {
                        lines.push(Line::from(std::mem::take(&mut current)));
                    }
                    if !part.is_empty() {
                        current.push(Span::styled(part.to_owned(), style));
                    }
                }
            }
            Event::Code(text) => {
                current.push(Span::styled(text.into_string(), style.fg(Color::Green)))
            }
            Event::SoftBreak if !code_block => current.push(Span::raw(" ")),
            Event::SoftBreak | Event::HardBreak => {
                lines.push(Line::from(std::mem::take(&mut current)))
            }
            Event::Rule => {
                lines.push(Line::raw("────────────────────"));
            }
            Event::TaskListMarker(done) => {
                current.push(Span::raw(if done { "[x] " } else { "[ ] " }))
            }
            _ => {}
        }
    }
    if !current.is_empty() {
        lines.push(Line::from(current));
    }
    while lines.last().is_some_and(|l| l.spans.is_empty()) {
        lines.pop();
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_is_literal_and_markdown_has_styles() {
        let plain = render(&RenderText::plain("**literal**"));
        assert_eq!(plain[0].to_string(), "**literal**");
        let md = render(&RenderText::markdown(
            "# Title\n\n**bold** and `code`\n\n- one\n- two",
        ));
        assert_eq!(md[0].to_string(), "Title");
        assert!(md[0].spans[0].style.add_modifier.contains(Modifier::BOLD));
        assert!(md.iter().any(|line| line.to_string() == "• two"));
        assert!(
            md.iter()
                .flat_map(|line| &line.spans)
                .any(|span| span.content == "code" && span.style.fg == Some(Color::Green))
        );
    }
}
