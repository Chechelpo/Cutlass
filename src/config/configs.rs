use std::path::PathBuf;

use directories::ProjectDirs;

/// Return Cutlass's conventional per-user configuration directory.
///
/// This follows the platform conventions exposed by the operating system:
/// XDG configuration on Linux, Application Support on macOS, and roaming
/// application data on Windows.
pub fn get_config_location() -> Option<PathBuf> {
    ProjectDirs::from("dev", "cutlass", "Cutlass")
        .map(|directories| directories.config_dir().to_path_buf())
}
