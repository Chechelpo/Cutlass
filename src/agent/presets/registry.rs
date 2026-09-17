use crate::agent::presets::agent::Agent;
use crate::agent::prompt::{SysPrompt, SysPromptSections};
use crate::tools::explorer::{ReadFileAction, ReadFileTool, TreeTool};
use crate::tools::group::{ToolGroup, ToolGroupKind};
use tracing::{debug, info};
use crate::tools::editing::edit::EditFileTool;
use crate::tools::editing::write::CreateFileTool;

pub struct AgentPresetRegistry {
    all_agents: Vec<Agent>,
}

impl AgentPresetRegistry {
    pub fn get_agents(&self) -> &Vec<Agent> {
        &self.all_agents
    }

    pub fn get_with_name(&self, name: &str) -> Option<&Agent> {
        self.all_agents.iter().find(|agent| agent.name == name)
    }

    pub fn new() -> Self {
        let to_return = AgentPresetRegistry {
            all_agents: vec![
                // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
                // CODER
                // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
                Agent::new(
                    String::from("Coder"),
                    String::from("Coding specialized agent"),
                    SysPrompt::new(
                        String::from("You are an expert coding assistant"),
                        SysPromptSections::all(),
                        String::from(""),
                    ),
                    vec![
                        ToolGroup::new(
                            ToolGroupKind::Explorer,
                            vec![
                                Box::new(ReadFileTool::with_actions([ReadFileAction::Read], false)),
                                Box::new(TreeTool::all_actions(false)),
                            ],
                        ),
                        ToolGroup::new(
                            ToolGroupKind::Editing,
                            vec![
                                Box::new(EditFileTool::all_actions(false)),
                                Box::new(CreateFileTool::all_actions(false)),
                            ]
                        )
                    ],
                    vec![],
                ),
            ],
        };
        debug!("Instantiated agent registry:\n{}", to_return);
        to_return
    }
}

impl std::fmt::Display for AgentPresetRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Available agents: {}", self.all_agents.len())?;

        for (index, agent) in self.all_agents.iter().enumerate() {
            writeln!(f, "{}. {} - {}", index + 1, agent.name, agent.description)?;
        }

        Ok(())
    }
}
