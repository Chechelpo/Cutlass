use std::fs;
use std::path::{Path, PathBuf};
use crate::agent::sandbox::sandbox::SandboxError;

pub struct BindMount {
    pub host: PathBuf,
    pub guest: PathBuf,
}

pub struct SandboxedFilesystem {
    ro_binds: Vec<BindMount>,
    w_binds: Vec<BindMount>,
}

impl SandboxedFilesystem {
    pub fn new(
        ro_binds: Vec<BindMount>,
        w_binds: Vec<BindMount>,
    ) -> SandboxedFilesystem {
        SandboxedFilesystem {
            ro_binds,
            w_binds,
        }
    }
    pub fn ro_binds(&self) -> &[BindMount] {
        self.ro_binds.as_slice()
    }
    pub fn w_binds(&self) -> &[BindMount] {
        self.w_binds.as_slice()
    }

    pub fn in_read_bounds(
        &self,
        path: &Path,
    ) -> bool {
        inside_any(&self.ro_binds, path)
            || self.in_write_bounds(path)
    }

    pub fn in_write_bounds(
        &self,
        path: &Path,
    ) -> bool {
        inside_any(&self.w_binds, path)
    }

    pub fn read_file(
        &self,
        path: &Path,
    ) -> Result<String, SandboxError> {
        if !self.in_read_bounds(path) {
            return Err(SandboxError::PermissionDenied(
                format!(
                    "Cannot read path: {}",
                    path.display()
                ),
            ));
        }

        fs::read_to_string(path)
            .map_err(SandboxError::Io)
    }
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

fn is_path_within_base(
    path: &Path,
    base: &Path,
) -> bool {
    match (
        normalize_path(path),
        normalize_path(base),
    ) {
        (Some(norm_path), Some(norm_base)) => {
            norm_path.starts_with(norm_base)
        }

        _ => false,
    }
}

fn inside_any(
    mounts: &[BindMount],
    path: &Path,
) -> bool {
    mounts.iter().any(|mount| {
        is_path_within_base(
            path,
            &mount.guest,
        )
    })
}


#[cfg(test)]
mod tests {
    use super::*;

    fn bind(host: &str, guest: &str) -> BindMount {
        BindMount {
            host: PathBuf::from(host),
            guest: PathBuf::from(guest),
        }
    }

    fn filesystem() -> SandboxedFilesystem {
        SandboxedFilesystem::new(
            vec![
                bind(
                    "/usr",
                    "/usr",
                ),
                bind(
                    "/shared",
                    "/shared",
                ),
            ],
            vec![
                bind(
                    "/project",
                    "/workspace",
                ),
                bind(
                    "/shared",
                    "/shared",
                ),
            ],
        )
    }

    #[test]
    fn read_workspace_allows_read_bounds() {
        let fs = filesystem();

        assert!(
            fs.in_read_bounds(
                Path::new("/usr/bin/bash")
            )
        );

        assert!(
            fs.in_read_bounds(
                Path::new("/usr/lib/libc.so")
            )
        );
    }

    #[test]
    fn read_workspace_allows_write_bounds() {
        let fs = filesystem();

        assert!(
            fs.in_read_bounds(
                Path::new("/workspace/file.txt")
            )
        );
    }

    #[test]
    fn write_workspace_allows_only_write_bounds() {
        let fs = filesystem();

        assert!(
            fs.in_write_bounds(
                Path::new("/workspace/file.txt")
            )
        );

        assert!(
            !fs.in_write_bounds(
                Path::new("/usr/bin/bash")
            )
        );
    }

    #[test]
    fn unrelated_paths_are_rejected() {
        let fs = filesystem();

        assert!(
            !fs.in_read_bounds(
                Path::new("/etc/shadow")
            )
        );

        assert!(
            !fs.in_write_bounds(
                Path::new("/etc/shadow")
            )
        );
    }

    #[test]
    fn exact_mount_point_is_allowed() {
        let fs = filesystem();

        assert!(
            fs.in_read_bounds(
                Path::new("/usr")
            )
        );

        assert!(
            fs.in_write_bounds(
                Path::new("/workspace")
            )
        );
    }

    #[test]
    fn parent_escape_is_rejected() {
        let fs = filesystem();

        assert!(
            !fs.in_write_bounds(
                Path::new("/workspace/../../etc/passwd")
            )
        );
    }

    #[test]
    fn normalized_paths_are_handled() {
        let fs = filesystem();

        assert!(
            fs.in_write_bounds(
                Path::new("/workspace/./src/../main.rs")
            )
        );

        assert!(
            fs.in_read_bounds(
                Path::new("/usr/./bin/../lib")
            )
        );
    }

    #[test]
    fn shared_paths_have_both_permissions() {
        let fs = filesystem();

        assert!(
            fs.in_read_bounds(
                Path::new("/shared/file.txt")
            )
        );

        assert!(
            fs.in_write_bounds(
                Path::new("/shared/file.txt")
            )
        );
    }
}