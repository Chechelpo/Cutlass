use crate::agent::sandbox::sandbox::SandboxError;
use std::fs;
use std::path::{Path, PathBuf};

pub struct BindMount {
    pub host: PathBuf,
    pub guest: PathBuf,
}

pub struct SandboxedFilesystem {
    ro_binds: Vec<BindMount>,
    w_binds: Vec<BindMount>,
}

impl SandboxedFilesystem {
    pub fn new(ro_binds: Vec<BindMount>, w_binds: Vec<BindMount>) -> SandboxedFilesystem {
        SandboxedFilesystem { ro_binds, w_binds }
    }
    pub fn ro_binds(&self) -> &[BindMount] {
        self.ro_binds.as_slice()
    }
    pub fn w_binds(&self) -> &[BindMount] {
        self.w_binds.as_slice()
    }

    pub fn in_read_bounds(&self, path: &Path) -> bool {
        inside_any(&self.ro_binds, path) || self.in_write_bounds(path)
    }

    pub fn in_write_bounds(&self, path: &Path) -> bool {
        inside_any(&self.w_binds, path)
    }

    pub fn read_file(&self, path: &Path) -> Result<String, SandboxError> {
        let host_path = self.resolve_read_path(path)?;

        fs::read_to_string(host_path).map_err(SandboxError::Io)
    }

    fn resolve_read_path(&self, path: &Path) -> Result<PathBuf, SandboxError> {
        let normalized_path = normalize_path(path)
            .filter(|path| path.is_absolute())
            .ok_or_else(|| read_denied(path))?;

        // A later, more specific bind shadows an earlier one, just like the
        // mounts installed by the process sandbox. Writable binds are chained
        // last because they are installed after read-only binds.
        let mount = self
            .ro_binds
            .iter()
            .chain(self.w_binds.iter())
            .filter_map(|mount| {
                let guest = normalize_path(&mount.guest)?;
                normalized_path
                    .starts_with(&guest)
                    .then_some((mount, guest.components().count()))
            })
            .max_by_key(|(_, depth)| *depth)
            .map(|(mount, _)| mount)
            .ok_or_else(|| read_denied(path))?;

        let guest = normalize_path(&mount.guest).ok_or_else(|| read_denied(path))?;
        let relative = normalized_path
            .strip_prefix(guest)
            .map_err(|_| read_denied(path))?;
        let host_root = fs::canonicalize(&mount.host).map_err(SandboxError::Io)?;
        let host_path = fs::canonicalize(host_root.join(relative)).map_err(SandboxError::Io)?;

        // Lexical checks alone allow a symlink inside a mount to escape it.
        // Verify the resolved target remains under the resolved host root.
        if !host_path.starts_with(&host_root) {
            return Err(read_denied(path));
        }

        Ok(host_path)
    }
}

fn read_denied(path: &Path) -> SandboxError {
    SandboxError::PermissionDenied(format!("Cannot read path: {}", path.display()))
}

// Path logic adapted from:
// https://stackoverflow.com/questions/62939265/how-to-check-if-a-path-is-a-subdirectory-of-another-path
//
// This performs lexical normalization only. It does not resolve symlinks.
// std::fs::canonicalize can be used for filesystem-backed validation,
// but it requires the paths to exist.
fn normalize_path(path: &Path) -> Option<PathBuf> {
    let mut normalized = PathBuf::new();

    for component in path.components() {
        match component {
            std::path::Component::ParentDir => {
                if !normalized.pop() {
                    return None;
                }
            }

            std::path::Component::CurDir => {}

            _ => {
                normalized.push(component.as_os_str());
            }
        }
    }

    Some(normalized)
}

fn is_path_within_base(path: &Path, base: &Path) -> bool {
    match (normalize_path(path), normalize_path(base)) {
        (Some(norm_path), Some(norm_base)) => norm_path.starts_with(norm_base),

        _ => false,
    }
}

