use crate::agent::agent_session::AgentSession;
use crate::chat_completions::tools::{ToolCall, ToolResult};
use crate::tools::tool::DynTool;
use crate::ui_interface::chat::{
    RenderColor, RenderText, RenderToolCall, RenderToolGroup, ToolGroupColorScheme,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToolGroupKind {
    Explorer,
    Editing,
    Running,
}

/// An explicitly configured collection of related tools rendered together.
///
/// Membership is chosen by the agent preset, independently of whether a tool
/// is deferred. The group owns the presentation of its collected call results.
pub struct ToolGroup {
    kind: ToolGroupKind,
    tools: Vec<Box<dyn DynTool>>,
    renderer: Box<dyn Fn(Vec<RenderToolCall>) -> RenderToolGroup>,
    color_scheme: ToolGroupColorScheme,
}

impl ToolGroup {
    pub fn new(kind: ToolGroupKind, tools: Vec<Box<dyn DynTool>>) -> Self {
        let header = match kind {
            ToolGroupKind::Explorer => "Explored",
            ToolGroupKind::Editing => "Edited",
            ToolGroupKind::Running => "Ran",
        };

        Self {
            kind,
            tools,
            renderer: Box::new(move |calls| RenderToolGroup::new(RenderText::plain(header), calls)),
            color_scheme: match kind {
                ToolGroupKind::Explorer => ToolGroupColorScheme {
                    header: RenderColor::Cyan,
                    connector: RenderColor::Blue,
                    title: RenderColor::Default,
                    body: RenderColor::Muted,
                },
                ToolGroupKind::Editing => ToolGroupColorScheme {
                    header: RenderColor::Green,
                    connector: RenderColor::Green,
                    title: RenderColor::Default,
                    body: RenderColor::Muted
                },
                ToolGroupKind::Running => ToolGroupColorScheme {
                    header: RenderColor::Yellow,
                    connector: RenderColor::Yellow,
                    title: RenderColor::Default,
                    body: RenderColor::Muted,
                },
            },
        }
    }

    /// Replaces this group's framework-neutral rendering declaration.
    pub fn with_renderer(
        mut self,
        renderer: impl Fn(Vec<RenderToolCall>) -> RenderToolGroup + 'static,
    ) -> Self {
        self.set_renderer(renderer);
        self
    }

    pub fn set_renderer(
        &mut self,
        renderer: impl Fn(Vec<RenderToolCall>) -> RenderToolGroup + 'static,
    ) {
        self.renderer = Box::new(renderer);
    }

    pub fn with_color_scheme(mut self, color_scheme: ToolGroupColorScheme) -> Self {
        self.set_color_scheme(color_scheme);
        self
    }

    pub fn set_color_scheme(&mut self, color_scheme: ToolGroupColorScheme) {
        self.color_scheme = color_scheme;
    }

    pub fn kind(&self) -> ToolGroupKind {
        self.kind
    }

    pub fn tools(&self) -> &[Box<dyn DynTool>] {
        &self.tools
    }

    pub fn render(&self, calls: Vec<RenderToolCall>) -> RenderToolGroup {
        (self.renderer)(calls).with_color_scheme(self.color_scheme)
    }

    pub fn execute_owned_call(&self, agent: &AgentSession, call: &ToolCall) -> Option<ToolResult> {
        self.tools
            .iter()
            .find(|tool| tool.name() == call.function.name)
            .map(|tool| tool.run(agent, call))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::explorer::{ReadFileTool, TreeTool};

    #[test]
    fn related_tools_share_a_group_regardless_of_deferred_execution() {
        let group = ToolGroup::new(
            ToolGroupKind::Explorer,
            vec![
                Box::new(ReadFileTool::all_actions(false)),
                Box::new(TreeTool::all_actions(true)),
            ],
        );

        assert_eq!(group.kind(), ToolGroupKind::Explorer);
        assert_eq!(group.tools().len(), 2);
        assert!(!group.tools()[0].deferred());
        assert!(group.tools()[1].deferred());
        assert_eq!(
            group.render(Vec::new()).header,
            RenderText::plain("Explored")
        );
        assert_eq!(
            group.render(Vec::new()).color_scheme.header,
            RenderColor::Cyan
        );
    }

    #[test]
    fn lets_a_group_declare_its_rendering() {
        let group = ToolGroup::new(ToolGroupKind::Explorer, Vec::new()).with_renderer(|calls| {
            RenderToolGroup::new(RenderText::markdown("### File operations"), calls)
        });

        let rendered = group.render(vec![RenderToolCall::new(RenderText::plain("Read file"))]);

        assert_eq!(rendered.header, RenderText::markdown("### File operations"));
        assert_eq!(rendered.calls.len(), 1);
    }

    #[test]
    fn lets_a_group_override_its_color_scheme() {
        let scheme = ToolGroupColorScheme {
            header: RenderColor::Magenta,
            connector: RenderColor::Red,
            title: RenderColor::Yellow,
            body: RenderColor::Green,
        };
        let group = ToolGroup::new(ToolGroupKind::Explorer, Vec::new()).with_color_scheme(scheme);

        assert_eq!(group.render(Vec::new()).color_scheme, scheme);
    }
}
