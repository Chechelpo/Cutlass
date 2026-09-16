use crate::agent::agent_session::AgentSession;
use crate::chat_completions::tools::{ToolCall, ToolResult};
use crate::tools::tool::DynTool;
use crate::ui_interface::chat::{RenderText, RenderToolCall, RenderToolGroup};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToolGroupKind {
    Immediate,
    Deferred,
}

/// A collection of tools with the same execution behavior.
///
/// Tool groups execute calls; message history owns the resulting wire content
/// and local render metadata.
pub struct ToolGroup {
    kind: ToolGroupKind,
    tools: Vec<Box<dyn DynTool>>,
    renderer: Box<dyn Fn(Vec<RenderToolCall>) -> RenderToolGroup>,
}

impl ToolGroup {
    fn new(kind: ToolGroupKind) -> Self {
        let header = match kind {
            ToolGroupKind::Immediate => "Tools",
            ToolGroupKind::Deferred => "Deferred tools",
        };

        Self {
            kind,
            tools: Vec::new(),
            renderer: Box::new(move |calls| RenderToolGroup::new(RenderText::plain(header), calls)),
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

    pub fn kind(&self) -> ToolGroupKind {
        self.kind
    }

    pub fn tools(&self) -> &[Box<dyn DynTool>] {
        &self.tools
    }

    pub fn render(&self, calls: Vec<RenderToolCall>) -> RenderToolGroup {
        (self.renderer)(calls)
    }

    pub fn execute_owned_call(&self, agent: &AgentSession, call: &ToolCall) -> Option<ToolResult> {
        self.tools
            .iter()
            .find(|tool| tool.name() == call.function.name)
            .map(|tool| tool.run(agent, call))
    }
}

/// Route preset-instantiated tools into the stable default groups.
pub fn into_groups(tools: Vec<Box<dyn DynTool>>) -> Vec<ToolGroup> {
    let mut immediate = ToolGroup::new(ToolGroupKind::Immediate);
    let mut deferred = ToolGroup::new(ToolGroupKind::Deferred);

    for tool in tools {
        if tool.deferred() {
            deferred.tools.push(tool);
        } else {
            immediate.tools.push(tool);
        }
    }

    vec![immediate, deferred]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::explorer::ReadFileTool;

    #[test]
    fn routes_configured_tool_instances_into_default_groups() {
        let groups = into_groups(vec![
            Box::new(ReadFileTool::all_actions(false)),
            Box::new(ReadFileTool::all_actions(true)),
        ]);

        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].kind(), ToolGroupKind::Immediate);
        assert_eq!(groups[0].tools().len(), 1);
        assert_eq!(groups[1].kind(), ToolGroupKind::Deferred);
        assert_eq!(groups[1].tools().len(), 1);
    }

    #[test]
    fn lets_a_group_declare_its_rendering() {
        let group = ToolGroup::new(ToolGroupKind::Immediate).with_renderer(|calls| {
            RenderToolGroup::new(RenderText::markdown("### File operations"), calls)
        });

        let rendered = group.render(vec![RenderToolCall::new(RenderText::plain("Read file"))]);

        assert_eq!(rendered.header, RenderText::markdown("### File operations"));
        assert_eq!(rendered.calls.len(), 1);
    }
}
