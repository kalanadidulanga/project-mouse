//! "Why is my PC awake?" — the pure reduction (FEATURES E1).
//!
//! An unelevated process cannot *name* the holder of a power request: `GetPowerRequestList`
//! refuses us outright and `powercfg /requests` demands an elevated prompt (see
//! `specs/003-settings-ui/research.md` R1). What it *can* read is the `EXECUTION_STATE`
//! aggregate.
//!
//! We deliberately do **not** subtract our own request from that aggregate. Whether a
//! `PowerSetRequest` handle is reflected in `SystemExecutionState` could not be verified — the
//! test machine's aggregate was saturated, so no addition was observable (research R1, T014).
//! Subtracting on an unverified assumption would *suppress* a genuine third-party request
//! whenever we happened to be holding the same kind, which is the worse failure. So the
//! aggregate is reported exactly as Windows gives it, our own state is reported separately and
//! exactly, and the UI lets the reader put the two together.

use serde::Serialize;

use crate::core::modes::WakeMode;

/// `EXECUTION_STATE` bits, from `winbase.h`.
pub const ES_SYSTEM_REQUIRED: u32 = 0x0000_0001;
pub const ES_DISPLAY_REQUIRED: u32 = 0x0000_0002;
pub const ES_AWAYMODE_REQUIRED: u32 = 0x0000_0040;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct AwakeReport {
    /// `false` when the OS refused the read. The UI must then say so rather than render a
    /// confident "nothing is held".
    pub readable: bool,
    pub system_held: bool,
    pub display_held: bool,
    pub away_mode_held: bool,
    /// What *we* hold. Known exactly — we made the request. Reported alongside the aggregate,
    /// never folded into it.
    pub ours: WakeMode,
}

/// `aggregate` is the machine-wide `EXECUTION_STATE`, or `None` if the OS refused the read.
pub fn report(aggregate: Option<u32>, ours: WakeMode) -> AwakeReport {
    let es = aggregate.unwrap_or(0);
    let system_held = es & ES_SYSTEM_REQUIRED != 0;
    let display_held = es & ES_DISPLAY_REQUIRED != 0;
    AwakeReport {
        readable: aggregate.is_some(),
        system_held,
        display_held,
        away_mode_held: es & ES_AWAYMODE_REQUIRED != 0,
        ours,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_held_is_reported_as_nothing_held() {
        let r = report(Some(0), WakeMode::Off);
        assert!(r.readable);
        assert!(!r.system_held && !r.display_held && !r.away_mode_held);
    }

    #[test]
    fn a_refused_read_is_flagged_not_silently_empty() {
        let r = report(None, WakeMode::KeepPresenting);
        assert!(
            !r.readable,
            "the UI must be able to say 'we could not read this'"
        );
        assert!(
            !r.system_held && !r.display_held && !r.away_mode_held,
            "an unreadable aggregate must never be rendered as a confident negative"
        );
        assert_eq!(
            r.ours,
            WakeMode::KeepPresenting,
            "our own state is still known"
        );
    }

    #[test]
    fn each_bit_maps_to_its_flag() {
        let r = report(
            Some(ES_SYSTEM_REQUIRED | ES_DISPLAY_REQUIRED | ES_AWAYMODE_REQUIRED),
            WakeMode::Off,
        );
        assert!(r.system_held && r.display_held && r.away_mode_held);

        let r = report(Some(ES_DISPLAY_REQUIRED), WakeMode::Off);
        assert!(r.display_held && !r.system_held && !r.away_mode_held);
    }

    /// Regression guard for the subtraction that was designed and then cut. Whether our own
    /// `PowerSetRequest` appears in `SystemExecutionState` is unverified (research R1, T014), so
    /// what we hold must never change what we report Windows reporting — otherwise a real
    /// third-party request disappears from the panel exactly when we are holding one too.
    #[test]
    fn our_own_request_never_alters_the_reported_aggregate() {
        let agg = Some(ES_SYSTEM_REQUIRED | ES_DISPLAY_REQUIRED);
        let idle = report(agg, WakeMode::Off);
        for mode in [WakeMode::KeepRunning, WakeMode::KeepPresenting] {
            let r = report(agg, mode);
            assert_eq!(
                r.system_held, idle.system_held,
                "{mode:?} changed system_held"
            );
            assert_eq!(
                r.display_held, idle.display_held,
                "{mode:?} changed display_held"
            );
        }
    }

    #[test]
    fn our_own_mode_is_carried_through_unchanged() {
        for mode in [
            WakeMode::Off,
            WakeMode::KeepRunning,
            WakeMode::KeepPresenting,
        ] {
            assert_eq!(report(Some(0), mode).ours, mode);
        }
    }
}
