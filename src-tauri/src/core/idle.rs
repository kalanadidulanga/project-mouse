//! Two idle clocks + the self-injection filter (ARCHITECTURE §6). Pure: fed `GetTickCount`-style
//! u32 ticks, no OS. This is the P0 core — without it the tool cannot tell that the user came back,
//! because it manufactures input indistinguishable from theirs.
//!
//! All ticks are u32 and compared with wrapping arithmetic (WINDOWS-API gotcha 1: 49-day wrap +
//! non-monotonic `dwTime`).

const TOLERANCE_MS: u32 = 250;
const SANITY_MAX_MS: u32 = 1000 * 60 * 60 * 24 * 7; // a week

fn wrapping_abs_diff(a: u32, b: u32) -> u32 {
    a.wrapping_sub(b).min(b.wrapping_sub(a))
}

fn clamp(ms: u32) -> u32 {
    if ms > SANITY_MAX_MS {
        0
    } else {
        ms
    }
}

pub struct IdleTracker {
    /// Tick of the last input believed to be human (our injections filtered out).
    human_last_input: u32,
    /// Tick of our most recent injection.
    injected_at: Option<u32>,
}

impl IdleTracker {
    pub fn new(now: u32) -> Self {
        Self { human_last_input: now, injected_at: None }
    }

    /// Record that we injected input at `now`.
    pub fn note_injection(&mut self, now: u32) {
        self.injected_at = Some(now);
    }

    /// Observe the OS's last-input tick (`GetLastInputInfo.dwTime`). If it lines up with our recent
    /// injection, it was us — leave the human clock alone. Otherwise a real human moved.
    pub fn observe(&mut self, system_last_input: u32) {
        let is_ours = self
            .injected_at
            .is_some_and(|t| wrapping_abs_diff(system_last_input, t) < TOLERANCE_MS);
        if !is_ours {
            self.human_last_input = system_last_input;
        }
    }

    /// Idle as Windows / presence / the screensaver see it (includes our injections).
    pub fn system_idle_ms(&self, system_last_input: u32, now: u32) -> u32 {
        clamp(now.wrapping_sub(system_last_input))
    }

    /// Idle as the *human* actually behaved (our injections filtered out).
    pub fn human_idle_ms(&self, now: u32) -> u32 {
        clamp(now.wrapping_sub(self.human_last_input))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_clock_rises_while_we_jiggle() {
        // Human last moved at t=0 and then went away.
        let mut t = IdleTracker::new(0);
        // 3 minutes later the app notices the human is idle and injects.
        t.observe(0); // no injection yet → human_last = 0
        assert_eq!(t.human_idle_ms(180_000), 180_000);
        t.note_injection(180_000);
        // Half a second later: the OS's last-input tick is our injection (180_000).
        t.observe(180_000);
        // System sees recent activity (our jiggle); the human clock keeps rising.
        assert_eq!(t.system_idle_ms(180_000, 180_500), 500);
        assert_eq!(t.human_idle_ms(180_500), 180_500);
    }

    #[test]
    fn real_input_resets_the_human_clock() {
        let mut t = IdleTracker::new(0);
        t.note_injection(180_000);
        t.observe(180_000); // ours
        assert_eq!(t.human_idle_ms(180_500), 180_500);
        // Real user moves at 181_000 — well outside the injection tolerance.
        t.observe(181_000);
        assert_eq!(t.human_idle_ms(181_000), 0); // stand-down: human is back
    }

    #[test]
    fn injection_within_tolerance_is_ours() {
        let mut t = IdleTracker::new(0);
        t.note_injection(1000);
        t.observe(1100); // 100ms after our injection → still us
        assert_eq!(t.human_idle_ms(1100), 1100); // human clock unaffected
    }

    #[test]
    fn nonsense_idle_is_clamped_to_zero() {
        let t = IdleTracker::new(0);
        // system_last_input in the future relative to now → wrapped huge value → clamp to 0.
        assert_eq!(t.system_idle_ms(1000, 0), 0);
    }
}
