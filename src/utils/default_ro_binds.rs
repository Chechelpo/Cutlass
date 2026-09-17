use crate::agent::sandbox::filesystem::BindMount;

pub fn ro_binds() -> Vec<BindMount> {
    vec![
        // Executables
        BindMount::path("/bin"),
        BindMount::path("/usr/bin"),
        // Shared libraries
        BindMount::path("/lib"),
        BindMount::path("/lib64"),
        BindMount::path("/usr/lib"),
        // System configuration, certificates, identities, and DNS.
        BindMount::path("/etc"),
        // Git helpers and common shell data such as locales and timezones.
        BindMount::path("/usr/share"),
        // /dev and /proc are created privately by the process sandbox. Binding
        // the host paths read-only would make /dev/null unwritable.
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn default_binds_do_not_mask_paths() {
        assert!(ro_binds().iter().all(|bind| bind.host == bind.guest));
    }

    #[test]
    fn includes_git_runtime_dependencies() {
        let binds = ro_binds();
        for path in ["/usr/bin", "/usr/lib", "/etc", "/usr/share"] {
            assert!(binds.iter().any(|bind| bind.guest == Path::new(path)));
        }
    }

    #[test]
    fn leaves_virtual_filesystems_to_the_process_sandbox() {
        let binds = ro_binds();

        for path in ["/dev", "/proc"] {
            assert!(!binds.iter().any(|bind| bind.guest == Path::new(path)));
        }
    }
}
