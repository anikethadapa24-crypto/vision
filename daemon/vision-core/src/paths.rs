use std::env;
use std::path::PathBuf;

/// The base data directory Vision owns on this machine — everything under
/// `docs/ARCHITECTURE.md` §5.1's storage layout (graph/, vectors/,
/// config.sqlite, the single-instance lock file, ...) lives beneath it.
///
/// Panics if the OS-specific environment variable it depends on
/// (`%LOCALAPPDATA%` on Windows, `$HOME` elsewhere) isn't set — a daemon
/// can't run without knowing where its own state lives, so failing fast
/// here beats limping along with `None`.
pub fn base_data_dir() -> PathBuf {
    #[cfg(windows)]
    {
        let local_app_data = env::var_os("LOCALAPPDATA")
            .expect("%LOCALAPPDATA% is not set — cannot locate Vision's data directory");
        PathBuf::from(local_app_data).join("Vision")
    }

    #[cfg(target_os = "macos")]
    {
        let home = env::var_os("HOME").expect("$HOME is not set");
        PathBuf::from(home)
            .join("Library")
            .join("Application Support")
            .join("Vision")
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Some(xdg_data_home) = env::var_os("XDG_DATA_HOME") {
            return PathBuf::from(xdg_data_home).join("vision");
        }
        let home = env::var_os("HOME").expect("$HOME is not set");
        PathBuf::from(home).join(".local").join("share").join("vision")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_data_dir_ends_with_the_product_name() {
        let dir = base_data_dir();
        let last_component = dir.file_name().unwrap().to_string_lossy().to_lowercase();
        assert_eq!(last_component, "vision");
    }
}
