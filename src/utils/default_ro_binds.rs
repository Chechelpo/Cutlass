use crate::agent::sandbox::filesystem::BindMount;
use std::path::PathBuf;

pub fn ro_binds() -> Vec<BindMount> {
    vec![
        // Executables
        BindMount {
            host: PathBuf::from("/bin"),
            guest: PathBuf::from("/bin"),
        },
        BindMount {
            host: PathBuf::from("/usr/bin"),
            guest: PathBuf::from("/usr/bin"),
        },
        // Shared libraries
        BindMount {
            host: PathBuf::from("/lib"),
            guest: PathBuf::from("/lib"),
        },
        BindMount {
            host: PathBuf::from("/lib64"),
            guest: PathBuf::from("/lib64"),
        },
        BindMount {
            host: PathBuf::from("/usr/lib"),
            guest: PathBuf::from("/usr/lib"),
        },
        // Basic system information
        BindMount {
            host: PathBuf::from("/etc"),
            guest: PathBuf::from("/etc"),
        },
        // CA certificates for HTTPS
        BindMount {
            host: PathBuf::from("/etc/ssl"),
            guest: PathBuf::from("/etc/ssl"),
        },
        // Timezone information
        BindMount {
            host: PathBuf::from("/usr/share/zoneinfo"),
            guest: PathBuf::from("/usr/share/zoneinfo"),
        },
        // Shell environments often expect this
        BindMount {
            host: PathBuf::from("/usr/share"),
            guest: PathBuf::from("/usr/share"),
        },
    ]
}
