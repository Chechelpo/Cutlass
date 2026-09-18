use crate::agent::presets::agent::Agent;
use crate::agent::presets::serial::serial_role_agents;
use crate::agent::prompt::{SysPrompt, SysPromptSections, UserPrependSections};
use crate::orchestrator::memory::MemoryGroupPreset;
use crate::tools::editing::edit::EditFileTool;
use crate::tools::editing::write::CreateFileTool;
use crate::tools::explorer::web_search::WebSearchTool;
use crate::tools::explorer::{ReadFileAction, ReadFileTool, TreeTool};
use crate::tools::group::{ToolGroup, ToolGroupKind};
use crate::tools::interaction::PromptUserTool;
use crate::tools::running::BashTool;
use tracing::{debug, info};

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
        let mut all_agents = vec![
            // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
            // CODER
            // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
            Agent::new(
                String::from("Coder"),
                String::from("Coding specialized agent"),
                SysPrompt::new(
                    String::from("You are an expert coding assistant"),
                    SysPromptSections::all(),
                    MemoryGroupPreset::protocol().into(),
                ),
                vec![UserPrependSections::RepoMap],
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
                        ],
                    ),
                    ToolGroup::new(
                        ToolGroupKind::Running,
                        vec![Box::new(BashTool::all_actions(false))],
                    ),
                    ToolGroup::new(
                        ToolGroupKind::Explorer,
                        vec![Box::new(WebSearchTool::from_env(false))],
                    ),
                    ToolGroup::new(
                        ToolGroupKind::Interaction,
                        vec![Box::new(PromptUserTool::new())],
                    ),
                ],
                vec![],
            )
            .with_memory_group(MemoryGroupPreset::coding()),
        ];
        all_agents.extend(serial_role_agents());
        let to_return = AgentPresetRegistry { all_agents };
        info!(
            agent_count = to_return.all_agents.len(),
            "created agent preset registry"
        );
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn includes_coder_and_both_serial_role_sets() {
        let registry = AgentPresetRegistry::new();
        assert_eq!(registry.get_agents().len(), 11);
        for name in [
            "serial:explore",
            "serial:plan",
            "serial:implement",
            "serial:test",
            "serial:review",
            "serial_assured:explore",
            "serial_assured:review",
        ] {
            assert!(registry.get_with_name(name).is_some(), "missing {name}");
        }
    }
}
