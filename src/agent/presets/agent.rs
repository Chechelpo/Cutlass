use crate::agent::prompt::SysPrompt;
use crate::tools::group::ToolGroup;
use crate::tools::tool::DynTool;

pub struct Agent {
    pub name: String,
    pub description: String,

    pub system_prompt: SysPrompt,

    pub tool_groups: Vec<ToolGroup>,
    pub skills: Vec<Box<dyn DynTool>>,
}

impl Agent {
    pub fn new(
        name: String,
        description: String,
        system_prompt: SysPrompt,
        tool_groups: Vec<ToolGroup>,
        skills: Vec<Box<dyn DynTool>>,
    ) -> Agent {
        Agent {
            name,
            description,
            system_prompt,
            tool_groups,
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
    fn preserves_explicit_tool_groups() {
        let preset = Agent::new(
            "Coder".into(),
            "Coding specialized agent".into(),
            SysPrompt::new(String::from(""), vec![], String::from("")),
            vec![ToolGroup::new(
                ToolGroupKind::Explorer,
                vec![Box::new(ReadFileTool::with_actions(
                    [ReadFileAction::Read],
                    false,
                ))],
            )],
            vec![],
        );

        assert_eq!(preset.tool_groups.len(), 1);
        assert_eq!(preset.tool_groups[0].kind(), ToolGroupKind::Explorer);
        assert_eq!(preset.tool_groups[0].tools()[0].name(), "read_file");
    }
}
