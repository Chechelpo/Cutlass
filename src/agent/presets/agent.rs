use crate::agent::sandbox::filesystem::SandboxedFilesystem;
use crate::tools::group::{ToolGroup, into_groups};
use crate::tools::tool::DynTool;

pub struct Agent {
    pub name: String,
    pub description: String,

    pub system_prompt: fn(&SandboxedFilesystem) -> String,

    pub tool_groups: Vec<ToolGroup>,
    pub skills: Vec<Box<dyn DynTool>>,
}

impl Agent {
    pub fn new(
        name: String,
        description: String,
        system_prompt: fn(&SandboxedFilesystem) -> String,
        tools: Vec<Box<dyn DynTool>>,
        skills: Vec<Box<dyn DynTool>>,
    ) -> Agent {
        Agent {
            name,
            description,
            system_prompt,
            tool_groups: into_groups(tools),
            skills,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::explorer::{ReadFileAction, ReadFileTool};
    use crate::tools::group::ToolGroupKind;

    #[test]
    fn stores_action_configured_tools_in_default_groups() {
        let preset = Agent::new(
            "Coder".into(),
            "Coding specialized agent".into(),
            |_filesystem| "You are a coder assistant".into(),
            vec![Box::new(ReadFileTool::with_actions(
                [ReadFileAction::Read],
                false,
            ))],
            vec![],
        );

        assert_eq!(preset.tool_groups.len(), 2);
        assert_eq!(preset.tool_groups[0].kind(), ToolGroupKind::Immediate);
        assert_eq!(preset.tool_groups[0].tools()[0].name(), "read_file");
        assert!(preset.tool_groups[1].tools().is_empty());
    }
}
