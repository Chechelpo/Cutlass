use crate::tools::tool::DynTool;

pub struct AgentPreset{
    pub system_prompt: String,
    pub allowed_tools: Vec<Box<dyn DynTool>>
}