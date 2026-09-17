use crate::agent::sandbox::bwrap::BwrapSandbox;
use crate::agent::sandbox::filesystem::SandboxedFilesystem;
use std::fmt;

#[derive(Debug)]
pub enum SandboxError {
    ExecutionFailed(String),
    PermissionDenied(String),
    InvalidWorkspace(String),
    Io(std::io::Error),
}
impl fmt::Display for SandboxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SandboxError::ExecutionFailed(msg) => {
                write!(f, "Sandbox execution failed: {}", msg)
            }

            SandboxError::PermissionDenied(msg) => {
                write!(f, "Permission denied: {}", msg)
            }

            SandboxError::InvalidWorkspace(msg) => {
                write!(f, "Invalid workspace: {}", msg)
            }

            SandboxError::Io(err) => {
                write!(f, "IO error: {}", err)
            }
        }
    }
}

pub trait Sandbox {
    fn execute(&self, command: &str, args: &[String]) -> Result<CommandOutput, SandboxError>;

    fn workspace(&self) -> &SandboxedFilesystem;
}

#[derive(Debug, Eq, PartialEq)]
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
}

pub fn create_sandbox(workspace: SandboxedFilesystem) -> Box<dyn Sandbox> {
    #[cfg(target_os = "linux")]
    {
        Box::new(BwrapSandbox::new(workspace))
    }

    #[cfg(target_os = "windows")]
    {
        Box::new(WindowsSandbox::new(workspace))
    }
}
