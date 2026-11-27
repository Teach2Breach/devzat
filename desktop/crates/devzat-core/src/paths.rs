use std::path::PathBuf;

pub fn home_dir() -> Option<PathBuf> {
    let var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    std::env::var_os(var).filter(|v| !v.is_empty()).map(PathBuf::from)
}

/// Where the app keeps its key and trusted host keys. `DEVZAT_DESKTOP_HOME` overrides it,
/// which keeps test runs away from a real profile.
pub fn data_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("DEVZAT_DESKTOP_HOME") {
        return PathBuf::from(dir);
    }
    let base = if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        home_dir().map(|h| h.join("Library").join("Application Support"))
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .or_else(|| home_dir().map(|h| h.join(".local").join("share")))
    };
    base.map(|b| b.join("devzat-desktop")).unwrap_or_else(|| PathBuf::from(".devzat-desktop"))
}

pub fn identity_file() -> PathBuf {
    data_dir().join("identity_ed25519")
}

pub fn known_hosts_file() -> PathBuf {
    data_dir().join("known_hosts")
}

/// eframe's saved UI state, only overridden for test runs.
pub fn state_file_override() -> Option<PathBuf> {
    std::env::var_os("DEVZAT_DESKTOP_HOME").map(|d| PathBuf::from(d).join("state.ron"))
}
