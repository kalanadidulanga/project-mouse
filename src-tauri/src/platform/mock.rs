//! Test doubles + non-Windows no-ops. Sharing one `Arc` inner lets a test hold a handle to
//! the same guard the reconciler owns, and read back what it did.
// Items here are used only under #[cfg(test)] or #[cfg(not(windows))]; dead in a plain
// Windows build by design.
#![allow(dead_code)]

use std::sync::{Arc, Mutex};

use super::{PowerGuard, Result};

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
