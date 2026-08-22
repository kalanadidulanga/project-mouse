//! Config file location. US2 = where the file lives (portable vs roaming). The atomic
//! read/write, model, and migrations are US3 (FEATURES D8).

use std::path::{Path, PathBuf};

const APP_DIR: &str = "project-mouse";
const CONFIG_FILE: &str = "config.json";

/// Pure decision (testable without touching the filesystem): portable — beside the exe — when
/// the exe's directory is writable; otherwise roaming (`%APPDATA%\project-mouse`); falling back
/// to beside-the-exe when there is no APPDATA at all.
pub fn choose_config_path(exe_dir: &Path, appdata: Option<&Path>, exe_dir_writable: bool) -> PathBuf {
    if exe_dir_writable {
        exe_dir.join(CONFIG_FILE)
    } else if let Some(ad) = appdata {
        ad.join(APP_DIR).join(CONFIG_FILE)
    } else {
        exe_dir.join(CONFIG_FILE)
    }
}

/// Resolve the real config path on this machine, probing writability by touching a temp file.
#[allow(dead_code)] // wired into config load/save in US3
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portable_when_exe_dir_writable() {
        let p = choose_config_path(
            Path::new("C:/tools/pm"),
            Some(Path::new("C:/Users/x/AppData/Roaming")),
            true,
        );
        assert_eq!(p, Path::new("C:/tools/pm/config.json"));
    }

    #[test]
    fn roaming_when_exe_dir_readonly() {
        let p = choose_config_path(
            Path::new("C:/Program Files/pm"),
            Some(Path::new("C:/Users/x/AppData/Roaming")),
            false,
        );
        assert_eq!(p, Path::new("C:/Users/x/AppData/Roaming/project-mouse/config.json"));
    }

    #[test]
    fn falls_back_beside_exe_when_no_appdata() {
        let p = choose_config_path(Path::new("C:/ro/pm"), None, false);
        assert_eq!(p, Path::new("C:/ro/pm/config.json"));
    }
}
