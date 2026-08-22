//! Test doubles + non-Windows no-ops. Sharing one `Arc` inner lets a test hold a handle to
//! the same guard the reconciler owns, and read back what it did.
// Items here are used only under #[cfg(test)] or #[cfg(not(windows))]; dead in a plain
// Windows build by design.
#![allow(dead_code)]

use std::sync::{Arc, Mutex};

use super::{ForegroundMonitor, InputInjector, PowerGuard, PowerSource, ProcessMonitor, Result};
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
        self.inner.calls.lock().unwrap().push(format!("set(display={display}, reason={reason})"));
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
}

/// Test double: counts injections.
#[derive(Clone, Default)]
pub struct MockInjector {
    pub jiggles: Arc<Mutex<u32>>,
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
