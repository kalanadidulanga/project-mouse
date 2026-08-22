//! `tracing` → rolling file (FEATURES D6). The returned guard must be kept alive for the
//! process lifetime or buffered lines are lost on exit.
//!
//! ponytail: daily rotation (tracing-appender's only built-in) instead of the docs' 1 MB × 3.
//! `get_logs` reads the newest file's tail rather than keeping an in-memory ring buffer — the
//! window opens minutes a day, so file IO on open is cheaper than a resident buffer.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use tracing_appender::non_blocking::WorkerGuard;

static LOG_DIR: OnceLock<PathBuf> = OnceLock::new();

pub fn init(log_dir: &Path) -> Option<WorkerGuard> {
    let _ = std::fs::create_dir_all(log_dir);
    let _ = LOG_DIR.set(log_dir.to_path_buf());
    let appender = tracing_appender::rolling::daily(log_dir, "project-mouse.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(appender);

    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(non_blocking)
        .with_ansi(false)
        .try_init();

    Some(guard)
}

/// The most recent `n` log lines from the newest log file (for the Activity view).
pub fn tail(n: usize) -> Vec<String> {
    let Some(dir) = LOG_DIR.get() else {
        return Vec::new();
    };
    let newest = std::fs::read_dir(dir).ok().and_then(|rd| {
        rd.filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| {
                p.file_name()
                    .is_some_and(|f| f.to_string_lossy().starts_with("project-mouse.log"))
            })
            .max_by_key(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok())
    });
    let Some(file) = newest else {
        return Vec::new();
    };
    let content = std::fs::read_to_string(&file).unwrap_or_default();
    let mut lines: Vec<String> = content.lines().rev().take(n).map(str::to_string).collect();
    lines.reverse();
    lines
}
