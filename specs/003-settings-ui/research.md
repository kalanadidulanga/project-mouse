# Phase 0 Research: M3 leftovers

**Feature**: `003-settings-ui` · **Date**: 2026-08-28

Only the genuinely unknown parts are researched here. The rest of M3 shipped in
`f77f51d`, `3b2f2a3`, `50926be`, `16c5ec1`.

---

## R1 — Can an unelevated process answer "why is my PC awake?" (E1)

This is the flagship diagnostic ([FEATURES E1](../../docs/FEATURES.md#e1-why-is-my-pc-awake)) and
it collides head-on with a constitution constraint: **never requires admin**.

`docs/WINDOWS-API.md` predicted "unelevated, expect a **partial** list". Measured on
Windows 11 Pro 26200, unelevated, it is worse than that.

### Measurements

| Probe | Result |
|---|---|
| `powercfg /requests` (shell out) | **exit 1**, 37 ms — *"This command requires administrator privileges and must be executed from an elevated command prompt."* No partial output. |
| `CallNtPowerInformation(GetPowerRequestList=45)`, out-buffer 0 / 16 / 1 KiB / 64 KiB | **`STATUS_INVALID_PARAMETER` (0xC000000D)** at every size |
| Controls: `LastSleepTime`(15), `LastWakeTime`(14), `SystemPowerInformation`(12) | **`STATUS_SUCCESS`** — so the calling convention is correct and level 45 is genuinely refusing us |
| `CallNtPowerInformation(SystemExecutionState=16)` | **`STATUS_SUCCESS`**, returned `ES=0x00000002` |
| Same, after the probe took its own `ES_SYSTEM_REQUIRED\|ES_DISPLAY_REQUIRED` | `ES=0x00000003` — the value **aggregates across the whole system** |

The control row is the important one: three sibling info levels succeed with identical argument
shapes, so `STATUS_INVALID_PARAMETER` on level 45 is a privilege gate, not a buffer-size bug.

### Decision

**Do not implement `power/inspect.rs` on top of `GetPowerRequestList`.** Build E1 on
`SystemExecutionState` (16) instead.

Unelevated we cannot *name* the process holding a request. We can determine, truthfully and with
no admin, **whether anything on this machine is holding one, and which kind** — and we know our
own contribution exactly, because we made it. Subtracting the two yields the honest headline:

> *Something other than project-mouse is keeping your display on.*

…followed by the copyable elevated command that names it. This is precisely the shape
[WINDOWS-API.md](../../docs/WINDOWS-API.md) prescribes for the unelevated case: *"show what it can
see and say plainly that a complete list needs an elevated `powercfg /requests` — with a copy
button for the command… it avoids asking for admin rights we promised never to need."*

### Rationale

- **It is the only route that returns data at all.** Both alternatives return nothing unelevated.
- **It cannot be wrong about memory.** `EXECUTION_STATE` is a documented `u32` bitmask, not an
  undocumented struct of relative offsets. `POWER_REQUEST_LIST` is not in the SDK headers and
  would have to be hand-declared; parsing it in `unsafe` from a half-known layout is exactly the
  class of bug that this feature exists to argue we do not write.
- **It still answers the user's question**, which is "is something holding my machine awake, and
  is it me?" Naming the offender is the second question, and we hand over the exact command.

### Alternatives rejected

| Alternative | Why not |
|---|---|
| Hand-declare `POWER_REQUEST_LIST` + parse | Level 45 refuses us entirely — there is nothing to parse. Moot before the safety argument is even reached. |
| Shell out to `powercfg /requests` | Refuses unelevated (measured). Also spawns a process and flashes a console window. |
| Request elevation / add a manifest | Breaks *"never requires admin"* (constitution, Technology & Footprint Constraints) and the trust argument in [PRODUCT.md §8](../../docs/PRODUCT.md#8-positioning). Non-negotiable. |
| Ship an elevated helper service | Same trust problem, plus an installer and a service to maintain. Wildly out of proportion to one panel. |

### Open verification (implementation-time)

The probe proved `SetThreadExecutionState` is reflected in `SystemExecutionState`. The app holds
its request via `PowerSetRequest`, not `SetThreadExecutionState`. **Whether our own
`PowerSetRequest` shows up in the aggregate must be measured before the "and it isn't us"
subtraction is trusted.** Both outcomes are shippable — if it is not reflected, the aggregate
already means "someone else", and the subtraction is simply dropped. Covered by task T004b.

---

## R2 — Remote-session detection (002 T012)

`GetSystemMetrics(SM_REMOTESESSION)` is documented, unprivileged, and a single call. The
`WM_WTSSESSION_CHANGE` half of the original task needs `WTSRegisterSessionNotification` and a
window to receive messages — this app has no window at rest, which is the whole memory design.

**Decision**: poll `SM_REMOTESESSION` in the existing ~1 s sampler tick alongside every other
signal. `Snapshot.remote_session` is already declared and already `#[allow(dead_code)]`; this
just fills it. No new thread, no message pump, no window.

---

## R3 — Where profile switching lives (002 T017)

Spec FR-002 lists `set_profile` in the IPC surface; it was never added, and `Config` already
persists a `profiles: Vec<Profile>` with an `active_profile` id while the engine holds exactly
one `Profile`.

**Decision**: keep the engine single-profile (it reconciles one rule set — that is correct) and
move the *collection* into the config layer, which already models it. `set_profile(id)` swaps
which profile the engine holds; `list_profiles` reads the collection. The tray gets a submenu
(002 T017) and the UI a switcher.

The one real hazard: `persist_current` currently writes `profiles: vec![profile]` — it
**flattens the collection to whatever the engine happens to hold**, so a second profile would be
destroyed on the next save. That is a live data-loss bug today, latent only because nothing can
create a second profile yet. Fixing it is a prerequisite, not a nicety.
