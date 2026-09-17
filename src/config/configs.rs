use std::path::PathBuf;

use directories::ProjectDirs;

/// Return Cutlass's conventional per-orchestrator configuration directory.
///
/// This follows the platform conventions exposed by the operating system:
/// XDG configuration on Linux, Application Support on macOS, and roaming
/// application data on Windows.
pub fn get_config_location() -> Option<PathBuf> {
    ProjectDirs::from("dev", "cutlass", "Cutlass")
        .map(|directories| directories.config_dir().to_path_buf())
}

/// Return the platform-standard directory for Cutlass logs.
///
/// Linux provides a dedicated state directory. On platforms without one,
/// use the local data directory, since logs are machine-local application
/// data rather than user configuration.
pub fn get_log_location() -> Option<PathBuf> {
    ProjectDirs::from("dev", "cutlass", "Cutlass").map(|directories| {
        directories
            .state_dir()
            .unwrap_or_else(|| directories.data_local_dir())
            .join("logs")
    })
}
