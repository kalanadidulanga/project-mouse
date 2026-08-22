//! Builds a `Snapshot` from the OS each tick, cadence-limiting the (relatively) expensive process
//! enumeration to ~5 s while cheap fields refresh every tick. Other monitors (session, foreground,
//! battery, CPU) fill in as those platform backends land; unset fields keep their `Snapshot`
//! defaults for now.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::core::snapshot::Snapshot;
use crate::platform::{self, ProcessMonitor};

pub struct Sampler {
    processes: Arc<dyn ProcessMonitor>,
    proc_cache: Mutex<(Option<Instant>, Vec<String>)>,
    proc_interval: Duration,
}

impl Sampler {
    pub fn new(processes: Arc<dyn ProcessMonitor>) -> Self {
        Self {
            processes,
            proc_cache: Mutex::new((None, Vec::new())),
            proc_interval: Duration::from_secs(5),
        }
    }

    fn process_names(&self) -> Vec<String> {
        let mut cache = self.proc_cache.lock().unwrap();
        let stale = cache.0.map_or(true, |t| t.elapsed() >= self.proc_interval);
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
        Snapshot {
            epoch_secs,
            weekday,
            minutes,
            running_processes: self.process_names(),
            ..Default::default()
        }
    }
}
