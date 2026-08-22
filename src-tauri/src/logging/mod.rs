//! `tracing` → rolling file (FEATURES D6). The returned guard must be kept alive for the
//! process lifetime or buffered lines are lost on exit.
//!
//! ponytail: daily rotation (tracing-appender's only built-in) instead of the docs' 1 MB × 3.
//! Upgrade to size-based rotation if log volume ever justifies a rotation crate.

use std::path::Path;

use tracing_appender::non_blocking::WorkerGuard;

pub fn init(log_dir: &Path) -> Option<WorkerGuard> {
    let _ = std::fs::create_dir_all(log_dir);
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
