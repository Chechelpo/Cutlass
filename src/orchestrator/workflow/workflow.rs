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

#[derive(Clone, Debug)]
pub enum WorkflowEvent {
    StepChanged { step: String },
    Notice(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WorkflowStatus {
    Ready,
    Running,
    WaitingForInput,
    Completed,
    Cancelled,
    Failed,
}

#[derive(Clone, Debug)]
pub struct WorkflowError {
    pub message: String,
    pub retryable: bool,
}

pub trait Workflow {
    fn name(&self) -> &String;
    fn description(&self) -> &String;

    fn status(&self) -> WorkflowStatus;
    fn events(&mut self) -> &[WorkflowEvent];

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
