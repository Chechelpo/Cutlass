use crate::agent::presets::preset::AgentPreset;
use crate::tools::explorer::ReadFileTool;

pub struct AgentPresetRegistry{
    all_agents: Vec<AgentPreset>,
}

impl AgentPresetRegistry {
    pub fn get_agents(&self) -> &Vec<AgentPreset> {
        &self.all_agents
    }

    pub fn get_with_name(&self, name: &str) -> Option<&AgentPreset> {
        self.all_agents.iter().find(|agent| agent.name == name)
    }

    pub fn new() -> Self {
        AgentPresetRegistry {
            all_agents: vec![
                // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
                // CODER
                // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
                AgentPreset {
                    name: String::from("Coder") ,
                    description: String::from("Coding specialized agent"),
                    allowed_tools: vec![
                        Box::new(ReadFileTool::new())
                    ],
                    system_prompt: |filesystem| String::from("You are a coder assistant"),
                    skills: vec![]
                }
            ],
        }
    }
}
