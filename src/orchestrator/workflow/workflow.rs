use crate::agent::agent_session::Agent;
//
// struct WorkflowStep {
//     name: String,
//     agent: &'static Agent,
//     possible_backtracks: Vec<&'static WorkflowStep>,
// }
//
// struct Workflow {
//     name: String,
//     description: String,
//     steps: Vec<WorkflowStep>,
// }

pub trait WorkflowStep {
    fn name(&self) -> &str;
    fn agent(&self) -> &Agent;

    fn possible_backtracks(&self) -> Vec<&'static dyn WorkflowStep>;
}

pub trait Workflow {
    fn name(&self) -> &String;
    fn description(&self) -> &String;

    fn steps(&self) -> &Vec<&Agent>;
}

pub struct WorkflowRegistry {
    workflows: Vec<Box<dyn Workflow>>,
}

impl Default for WorkflowRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkflowRegistry {
    pub fn new() -> WorkflowRegistry {
        WorkflowRegistry {
            workflows: Vec::new(),
        }
    }

    pub fn with_name(&self, name: &str) -> Option<&dyn Workflow> {
        self.workflows
            .iter()
            .find(|w| w.name() == name)
            .map(|w| w.as_ref())
    }

    pub fn register(&mut self, workflow: impl Workflow + 'static) {
        self.workflows.push(Box::new(workflow));
    }
}
