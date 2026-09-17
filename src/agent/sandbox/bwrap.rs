use std::process::Command;

use crate::agent::sandbox::filesystem::SandboxedFilesystem;
use crate::agent::sandbox::sandbox::{CommandOutput, Sandbox, SandboxError};

pub struct BwrapSandbox {
    workspace: SandboxedFilesystem,
}

impl BwrapSandbox {
    pub fn new(workspace: SandboxedFilesystem) -> BwrapSandbox {
        BwrapSandbox { workspace }
    }
}

impl Sandbox for BwrapSandbox {
    fn execute(&self, command: &str, args: &[String]) -> Result<CommandOutput, SandboxError> {
        let mut cmd = Command::new("bwrap");

        // Basic isolation
        cmd.arg("--unshare-all");
        cmd.arg("--die-with-parent");

        // Read-only mounts
        for mount in self.workspace.ro_binds() {
            cmd.arg("--ro-bind").arg(&mount.host).arg(&mount.guest);
        }

        // Writable mounts
        for mount in self.workspace.w_binds() {
            cmd.arg("--bind").arg(&mount.host).arg(&mount.guest);
        }

        if !self.workspace.workspace_base().as_os_str().is_empty() {
            cmd.arg("--chdir").arg(self.workspace.workspace_base());
        }

        // Command to execute inside sandbox
        cmd.arg("--");
        cmd.arg(command);

        for arg in args {
            cmd.arg(arg);
        }

        let output = cmd.output().map_err(SandboxError::Io)?;

        Ok(CommandOutput {
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            exit_code: output.status.code(),
        })
    }

    fn workspace(&self) -> &SandboxedFilesystem {
        &self.workspace
    }
}
