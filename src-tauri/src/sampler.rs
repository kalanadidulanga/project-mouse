//! Builds a `Snapshot` from the OS each tick, cadence-limiting the (relatively) expensive process
//! enumeration to ~5 s while cheap fields refresh every tick.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::core::rule::NotifState;
use crate::core::snapshot::Snapshot;
use crate::platform::{self, ForegroundMonitor, PowerSource, ProcessMonitor, SessionMonitor};

pub struct Sampler {
    processes: Arc<dyn ProcessMonitor>,
    foreground: Arc<dyn ForegroundMonitor>,
    power_source: Arc<dyn PowerSource>,
    session: Arc<dyn SessionMonitor>,
    proc_cache: Mutex<(Option<Instant>, Vec<String>)>,
    proc_interval: Duration,
    /// The most recent sample, so a `#[tauri::command]` can read live state without re-sampling.
    last: Mutex<Snapshot>,
}

impl Sampler {
    pub fn new(
        processes: Arc<dyn ProcessMonitor>,
        foreground: Arc<dyn ForegroundMonitor>,
        power_source: Arc<dyn PowerSource>,
        session: Arc<dyn SessionMonitor>,
    ) -> Self {
        Self {
            processes,
            foreground,
            power_source,
            session,
            proc_cache: Mutex::new((None, Vec::new())),
            proc_interval: Duration::from_secs(5),
            last: Mutex::new(Snapshot::default()),
        }
    }

    /// The most recent snapshot the scheduler took. Never samples the OS itself.
    pub fn last(&self) -> Snapshot {
        self.last.lock().unwrap().clone()
    }

    fn process_names(&self) -> Vec<String> {
        let mut cache = self.proc_cache.lock().unwrap();
        let stale = cache.0.is_none_or(|t| t.elapsed() >= self.proc_interval);
        if stale {
            cache.1 = self.processes.running_process_names();
            cache.0 = Some(Instant::now());
        }
        cache.1.clone()
    }

    pub fn snapshot(&self) -> Snapshot {
        let (weekday, minutes) = platform::local_time();
        let epoch_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let notification_state = self.foreground.notification_state();
        let foreground_exe = self.foreground.foreground_app();
        let (on_ac, battery_pct) = self.power_source.power_status();
        let snap = Snapshot {
            epoch_secs,
            weekday,
            minutes,
            running_processes: self.process_names(),
            foreground_exe,
            // ponytail: derive lock from the notification state's NotPresent (the docs' cheapest
            // reliable "locked or screensaver" signal); precise WTS lock/unlock events are a later
            // refinement (B6).
            session_locked: notification_state == NotifState::NotPresent,
            notification_state,
            on_ac,
            battery_pct,
            remote_session: self.session.is_remote_session(),
            ..Default::default()
        };
        *self.last.lock().unwrap() = snap.clone();
        snap
    }
}
