use std::process::Command;

use crate::agent::sandbox::sandbox::{Sandbox, SandboxError};
use crate::agent::sandbox::filesystem::SandboxedFilesystem;

pub struct BwrapSandbox {
    workspace: SandboxedFilesystem,
}

impl BwrapSandbox {
    pub fn new(
        workspace: SandboxedFilesystem,
    ) -> BwrapSandbox {
        BwrapSandbox {
            workspace,
        }
    }
}


impl Sandbox for BwrapSandbox {
    fn execute(
        &self,
        command: &str,
        args: &[String],
    ) -> Result<(), SandboxError> {
        let mut cmd = Command::new("bwrap");

        // Basic isolation
        cmd.arg("--unshare-all");
        cmd.arg("--die-with-parent");

        // Read-only mounts
        for mount in self.workspace.ro_binds() {
            cmd.arg("--ro-bind")
                .arg(&mount.host)
                .arg(&mount.guest);
        }

        // Writable mounts
        for mount in self.workspace.w_binds() {
            cmd.arg("--bind")
                .arg(&mount.host)
                .arg(&mount.guest);
        }

        // Command to execute inside sandbox
        cmd.arg("--");
        cmd.arg(command);

        for arg in args {
            cmd.arg(arg);
        }

        let status = cmd
            .status()
            .map_err(SandboxError::Io)?;

        if status.success() {
            Ok(())
        } else {
            Err(SandboxError::ExecutionFailed(
                format!(
                    "command exited with status {}",
                    status
                ),
            ))
        }
    }

    fn workspace(&self) -> &SandboxedFilesystem {
        &self.workspace
    }
}