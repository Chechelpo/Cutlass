use crate::agent::sandbox::filesystem::SandboxedFilesystem;
use crate::tools::tool::DynTool;

pub struct AgentPreset {
    pub name: String,
    pub description: String,

    pub system_prompt: fn(&SandboxedFilesystem) -> String,

    pub allowed_tools: Vec<Box<dyn DynTool>>,
    pub skills: Vec<Box<dyn DynTool>>,
}