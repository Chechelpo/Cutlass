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

    fn command(&self, command: &str, args: &[String], writable: bool) -> Command {
        let mut cmd = Command::new("bwrap");

        cmd.arg("--unshare-all");
        cmd.arg("--die-with-parent");

        for mount in self.workspace.ro_binds() {
            cmd.arg("--ro-bind").arg(&mount.host).arg(&mount.guest);
        }

        for mount in self.workspace.w_binds() {
            let bind_argument = if writable { "--bind" } else { "--ro-bind" };
            cmd.arg(bind_argument).arg(&mount.host).arg(&mount.guest);
        }

        if !self.workspace.workspace_base().as_os_str().is_empty() {
            cmd.arg("--chdir").arg(self.workspace.workspace_base());
        }

        cmd.arg("--").arg(command).args(args);
        cmd
    }
}

impl Sandbox for BwrapSandbox {
    fn execute(
        &self,
        command: &str,
        args: &[String],
        writable: bool,
    ) -> Result<CommandOutput, SandboxError> {
        let output = self
            .command(command, args, writable)
            .output()
            .map_err(SandboxError::Io)?;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::sandbox::filesystem::BindMount;
    use std::path::PathBuf;

    fn sandbox() -> BwrapSandbox {
        BwrapSandbox::new(SandboxedFilesystem::new(
            PathBuf::from("/workspace"),
            vec![BindMount {
                host: PathBuf::from("/usr"),
                guest: PathBuf::from("/usr"),
            }],
            vec![BindMount {
                host: PathBuf::from("/host/workspace"),
                guest: PathBuf::from("/workspace"),
            }],
        ))
    }

    fn arguments(writable: bool) -> Vec<String> {
        sandbox()
            .command("/bin/bash", &["-lc".into(), "pwd".into()], writable)
            .get_args()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn workspace_mount_is_read_only_by_default() {
        let args = arguments(false);
        assert!(args
            .windows(3)
            .any(|args| args == ["--ro-bind", "/host/workspace", "/workspace"]));
        assert!(!args.iter().any(|argument| argument == "--bind"));
    }

    #[test]
    fn workspace_mount_is_writable_when_enabled() {
        let args = arguments(true);
        assert!(args
            .windows(3)
            .any(|args| args == ["--bind", "/host/workspace", "/workspace"]));
    }
}
