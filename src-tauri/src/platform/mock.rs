//! Test doubles + non-Windows no-ops. Sharing one `Arc` inner lets a test hold a handle to
//! the same guard the reconciler owns, and read back what it did.
// Items here are used only under #[cfg(test)] or #[cfg(not(windows))]; dead in a plain
// Windows build by design.
#![allow(dead_code)]

use std::sync::{Arc, Mutex};

use super::{
    ForegroundMonitor, InputInjector, PowerGuard, PowerInspector, PowerSource, ProcessMonitor,
    Result, SessionMonitor,
};
use crate::core::rule::NotifState;

#[derive(Default)]
struct Inner {
    calls: Mutex<Vec<String>>,
    display_held: Mutex<Option<bool>>, // None = released; Some(display) = held
}

/// Records every `set`/`clear`, and tracks the currently-held display flag.
#[derive(Clone, Default)]
pub struct MockPowerGuard {
    inner: Arc<Inner>,
}

impl MockPowerGuard {
    pub fn calls(&self) -> Vec<String> {
        self.inner.calls.lock().unwrap().clone()
    }
    /// None = nothing held; Some(display) = held with this display flag.
    pub fn held(&self) -> Option<bool> {
        *self.inner.display_held.lock().unwrap()
    }
}

impl PowerGuard for MockPowerGuard {
    fn set(&self, display: bool, reason: &str) -> Result<()> {
        self.inner
            .calls
            .lock()
            .unwrap()
            .push(format!("set(display={display}, reason={reason})"));
        *self.inner.display_held.lock().unwrap() = Some(display);
        Ok(())
    }
    fn clear(&self) -> Result<()> {
        self.inner.calls.lock().unwrap().push("clear".into());
        *self.inner.display_held.lock().unwrap() = None;
        Ok(())
    }
}

/// No-op power guard for non-Windows builds.
#[derive(Default)]
pub struct NoopPowerGuard;
impl PowerGuard for NoopPowerGuard {
    fn set(&self, _display: bool, _reason: &str) -> Result<()> {
        Ok(())
    }
    fn clear(&self) -> Result<()> {
        Ok(())
    }
}

/// No-op process monitor for non-Windows builds.
#[derive(Default)]
pub struct NoopProcessMonitor;
impl ProcessMonitor for NoopProcessMonitor {
    fn running_process_names(&self) -> Vec<String> {
        Vec::new()
    }
}

/// No-op foreground monitor for non-Windows builds.
#[derive(Default)]
pub struct NoopForegroundMonitor;
impl ForegroundMonitor for NoopForegroundMonitor {
    fn foreground_app(&self) -> Option<String> {
        None
    }
    fn notification_state(&self) -> NotifState {
        NotifState::Normal
    }
}

/// No-op power source for non-Windows builds — assume plugged in.
#[derive(Default)]
pub struct NoopPowerSource;
impl PowerSource for NoopPowerSource {
    fn power_status(&self) -> (bool, u8) {
        (true, 100)
    }
}

/// No-op injector for non-Windows builds — synthesizes nothing.
#[derive(Default)]
pub struct NoopInjector;
impl InputInjector for NoopInjector {
    fn virtual_jiggle(&self) -> Result<()> {
        Ok(())
    }
    fn key(&self, _vk: u16) -> Result<()> {
        Ok(())
    }
    fn move_relative(&self, _dx: i32, _dy: i32) -> Result<()> {
        Ok(())
    }
}

/// Test double: counts injections and records every relative move, so a test can assert the
/// cursor was actually put back where it started.
#[derive(Clone, Default)]
pub struct MockInjector {
    pub jiggles: Arc<Mutex<u32>>,
    pub moves: Arc<Mutex<Vec<(i32, i32)>>>,
}
impl MockInjector {
    /// Net displacement of every move so far. Should be (0, 0) at the end of a full cycle.
    pub fn net_move(&self) -> (i32, i32) {
        self.moves
            .lock()
            .unwrap()
            .iter()
            .fold((0, 0), |(x, y), (dx, dy)| (x + dx, y + dy))
    }
}
impl InputInjector for MockInjector {
    fn virtual_jiggle(&self) -> Result<()> {
        *self.jiggles.lock().unwrap() += 1;
        Ok(())
    }
    fn key(&self, _vk: u16) -> Result<()> {
        *self.jiggles.lock().unwrap() += 1;
        Ok(())
    }
    fn move_relative(&self, dx: i32, dy: i32) -> Result<()> {
        *self.jiggles.lock().unwrap() += 1;
        self.moves.lock().unwrap().push((dx, dy));
        Ok(())
    }
}

/// Test double: returns whatever names the test sets.
#[derive(Clone, Default)]
pub struct MockProcessMonitor {
    names: Arc<Mutex<Vec<String>>>,
}
impl MockProcessMonitor {
    pub fn set(&self, names: Vec<String>) {
        *self.names.lock().unwrap() = names;
    }
}
impl ProcessMonitor for MockProcessMonitor {
    fn running_process_names(&self) -> Vec<String> {
        self.names.lock().unwrap().clone()
    }
}

/// No-op inspector for non-Windows builds — reports the aggregate as unreadable, which is the
/// truthful answer where we have not implemented the read.
#[derive(Default)]
pub struct NoopPowerInspector;
impl PowerInspector for NoopPowerInspector {
    fn execution_state(&self) -> Option<u32> {
        None
    }
}

/// No-op session monitor for non-Windows builds — assume a local session.
#[derive(Default)]
pub struct NoopSessionMonitor;
impl SessionMonitor for NoopSessionMonitor {
    fn is_remote_session(&self) -> bool {
        false
    }
}

/// Test double: returns whatever aggregate the test sets, including `None` for a refused read.
#[derive(Clone, Default)]
pub struct MockPowerInspector {
    state: Arc<Mutex<Option<u32>>>,
}
impl MockPowerInspector {
    pub fn set(&self, state: Option<u32>) {
        *self.state.lock().unwrap() = state;
    }
}
impl PowerInspector for MockPowerInspector {
    fn execution_state(&self) -> Option<u32> {
        *self.state.lock().unwrap()
    }
}
