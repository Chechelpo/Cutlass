use crate::agent::agent_session::AgentSession;
use crate::chat_completions::tools::{ToolCall, ToolResult};
use crate::tools::tool::DynTool;

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
}

impl ToolGroup {
    fn new(kind: ToolGroupKind) -> Self {
        Self {
            kind,
            tools: Vec::new(),
        }
    }

    pub fn kind(&self) -> ToolGroupKind {
        self.kind
    }

    pub fn tools(&self) -> &[Box<dyn DynTool>] {
        &self.tools
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
}
