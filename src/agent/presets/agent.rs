use crate::agent::prompt::{SysPrompt, UserPrependSections};
use crate::orchestrator::memory::MemoryGroupPreset;
use crate::tools::group::ToolGroup;
use crate::tools::tool::DynTool;

/// Periodic private guidance injected between model/tool rounds.
pub struct TaskSteeringConfig {
    pub include_first: bool,
    pub every_n_turns: usize,
    pub content: String,
}

impl TaskSteeringConfig {
    pub fn new(include_first: bool, every_n_turns: usize, content: impl Into<String>) -> Self {
        assert!(every_n_turns > 0, "task steering cadence must be positive");
        Self {
            include_first,
            every_n_turns,
            content: content.into(),
        }
    }

    pub fn applies_before_round(&self, completed_rounds: usize) -> bool {
        (self.include_first && completed_rounds == 0)
            || (completed_rounds > 0 && completed_rounds % self.every_n_turns == 0)
    }
}

pub struct Agent {
    pub name: String,
    pub description: String,

    pub system_prompt: SysPrompt,
    pub user_sections: Vec<UserPrependSections>,

    pub tool_groups: Vec<ToolGroup>,
    pub skills: Vec<Box<dyn DynTool>>,
    pub memory_group: Option<MemoryGroupPreset>,
    pub task_steering: Option<TaskSteeringConfig>,
}

impl Agent {
    pub fn new(
        name: String,
        description: String,
        system_prompt: SysPrompt,
        user_sections: Vec<UserPrependSections>,
        tool_groups: Vec<ToolGroup>,
        skills: Vec<Box<dyn DynTool>>,
    ) -> Agent {
        Agent {
            name,
            description,
            system_prompt,
            user_sections,
            tool_groups,
            skills,
            memory_group: None,
            task_steering: None,
        }
    }

    /// Attach a complete typed memory group to this agent preset.
    pub fn with_memory_group(mut self, memory_group: MemoryGroupPreset) -> Self {
        self.tool_groups.push(memory_group.tool_group());
        self.memory_group = Some(memory_group);
        self
    }

    pub fn with_task_steering(mut self, task_steering: TaskSteeringConfig) -> Self {
        self.task_steering = Some(task_steering);
        self
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
            vec![],
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

    #[test]
    fn task_steering_obeys_its_round_cadence() {
        let steering = TaskSteeringConfig::new(false, 3, "review");
        assert!(!steering.applies_before_round(0));
        assert!(!steering.applies_before_round(2));
        assert!(steering.applies_before_round(3));
        assert!(steering.applies_before_round(6));
    }
}
