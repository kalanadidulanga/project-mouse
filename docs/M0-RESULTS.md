# M0 spike — results

**Date:** 2026-08-22 · **Machine:** Windows 11 Pro (26200), WebView2 151.0.4129.93,
Tauri 2.11.5, windows-rs 0.62.2, Rust 1.98.0 (MSVC) · **Build:** `spike-m0` release
(opt-level z, LTO fat), exe **2.56 MB**.

These numbers become the CI baseline in M5 (ROADMAP). Measured with `spike-m0/measure.ps1`
summing the **whole process tree** (host + WebView2 children) once a second — the in-app
`log_ws` only sees the host process, which undercounts, so the CSV is authoritative.

> M0 exists to answer two go/no-go questions before any real code is written
> ([ROADMAP.md](ROADMAP.md#m0--the-spike)). **Verdict: GO on the Tauri stack.**

---

## Assumption 1 — does a destroyed webview return the memory? **VALIDATED ✅**

The entire ≤8 MB budget rests on this (ARCHITECTURE §3). It holds — with one caveat worth
carrying into M1.

| Phase | Processes | Working set | Private commit |
|---|---:|---:|---:|
| **T1** tray only (cold, no window yet) | 1 | **13.6 MB** | **1.9 MB** |
| Window open (peak) | **7** | ~314 MB | ~151 MB |
| **T2** after `destroy()` + 8 s settle | **1** | 26.6 MB | 4.6 MB |
| **T3** after 20× open/destroy, settled | 1 | 27.4 MB | 5.6 MB |
| **T4** after `hide()` + 8 s settle | **7** | ~325 MB | ~155 MB |

What the data shows:

- **`destroy()` works.** Opening a window spawns 6 WebView2 child processes (~300 MB); on
  `destroy()` all six **exit** and ~290 MB is returned to the OS. Process count goes 7 → 1
  every single time.
- **No leak.** Across 20 open/destroy cycles the settled figure moved 26.6 → 27.4 MB — 0.8 MB
  over 20 cycles, i.e. noise. No upward drift. (ROADMAP M0 test 3 ✅)
- **`hide()` frees nothing** — 7 processes stay resident at ~325 MB indefinitely. This is the
  mistake every "120 MB idle Tauri app" makes, confirmed directly. `destroy()` is not optional.
  (ROADMAP M0 test 4 ✅)

### ⚠️ Caveat carried to M1 — one-time host retention

The **host** process keeps ~13 MB working set (~2.7 MB private) after the *first* window is
ever opened, and never gives it back (WebView2's environment stays loaded in-process). It is a
one-time step, **not** a per-cycle leak (T2 ≈ T3). Two consequences:

1. ROADMAP M0 test 2 ("back within 2 MB of the starting figure") is **not literally met by full
   working set** — the host retains the WebView2 loader. It *is* met in spirit: the reclaimable
   part (the children, ~290 MB) is fully reclaimed. The 2 MB criterion was written assuming
   `destroy()` returns everything; reality is "returns the children, keeps a one-time host cost."
2. The README budget is **private working set ≤ 8 MB**. This spike measured full working set and
   private *commit* (1.9 MB cold, ~5 MB post-window) — **not** private working set exactly.
   Private commit sits comfortably under 8 MB, so the budget looks achievable, but it must be
   confirmed the right way in M1:
   - measure true **private working set** (VMMap, or `QueryWorkingSetEx`), not full WS;
   - apply `EmptyWorkingSet` after teardown (ARCHITECTURE §3) and re-measure the settled figure.

**Bottom line: the create-on-demand / destroy-on-close architecture is sound. No Avalonia
fallback.** The 8 MB target needs a private-working-set measurement + a trim in M1, not a stack
change.

---

## Assumption 2 — does `PowerRequestExecutionRequired` hold an S0 machine awake?

The flagship feature (WINDOWS-API gotcha 0). The full proof (**T5**: an S0 laptop, lid closed,
still reachable after 8 h; **T6**: the same failing with `SetThreadExecutionState`) **cannot be
run here** — it needs a physical S0 laptop overnight. It stays on the release checklist as a
manual gate.

What the spike *can* confirm now is that the request is correctly formed, held on a handle, and
auditable by name:

### T7 — `powercfg /requests` while holding

<!-- T7_RESULT -->
_Elevated check running…_

### T8 — clean after kill

<!-- T8_RESULT -->
_Elevated check running…_

---

## Exit criteria scorecard (ROADMAP M0)

| # | Test | Result |
|---|---|---|
| 1 | Tray-only, read private WS ≤ 10 MB | ✅ private commit 1.9 MB cold (true private WS to confirm in M1) |
| 2 | Open → `destroy()` → within 2 MB of start | ⚠️ children fully reclaimed; host keeps one-time ~13 MB WS — see caveat |
| 3 | 20× open/destroy, no drift | ✅ +0.8 MB over 20 cycles |
| 4 | Same with `hide()`, documented | ✅ 7 procs / ~325 MB resident forever |
| 5 | S0 laptop, lid closed, 8 h reachable | ⏳ MANUAL — physical S0 hardware, release checklist |
| 6 | Same with `SetThreadExecutionState` (comparison) | ⏳ MANUAL (with T5) |
| 7 | `powercfg /requests` shows our reason string | see T7 above |
| 8 | Kill process → `powercfg /requests` clean | see T8 above |

**Decision: proceed to M1 on Tauri + windows-rs.** Feed the two caveats forward:
private-working-set measurement and `EmptyWorkingSet` trim.
