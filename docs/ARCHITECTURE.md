# Architecture

## 0. Two engines, not one

Before anything else: this application has **two independent output mechanisms**, and the
architecture must keep them separate all the way down. See
[PRODUCT.md §2](PRODUCT.md#2-the-three-mechanisms).

```
                        ┌──────────────────┐
                        │   Rule engine    │
                        │ triggers · conds │
                        └────────┬─────────┘
                     ┌───────────┴───────────┐
                     ▼                       ▼
          ┌────────────────────┐  ┌────────────────────┐
          │   POWER ENGINE     │  │   INPUT ENGINE     │
          │  on by default     │  │  off by default    │
          │                    │  │                    │
          │  PowerSetRequest   │  │  SendInput         │
          │  holds a state     │  │  fires an event    │
          │  idempotent        │  │  not idempotent    │
          │  released on exit  │  │  nothing to release│
          └────────────────────┘  └────────────────────┘
```

They differ in kind, not just in risk:

- **The power engine holds a *state*.** A request is acquired, held, and released. Asking for
  it twice is a no-op. Its correctness property is *the request matches what the rules say, and
  is gone when we exit*.
- **The input engine fires *events*.** Each one happens once and cannot be undone. Its
  correctness property is *the right event at the right moment, and never while the user is
  working*.

State reconciliation and event dispatch are different problems. Modelling both as "actions" —
which an earlier draft of this document did, and which Move Mouse does — leads to a power
request that gets re-acquired every tick and released by accident.

**So: the power engine is reconciled once per tick against desired state. The input engine is
dispatched from rule firings.** Same rule engine, two different consumers.

---

## 1. The other idea

Everything about memory below follows from a single observation:

> A tray utility's settings window is open for roughly **two minutes a day**.
> Therefore the UI must not be resident for the other 1438.

Move Mouse's WPF stack is resident 100% of the time. Tauri's webview does not have to be.
That difference — not the language, not the framework — is where the memory budget comes from.

---

## 2. Process and thread model

Single process. Three long-lived threads at rest.

```
project-mouse.exe
│
├── [T1] Main / event loop  ──────────  Tauri runtime, tray icon, window lifecycle,
│                                       Win32 message pump, hotkey WM_HOTKEY
│
├── [T2] Scheduler                ────  waitable-timer loop, 1 tick/sec
│                                       samples state, evaluates rules,
│                                       reconciles the power engine,
│                                       dispatches input actions
│                                       *** owns the power request handle ***
│
├── [T3] Executor                 ────  runs one action sequence at a time,
│                                       cancellable, sleeps between steps
│
└── [T4..] WebView2 processes     ────  EXIST ONLY WHILE A WINDOW EXISTS
```

**Why the executor is its own thread:** an action sequence can be
`move → wait 800ms → key → wait 400ms → move back`. That must not block the scheduler tick,
and it must be cancellable the instant real user input arrives.

**Why the scheduler owns the power request:** the request is tied to a **handle** that must
outlive every individual rule evaluation. Holding it on the long-lived scheduler thread means
there is exactly one owner and exactly one place that can leak it.

⚠️ Note what we are *not* using. `SetThreadExecutionState` is the API everything in this
category reaches for, and it has two problems: its thread affinity is **undocumented folklore**
(see [WINDOWS-API gotcha 2](WINDOWS-API.md#gotcha-2--setthreadexecutionstate-is-thread-affine)),
and on Modern Standby machines with the display off **it does not actually prevent connected
standby**. `PowerCreateRequest` / `PowerSetRequest` with `PowerRequestExecutionRequired` is the
path that works, is documented, and shows up in `powercfg /requests` attributed to us by name.
See [FEATURES A2](FEATURES.md#a2-modern-standby-s0-correctness).

No thread pool of our own, and no async runtime *at rest*.

⚠️ **Correction to the obvious plan: `tokio` cannot be removed.** It is a hard, unconditional
dependency of the `tauri` crate (`features = ["rt", "rt-multi-thread", "sync", "fs", "io-util"]`)
with no feature flag to drop it. What *is* controllable is whether it ever starts:

- Tauri holds the runtime in a `static OnceLock` and constructs it lazily, on the first
  `async_runtime::spawn` or `block_on`.
- Tauri commands declared **without** `async` run on the main thread. Only `async` commands (or
  `#[tauri::command(async)]`) go through `async_runtime::spawn`.

So the rule is: **every `#[tauri::command]` in this app is synchronous.** They are all
microseconds of work — reading state, toggling a flag — and belong on the main thread anyway.
Do that and no tokio worker threads exist at idle, which is what the CPU budget actually
depends on.

The one unavoidable exception is the updater, which is async. The moment an update check runs,
a **multi-threaded** tokio runtime is created for the remaining process lifetime (worker threads
≈ CPU count). Mitigation: run update checks on a dedicated `std::thread` with a
`current_thread` runtime, so the global multi-threaded one is never touched. Measure both ways —
this is the single biggest threat to the idle-RAM number after the webview itself.

For our own work, plain `std::thread` plus Win32 waitable timers.

---

## 3. Window lifecycle — where the memory budget lives

```
App start
   │
   ├─ tray icon created                              RSS ≈ 6 MB
   ├─ NO window built  ← default; --show overrides
   │
User clicks tray "Settings"
   │
   ├─ WebviewWindowBuilder::new(...)                 RSS ≈ 130 MB
   ├─ UI subscribes to `state:changed` events
   │
User closes the window
   │
   ├─ CloseRequested → api.prevent_close()
   ├─ window.destroy()          ← DESTROY, not hide
   ├─ WebView2 processes exit                        RSS ≈ 7 MB
   └─ optional: EmptyWorkingSet() to return the peak to the OS
```

### The rule that must never be broken

**`hide()` does not free memory. `destroy()` should.**

A hidden webview keeps its renderer, GPU, and utility processes alive and its JS heap resident.
Every "lightweight Tauri app" that measures at 120 MB idle has made exactly this mistake.
Rebuilding the window costs roughly 180 ms — imperceptible for something opened twice a day.

⚠️ **"should", not "does" — this is an assumption, not a documented guarantee.** Tauri's docs
describe `destroy()` only as *"Similar to `close` but does not emit any events and force close
the window instead."* There is no statement anywhere in the Tauri documentation about memory or
resource release on window destruction, and each WebView2 window runs its own browser and GPU
processes whose shared environment may or may not be torn down with the last webview.

**The entire 8 MB budget rests on this one unverified behaviour.** It is therefore the first
thing to prototype — before the rule engine, before the UI. A day-one spike that starts a tray
app, opens a window, destroys it, and reads the working set with Task Manager and VMMap either
validates the whole architecture or sends us back to Avalonia. Do not write the rule engine
first and find out later.

`close()` is the wrong call here: it emits `CloseRequested` and is interceptable, which is
useful when you want to *prevent* closing. We want the opposite.

Corollary: **no application state may live in the frontend.** The window can be destroyed at
any moment. React holds a projection of core state, never the state itself.

### Preventing exit-on-last-window

Tauri exits when the last window closes. In the run-event handler:

```rust
tauri::RunEvent::ExitRequested { api, .. } => api.prevent_exit()
```

Exit happens only via the tray menu or an explicit command.

### `EmptyWorkingSet` — with a caveat

Calling `EmptyWorkingSet` / `SetProcessWorkingSetSizeEx(-1, -1)` after teardown makes Task
Manager show a small number immediately. Be honest about what it does: it trims the working
set, it does not free committed memory, and pages fault back in on next touch. It is cosmetic
for a process that is genuinely idle. Do it, because users judge by Task Manager — but do not
count it as an optimisation, and never call it on a hot path.

---

## 4. Module layout

```
src-tauri/src/
├── main.rs                  bootstrap, single-instance guard, tray, run loop
├── ipc/                     Tauri commands + event emitters (thin — no logic)
│
├── core/
│   ├── engine.rs            the tick: sample → evaluate → reconcile → dispatch
│   ├── rule.rs              Rule, Trigger, Condition, Action types
│   ├── profile.rs           profile set, active profile, switching
│   ├── evaluator.rs         condition evaluation, trigger arming/firing
│   ├── executor.rs          input action sequence runner (T3)
│   └── state.rs             single source of truth, RwLock, change events
│
├── power/                   ← the default engine (FEATURES Part A)
│   ├── mod.rs               desired-vs-actual reconciliation, one owner
│   ├── request.rs           PowerCreateRequest handle, reason strings
│   ├── modes.rs             KeepRunning / KeepPresenting → request sets
│   └── inspect.rs           enumerate OTHER processes' power requests (E1)
│
├── input/                   ← the opt-in engine (FEATURES Part C)
│   ├── jiggle.rs            virtual jiggle — the default action
│   ├── motion.rs            visible movement, paths
│   ├── keys.rs              keystrokes, chords
│   ├── verify.rs            did the idle timer actually move? (C7)
│   └── screen.rs            virtual-desktop geometry, per-monitor identity
│
├── platform/                ← the portability boundary (see CROSS-PLATFORM.md)
│   ├── mod.rs               traits: InputInjector, IdleMonitor, PowerGuard,
│   │                        PowerInspector, ProcessMonitor, ForegroundMonitor,
│   │                        SessionMonitor, AutoStart, SystemLoad
│   ├── windows/             the only implementation in v1
│   │   ├── input.rs         SendInput
│   │   ├── idle.rs          GetLastInputInfo + synthetic-input filtering
│   │   ├── power.rs         PowerCreateRequest/SetRequest, GetSystemPowerStatus
│   │   ├── process.rs       CreateToolhelp32Snapshot
│   │   ├── foreground.rs    GetForegroundWindow, SHQueryUserNotificationState
│   │   ├── session.rs       WTSRegisterSessionNotification (lock/unlock)
│   │   └── autostart.rs     Run key / Task Scheduler
│   ├── macos/               stub in v1
│   └── linux/               stub in v1
│
├── config/
│   ├── model.rs             serde types, schema_version
│   ├── store.rs             atomic save (temp + rename), debounced
│   └── migrate.rs           versioned migrations
│
├── timing/
│   └── ticker.rs            CreateWaitableTimerExW + SetWaitableTimerEx
│
└── logging/
    └── mod.rs               tracing subscriber, rolling file, ring buffer for UI
```

**`ipc/` contains no logic.** Every command is a thin wrapper over a `core` call. This keeps
the engine testable without a Tauri runtime and makes a future headless/CLI mode free.

---

## 5. The rule engine

The whole product is one data model. Every feature in FEATURES.md is a composition of these
three enums — there is no special-case code path for "the Teams feature" or "the gaming
feature".

```rust
pub struct Rule {
    pub id: Uuid,
    pub name: String,
    pub enabled: bool,
    pub triggers:   Vec<Trigger>,    // any fires  (OR)
    pub conditions: Vec<Condition>,  // all must hold (AND)
    pub actions:    Vec<Action>,     // sequential
    pub cooldown:   Option<Duration>,
}
```

### Triggers — *when to consider acting*

| Trigger | Fields |
|---|---|
| `Interval` | `base: Duration`, `jitter: Option<Range<Duration>>` |
| `Idle` | `threshold: Duration`, `repeat_every: Option<Duration>` |
| `ProcessStarted` / `ProcessStopped` | `name: String` |
| `ForegroundChanged` | `to: Option<String>` |
| `SessionUnlocked` / `SessionLocked` | — |
| `ResumedFromSleep` | — |
| `Hotkey` | `combo: HotkeyCombo` |
| `AppStartup` | — |
| `Manual` | — |

### Conditions — *whether acting is currently allowed*

Guards, not triggers. Evaluated at fire time, cheap, side-effect free.

| Condition | Notes |
|---|---|
| `ProcessRunning(name)` / `ProcessNotRunning(name)` | cached snapshot, refreshed every 5 s |
| `ForegroundAppIn(list)` / `ForegroundAppNotIn(list)` | the "ignore Photoshop" feature |
| `TimeWindow { days, from, to }` | schedule |
| `UserNotificationState(allowed)` | fullscreen / presentation / **locked or screensaver** — see below |
| `CpuBelow(percent)` | |
| `BatteryAbove(percent)` / `OnACPower` | |
| `SessionUnlocked` | |
| `UserIdleFor(min)` | idle as a guard rather than a trigger |
| `Not(Box<Condition>)` / `AnyOf(Vec<Condition>)` | composition |

`UserNotificationState` deserves a note: one `SHQueryUserNotificationState` call returns
`QUNS_NOT_PRESENT` (locked or screensaver), `QUNS_BUSY` (fullscreen app),
`QUNS_RUNNING_D3D_FULL_SCREEN` (game), `QUNS_PRESENTATION_MODE`, `QUNS_QUIET_TIME`, `QUNS_APP`,
or `QUNS_ACCEPTS_NOTIFICATIONS` (normal).

That single call covers "pause while gaming", "pause during presentations", "pause during
fullscreen video", *and* "is the screen locked" — four separate features on the Move Mouse list
— correctly and for the cost of one Shell call. Do not reimplement any of them by comparing
window rectangles to monitor rectangles; that heuristic misfires on borderless windows and on
multi-monitor setups. Full value table and the quiet-time precedence rule in
[WINDOWS-API](WINDOWS-API.md).

### Actions — *what to do*

**Split by engine**, because the two halves behave differently.

**Held state** — declared, not dispatched. A rule whose conditions hold contributes its
`WakeMode` to the desired state; the engine reconciles the union of all contributions against
what is currently held, once per tick.

| | Fields |
|---|---|
| `WakeMode` | `Off` / `KeepRunning` / `KeepPresenting` |

That is the entire power surface. It is small on purpose: everything interesting about it lives
in the *conditions*, which is the whole thesis of the product.

**Dispatched events** — fired once, in order, by the executor thread.

| Action | Fields |
|---|---|
| `VirtualJiggle` | — · **the default when input is enabled** |
| `MoveRelative` | `dx, dy` |
| `MoveToRandom` | `bounds: MonitorSelector` |
| `MoveAlongPath` | `path: PathKind` |
| `ReturnToOrigin` | — |
| `Click` | `button, count` |
| `Scroll` | `delta` |
| `KeyPress` | `key: VirtualKey` — default `VK_F15` |
| `KeyCombo` | `Vec<VirtualKey>` |
| `Wait` | `Duration` |
| `SwitchProfile` | `profile_id` |
| `Notify` | `text` |
| `RunCommand` | `path, args` — M6, confirmed once per distinct command |

⚠️ Every dispatched action is gated at the engine boundary by the global input-synthesis
switch. There is exactly one place that checks it, and it is not in the UI.

**Default key choice matters.** Move Mouse and most jigglers default to `Shift` or `Scroll Lock`.
`Shift` is not inert — it modifies whatever has focus, and a stray `Shift` into a text field or
a game is a real bug. `Scroll Lock` toggles a real keyboard LED. `VK_F15` (0x7E) is the correct
default: a valid virtual key that resets the idle timer and that essentially no application
binds. Offer the others; do not default to them.

### The tick

```
every 1000 ms (coalesced):

    snapshot = sample_cheap_state()          // idle clocks, tick, foreground hwnd,
                                             // session state, notification state
    if tick % 5 == 0:
        snapshot += sample_expensive()       // process list, CPU, battery

    // ── phase 1: reconcile held state ──────────────────────────
    desired = active_profile.enabled_rules()
        .filter(|r| r.conditions.all_hold(&snapshot))
        .map(|r| r.wake_mode)
        .max()                               // KeepPresenting > KeepRunning > Off
    power.reconcile(desired, &reason_for(desired))

    // ── phase 2: dispatch events ───────────────────────────────
    if !config.input_synthesis_enabled { return }
    if snapshot.human_idle_ms < STAND_DOWN_MS { executor.cancel(); return }   // C6

    for rule in active_profile.enabled_rules():
        if rule.actions.is_empty()             { continue }
        if rule.cooling_down()                 { continue }
        if !rule.triggers.any_fires(&snapshot) { continue }
        if !rule.conditions.all_hold(&snapshot){ continue }
        executor.dispatch(rule.actions.clone())
        rule.start_cooldown()
```

Three things about phase 1 worth stating explicitly:

- **`reconcile` is idempotent.** Called with the same desired mode a thousand times, it acquires
  the request once. This is why power is not modelled as an action.
- **Modes combine by maximum, not by last-writer.** If one rule wants `KeepRunning` and another
  wants `KeepPresenting`, the machine keeps presenting. A rule can never *weaken* what another
  rule is holding.
- **The reason string is recomputed** whenever the contributing rule set changes, so
  `powercfg /requests` always shows why the lock is currently held — see
  [FEATURES A3](FEATURES.md#a3-handle-scoped-self-releasing-auditable).

And phase 2 has a guard phase 1 does not: **the stand-down check runs before any rule is
considered**, so returning to the keyboard cancels everything at once rather than rule by rule.

Cheap sampling is a handful of Win32 calls costing single-digit microseconds. Process
enumeration is the only expensive part (`CreateToolhelp32Snapshot` is ~1–3 ms on a typical
machine) — hence the 5-second cadence and the cached snapshot.

---

## 6. The self-injection feedback loop

**This is the bug that will cost you an evening if it is not designed for.**

`GetLastInputInfo` does not distinguish real input from `SendInput` input. So:

```
idle 3 min  →  rule fires  →  SendInput  →  idle resets to 0
            →  3 min later  →  fires again  →  ...
```

That is the intended behaviour for a jiggler. But it breaks *every* rule that means
"do this only while the user is genuinely away", and it breaks "pause immediately when the
user comes back" — the app can never tell that the user returned, because it keeps
manufacturing input that looks identical.

The fix, since `LLMHF_INJECTED` is only observable from a low-level hook (which is out of
scope, see README §Scope limits):

```rust
struct IdleTracker {
    real_last_input: u64,          // tick count of last input believed to be human
    injected_at: Option<u64>,      // tick count of our most recent SendInput
}

// after every injection:
self.injected_at = Some(GetTickCount64());

// on every sample:
let sys_last = GetLastInputInfo();
match self.injected_at {
    // system idle timer moved within 250 ms of our injection → it was us
    Some(t) if sys_last.abs_diff(t) < 250 => { /* leave real_last_input alone */ }
    _ => self.real_last_input = sys_last,
}
```

The engine then exposes **two** clocks:

- `system_idle_ms` — what Windows/Teams/the screensaver sees. Used for keep-awake logic.
- `human_idle_ms` — what the *user* actually did. Used for every rule and for auto-pause.

Every condition and trigger in the model above uses `human_idle_ms`. Getting this wrong
produces a tool that cannot be interrupted, which is the number one complaint about every
jiggler on the market.

### The same mechanism detects that injection is failing

Once both clocks exist, a third thing falls out for free. After injecting, `system_idle_ms`
**must** have reset. If it did not, the injection went nowhere — which is exactly what happens
under UIPI, where `SendInput` reports success and Windows silently discards the input (see
[WINDOWS-API gotcha 3](WINDOWS-API.md#gotcha-3--sendinput-fails-undetectably-under-uipi)).

```rust
// immediately after an action that should have reset the idle timer
if system_idle_ms_after > INJECTION_TOLERANCE_MS {
    state.set_injection_blocked(true);   // → tray icon, tooltip, status page
}
```

Move Mouse arrived at this same technique in v4.16.3 after years of support traffic, and logs a
warning. **We surface it as a first-class UI state**, because a warning in a log the user must
first enable is not an answer to "it's running but nothing happens."

This also diagnoses the inverse problem — the well-documented NVIDIA GeForce Experience bug
where the system idle timer resets to zero constantly, making everything idle-dependent
(screensavers included) believe a user is present when nobody is. With both clocks on screen
that is visible in one glance instead of being a two-week support thread.

---

## 7. Timing — how idle CPU stays at ~0.00%

Not `thread::sleep` in a loop, and not an async runtime.

```rust
let timer = CreateWaitableTimerExW(
    None, None,
    CREATE_WAITABLE_TIMER_MANUAL_RESET,
    TIMER_ALL_ACCESS.0,
)?;

SetWaitableTimerEx(
    timer,
    &due_time,
    1000,            // period ms
    None, None, None,
    200,             // tolerable delay ms  ← the important argument
);

WaitForSingleObject(timer, INFINITE);
```

The `tolerable_delay` parameter lets Windows **coalesce** this timer with other system timers
instead of forcing a dedicated wake-up. The thread parks in the kernel; the scheduler never
runs it speculatively. With a 200 ms tolerance the practical effect is that the process
contributes essentially nothing to the platform timer-resolution budget.

Two more things in the same spirit:

- **Never call `timeBeginPeriod`.** It raises the *global* system timer resolution, increases
  power draw machine-wide, and is exactly the kind of thing a "lightweight" utility must not
  do. If sub-15 ms precision is needed inside a motion sequence, use a high-resolution
  waitable timer scoped to that sequence only.
- **Opt into EcoQoS at rest.** `SetProcessInformation(ProcessPowerThrottling, ...)` with
  `PROCESS_POWER_THROTTLING_EXECUTION_SPEED` tells the scheduler to park this process on
  efficiency cores. Clear it while an action sequence is running, re-enable after.

---

## 8. State, config, and IPC

### State

One `RwLock<AppState>` in `core::state`. Writers are the scheduler and IPC commands; readers
are everything. On mutation, emit a `state:changed` Tauri event — but **only if a window
exists**. Emitting into the void when no webview is alive is a pure waste, and at one tick
per second it adds up.

### Config

Single `config.json` next to the executable in portable mode, or in `%APPDATA%\project-mouse`
otherwise.

- `schema_version` field from v1, with a `migrate.rs` chain. Retrofitting migrations onto a
  config format that already shipped is miserable.
- Atomic save: write `config.json.tmp`, `fsync`, `MoveFileEx` with `MOVEFILE_REPLACE_EXISTING`.
  A half-written config on a power cut means a user loses every profile they built.
- Debounce saves by 500 ms. The rule builder UI will fire changes on every keystroke.
- **No SQLite.** The entire dataset is a few dozen KB of rules. SQLite costs ~1 MB of binary
  and a resident connection for zero benefit at this scale.

### IPC surface

```
Commands (UI → core)
  get_state() -> AppState
  set_profile(id)
  upsert_rule(rule) / delete_rule(id)
  set_rule_enabled(id, bool)
  trigger_rule_now(id)          // manual trigger
  pause_all(Option<Duration>) / resume_all()
  get_logs(limit) -> Vec<LogEntry>
  get_diagnostics() -> Diagnostics   // idle clocks, RSS, tick timing — for the About page

Events (core → UI)
  state:changed
  rule:fired { rule_id, at }
  log:appended { entry }
```

`get_diagnostics` is worth building early. If the memory budget is a promise, the app should
show its own working set, and the CI benchmark should read the same numbers.

It also carries the two idle clocks and the current power state, which makes it the single
endpoint behind [FEATURES E2](FEATURES.md#e2-live-idle-clocks) and
[E3](FEATURES.md#e3-effect-readout).

`power::inspect` deserves a note: it enumerates the power requests held by **other** processes,
which is what backs [FEATURES E1](FEATURES.md#e1-why-is-my-pc-awake) — the
"why is my PC awake?" panel. It is read-only, it touches nothing, and it is the one feature in
this application that is useful to someone who never turns the main function on.

---

## 9. Logging

`tracing` with two sinks:

- **Rolling file**, `info` by default, 1 MB × 3 files. Never log injected coordinates at
  `info` — a log of everywhere the cursor went is a privacy problem in a file users will
  attach to bug reports.
- **In-memory ring buffer**, 500 entries, for the live log view in the UI. The UI reads the
  ring buffer; it does not tail the file.

---

## 10. Build profile

```toml
[profile.release]
opt-level     = "z"
lto           = "fat"
codegen-units = 1
panic         = "abort"
strip         = true
```

`panic = "abort"` is a deliberate trade: it removes unwinding tables (meaningful size win)
and means a panic in the scheduler kills the process rather than leaving a zombie tray icon
with a dead engine behind it. For a background utility, failing loudly beats failing silently.

Frontend: Vite with manual chunking off, no UI framework beyond React + CSS. Every KB of JS
is a KB parsed on window open, and window-open latency is the one place the user perceives
this app at all.

---

## 11. Testing the promise

The budgets in the README are only real if they are enforced:

- **Unit** — the rule engine with an injected fake `platform` implementation. Trigger firing,
  condition evaluation, cooldowns, and the self-injection filter are all pure logic and should
  be tested without touching Win32.
- **Integration** — a headless build that runs the engine with a recording injector, so a rule
  set can be asserted end to end.
- **Benchmark, in CI** — launch the release binary, idle 10 minutes, sample working set and
  CPU time, fail the build on regression against the README table. A performance promise with
  no gate on it becomes false within three months.
