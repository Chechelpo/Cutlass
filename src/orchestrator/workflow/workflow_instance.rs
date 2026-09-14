use crate::orchestrator::workflow::workflow::Workflow;

pub struct WorkflowInstance {
    workflow: &'static dyn Workflow,
    current_step: usize,
}
impl WorkflowInstance {
    pub fn new(workflow: &'static dyn Workflow) -> Self {
        WorkflowInstance {
            workflow,
            current_step: 0,
        }
    }
}
