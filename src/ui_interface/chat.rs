/// Framework-neutral text that a frontend can render appropriately.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RenderText {
    Plain(String),
    Markdown(String),
}

impl RenderText {
    pub fn plain(value: impl Into<String>) -> Self {
        Self::Plain(value.into())
    }

    pub fn markdown(value: impl Into<String>) -> Self {
        Self::Markdown(value.into())
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::Plain(value) | Self::Markdown(value) => value,
        }
    }
}

/// Rendering declared by one tool for one completed call.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderToolCall {
    pub title: RenderText,
    pub body: Option<RenderText>,
}

impl RenderToolCall {
    pub fn new(title: RenderText) -> Self {
        Self { title, body: None }
    }

    pub fn with_body(mut self, body: RenderText) -> Self {
        self.body = Some(body);
        self
    }
}

/// Rendering declared by a tool group for one batch of its completed calls.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderToolGroup {
    pub header: RenderText,
    pub calls: Vec<RenderToolCall>,
}

impl RenderToolGroup {
    pub fn new(header: RenderText, calls: Vec<RenderToolCall>) -> Self {
        Self { header, calls }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RenderMessageSection {
    Message {
        speaker: String,
        content: RenderText,
    },
    ToolGroup(RenderToolGroup),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supports_markdown_in_tool_group_headers_and_calls() {
        let group = RenderToolGroup::new(
            RenderText::markdown("### Files"),
            vec![
                RenderToolCall::new(RenderText::plain("Read file"))
                    .with_body(RenderText::markdown("Read **12** lines")),
            ],
        );

        assert_eq!(group.header.as_str(), "### Files");
        assert_eq!(group.calls[0].title.as_str(), "Read file");
        assert_eq!(
            group.calls[0].body.as_ref().map(RenderText::as_str),
            Some("Read **12** lines"),
        );
    }
}
