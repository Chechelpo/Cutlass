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

/// A frontend-neutral colour that presentation layers can map to their own
/// native colour type.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RenderColor {
    Default,
    Muted,
    Red,
    Yellow,
    Green,
    Cyan,
    Blue,
    Magenta,
}

/// Colours used for the structural parts of a rendered tool group.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ToolGroupColorScheme {
    pub header: RenderColor,
    pub connector: RenderColor,
    pub title: RenderColor,
    pub body: RenderColor,
}

impl Default for ToolGroupColorScheme {
    fn default() -> Self {
        Self {
            header: RenderColor::Muted,
            connector: RenderColor::Muted,
            title: RenderColor::Default,
            body: RenderColor::Muted,
        }
    }
}

/// Rendering declared by a tool group for one batch of its completed calls.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderToolGroup {
    pub header: RenderText,
    pub calls: Vec<RenderToolCall>,
    pub color_scheme: ToolGroupColorScheme,
}

impl RenderToolGroup {
    pub fn new(header: RenderText, calls: Vec<RenderToolCall>) -> Self {
        Self {
            header,
            calls,
            color_scheme: ToolGroupColorScheme::default(),
        }
    }

    pub fn with_color_scheme(mut self, color_scheme: ToolGroupColorScheme) -> Self {
        self.color_scheme = color_scheme;
        self
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
        assert_eq!(group.color_scheme, ToolGroupColorScheme::default());
    }

    #[test]
    fn tool_groups_can_declare_a_color_scheme() {
        let scheme = ToolGroupColorScheme {
            header: RenderColor::Cyan,
            connector: RenderColor::Blue,
            title: RenderColor::Green,
            body: RenderColor::Muted,
        };
        let group =
            RenderToolGroup::new(RenderText::plain("Files"), Vec::new()).with_color_scheme(scheme);

        assert_eq!(group.color_scheme, scheme);
    }
}
