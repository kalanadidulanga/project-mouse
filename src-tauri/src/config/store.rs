//! Config location + atomic load/save (FEATURES D8). On a parse error we surface and KEEP the
//! file — never silently reset to defaults (that is Move Mouse's mistake).

use std::path::{Path, PathBuf};

use super::migrate;
use super::model::Config;

const APP_DIR: &str = "project-mouse";
const CONFIG_FILE: &str = "config.json";

#[derive(Debug)]
pub enum ConfigError {
    Io(std::io::Error),
    /// Corrupt or unmigratable — the file is kept; the caller must surface this and NOT overwrite.
    Parse(String),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::Io(e) => write!(f, "config io error: {e}"),
            ConfigError::Parse(m) => write!(f, "config parse error: {m}"),
        }
    }
}

/// Pure decision (testable without the filesystem): portable — beside the exe — when the exe's
/// directory is writable; otherwise roaming (`%APPDATA%\project-mouse`); falling back to
/// beside-the-exe when there is no APPDATA at all.
pub fn choose_config_path(
    exe_dir: &Path,
    appdata: Option<&Path>,
    exe_dir_writable: bool,
) -> PathBuf {
    if exe_dir_writable {
        exe_dir.join(CONFIG_FILE)
    } else if let Some(ad) = appdata {
        ad.join(APP_DIR).join(CONFIG_FILE)
    } else {
        exe_dir.join(CONFIG_FILE)
    }
}

/// Resolve the real config path on this machine, probing writability by touching a temp file.
pub fn resolve_config_path() -> PathBuf {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."));
    let appdata = std::env::var_os("APPDATA").map(PathBuf::from);
    let writable = dir_is_writable(&exe_dir);
    choose_config_path(&exe_dir, appdata.as_deref(), writable)
}

fn dir_is_writable(dir: &Path) -> bool {
    let probe = dir.join(".pm-write-probe");
    match std::fs::File::create(&probe) {
        Ok(_) => {
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

/// Load config. Missing file → defaults (a fresh install, not an error). Present-but-corrupt →
/// `Err(Parse)`, and the file is left untouched for recovery.
pub fn load(path: &Path) -> Result<Config, ConfigError> {
    match std::fs::read_to_string(path) {
        Ok(s) => {
            let value: serde_json::Value = serde_json::from_str(&s)
                .map_err(|e| ConfigError::Parse(format!("invalid JSON: {e}")))?;
            migrate::migrate(value).map_err(ConfigError::Parse)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
        Err(e) => Err(ConfigError::Io(e)),
    }
}

/// Atomic save: write a temp file, fsync it, then rename over the target. `std::fs::rename` maps
/// to `MoveFileExW(REPLACE_EXISTING|WRITE_THROUGH)` on Windows, so the target is only ever the
/// complete old or the complete new file — a kill mid-write cannot corrupt it (FEATURES D8).
pub fn save_atomic(path: &Path, cfg: &Config) -> std::io::Result<()> {
    use std::io::Write;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    {
        let mut f = std::fs::File::create(&tmp)?;
        let data = serde_json::to_vec_pretty(cfg)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        f.write_all(&data)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::modes::WakeMode;
    use std::sync::atomic::{AtomicU32, Ordering};

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    fn temp_path() -> PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("pm-cfg-test-{}-{n}.json", std::process::id()))
    }

    // --- path detection (US2) ---

    #[test]
    fn portable_when_exe_dir_writable() {
        let p = choose_config_path(
            Path::new("C:/tools/pm"),
            Some(Path::new("C:/AppData/Roaming")),
            true,
        );
        assert_eq!(p, Path::new("C:/tools/pm/config.json"));
    }

    #[test]
    fn roaming_when_exe_dir_readonly() {
        let p = choose_config_path(
            Path::new("C:/Program Files/pm"),
            Some(Path::new("C:/AppData/Roaming")),
            false,
        );
        assert_eq!(p, Path::new("C:/AppData/Roaming/project-mouse/config.json"));
    }

    #[test]
    fn falls_back_beside_exe_when_no_appdata() {
        let p = choose_config_path(Path::new("C:/ro/pm"), None, false);
        assert_eq!(p, Path::new("C:/ro/pm/config.json"));
    }

    // --- load / save (US3) ---

    #[test]
    fn round_trips() {
        let path = temp_path();
        let cfg = Config::with_mode(WakeMode::KeepPresenting);
        save_atomic(&path, &cfg).unwrap();
        let loaded = load(&path).unwrap();
        assert_eq!(loaded, cfg);
        // no temp left behind
        assert!(!path.with_extension("json.tmp").exists());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn missing_file_is_defaults_not_error() {
        let path = temp_path(); // never created
        assert_eq!(load(&path).unwrap(), Config::default());
    }

    #[test]
    fn corrupt_file_errors_and_is_kept() {
        let path = temp_path();
        std::fs::write(&path, b"{ this is not valid json ").unwrap();
        let result = load(&path);
        assert!(matches!(result, Err(ConfigError::Parse(_))));
        // the broken file is preserved for recovery — never silently reset
        assert!(path.exists());
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "{ this is not valid json "
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn save_replaces_existing_atomically() {
        let path = temp_path();
        save_atomic(&path, &Config::with_mode(WakeMode::Off)).unwrap();
        save_atomic(&path, &Config::with_mode(WakeMode::KeepRunning)).unwrap();
        assert_eq!(load(&path).unwrap().mode, WakeMode::KeepRunning);
        let _ = std::fs::remove_file(&path);
    }
}
