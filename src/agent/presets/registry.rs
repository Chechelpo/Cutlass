use crate::agent::presets::agent::Agent;
use crate::tools::explorer::{ReadFileAction, ReadFileTool};

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
        AgentPresetRegistry {
            all_agents: vec![
                // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
                // CODER
                // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
                Agent::new(
                    String::from("Coder"),
                    String::from("Coding specialized agent"),
                    |_filesystem| String::from("You are a coder assistant"),
                    vec![Box::new(ReadFileTool::with_actions(
                        [ReadFileAction::Read],
                        false,
                    ))],
                    vec![],
                ),
            ],
        }
    }
}
