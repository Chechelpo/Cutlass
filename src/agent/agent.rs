use crate::agent::names;
use crate::agent::sandbox::sandbox::{Sandbox, create_sandbox};
use crate::agent::sandbox::filesystem::SandboxedFilesystem;
use crate::chat_completions::messages::Message;
use names::get_agent_name;
use crate::agent::presets::preset::AgentPreset;
use crate::agent::steering::SteeringInbox;
use crate::config::ModelConfig;

pub struct Agent<'a> {
    pub name: String,
    pub preset: &'a AgentPreset,
    pub model_config: &'a ModelConfig,
    pub chat_history: Vec<Message>,
    pub steering_inbox: SteeringInbox,
    pub sandbox: Box<dyn Sandbox>,
}

impl<'a> Agent<'a> {
    pub fn new(
        sandboxed_filesystem: SandboxedFilesystem,
        config: &'a ModelConfig,
        preset: &'a AgentPreset,
    ) -> Self {
        Agent {
            name: get_agent_name(),
            preset,
            model_config: config,
            chat_history: Vec::new(),
            steering_inbox: SteeringInbox::new(),
            sandbox: create_sandbox(sandboxed_filesystem),
        }
    }

    pub fn run_turn(&self) {
        println!("{} is running", self.name);
    }

}