fn inside_any(mounts: &[BindMount], path: &Path) -> bool {
    mounts
        .iter()
        .any(|mount| is_path_within_base(path, &mount.guest))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn bind(host: &str, guest: &str) -> BindMount {
        BindMount {
            host: PathBuf::from(host),
            guest: PathBuf::from(guest),
        }
    }

    fn filesystem() -> SandboxedFilesystem {
        SandboxedFilesystem::new(
            vec![bind("/usr", "/usr"), bind("/shared", "/shared")],
            vec![bind("/project", "/workspace"), bind("/shared", "/shared")],
        )
    }

    #[test]
    fn read_workspace_allows_read_bounds() {
        let fs = filesystem();

        assert!(fs.in_read_bounds(Path::new("/usr/bin/bash")));

        assert!(fs.in_read_bounds(Path::new("/usr/lib/libc.so")));
    }

    #[test]
    fn read_workspace_allows_write_bounds() {
        let fs = filesystem();

        assert!(fs.in_read_bounds(Path::new("/workspace/file.txt")));
    }

    #[test]
    fn write_workspace_allows_only_write_bounds() {
        let fs = filesystem();

        assert!(fs.in_write_bounds(Path::new("/workspace/file.txt")));

        assert!(!fs.in_write_bounds(Path::new("/usr/bin/bash")));
    }

    #[test]
    fn unrelated_paths_are_rejected() {
        let fs = filesystem();

        assert!(!fs.in_read_bounds(Path::new("/etc/shadow")));

        assert!(!fs.in_write_bounds(Path::new("/etc/shadow")));
    }

    #[test]
    fn exact_mount_point_is_allowed() {
        let fs = filesystem();

        assert!(fs.in_read_bounds(Path::new("/usr")));

        assert!(fs.in_write_bounds(Path::new("/workspace")));
    }

    #[test]
    fn parent_escape_is_rejected() {
        let fs = filesystem();

        assert!(!fs.in_write_bounds(Path::new("/workspace/../../etc/passwd")));
    }

    #[test]
    fn normalized_paths_are_handled() {
        let fs = filesystem();

        assert!(fs.in_write_bounds(Path::new("/workspace/./src/../orchestrator.rs")));

        assert!(fs.in_read_bounds(Path::new("/usr/./bin/../lib")));
    }

    #[test]
    fn shared_paths_have_both_permissions() {
        let fs = filesystem();

        assert!(fs.in_read_bounds(Path::new("/shared/file.txt")));

        assert!(fs.in_write_bounds(Path::new("/shared/file.txt")));
    }

    fn temporary_directory(test_name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "cutlass-{test_name}-{}-{unique}",
            std::process::id(),
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn read_file_translates_guest_path_to_host_path() {
        let host = temporary_directory("read-mapping");
        fs::write(host.join("message.txt"), "from host").unwrap();
        let filesystem = SandboxedFilesystem::new(
            vec![BindMount {
                host: host.clone(),
                guest: PathBuf::from("/workspace"),
            }],
            vec![],
        );

        let content = filesystem
            .read_file(Path::new("/workspace/message.txt"))
            .unwrap();

        assert_eq!(content, "from host");
        fs::remove_dir_all(host).unwrap();
    }

    #[test]
    fn read_file_rejects_paths_outside_mounts() {
        let filesystem = filesystem();

        let error = filesystem.read_file(Path::new("/etc/shadow")).unwrap_err();

        assert!(matches!(error, SandboxError::PermissionDenied(_)));
    }

    #[cfg(unix)]
    #[test]
    fn read_file_rejects_symlinks_that_escape_a_mount() {
        use std::os::unix::fs::symlink;

        let host = temporary_directory("read-symlink-host");
        let outside = temporary_directory("read-symlink-outside");
        fs::write(outside.join("secret.txt"), "secret").unwrap();
        symlink(&outside, host.join("escape")).unwrap();
        let filesystem = SandboxedFilesystem::new(
            vec![BindMount {
                host: host.clone(),
                guest: PathBuf::from("/workspace"),
            }],
            vec![],
        );

        let error = filesystem
            .read_file(Path::new("/workspace/escape/secret.txt"))
            .unwrap_err();

        assert!(matches!(error, SandboxError::PermissionDenied(_)));
        fs::remove_dir_all(host).unwrap();
        fs::remove_dir_all(outside).unwrap();
    }
}
