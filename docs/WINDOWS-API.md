# Windows API mapping

Everything here is via the [`windows`](https://crates.io/crates/windows) crate (windows-rs).
No `winapi`, no hand-written `extern "system"` blocks, no P/Invoke-style marshalling layer —
windows-rs generates direct calls from the official metadata.

The second half of this document is the more valuable half: **the gotchas**. Each one is a
bug that will otherwise be found the hard way.

---

## 1. Cargo setup

```toml
[dependencies.windows]
version = "0.62"          # pin the current 0.x — module paths occasionally
                          # move between minor versions; verify on crates.io
features = [
  "Win32_Foundation",
  "Win32_UI_Input_KeyboardAndMouse",
  "Win32_UI_WindowsAndMessaging",
  "Win32_UI_Shell",
  "Win32_UI_HiDpi",
  "Win32_Graphics_Gdi",
  "Win32_System_Power",
  "Win32_System_Threading",
  "Win32_System_SystemInformation",
  "Win32_System_ProcessStatus",
  "Win32_System_Diagnostics_ToolHelp",
  "Win32_System_RemoteDesktop",
  "Win32_System_Registry",
]
```

Feature flags are additive and gate compile time — do not enable `Win32` wholesale.

---

## 2. Feature → API map

### Mouse movement

`windows::Win32::UI::Input::KeyboardAndMouse`

| Need | Call |
|---|---|
| Read cursor | `GetCursorPos(&mut POINT)` — `Win32::UI::WindowsAndMessaging` |
| Relative move | `SendInput` with `MOUSEEVENTF_MOVE` |
| Absolute move | `SendInput` with `MOUSEEVENTF_MOVE \| MOUSEEVENTF_ABSOLUTE \| MOUSEEVENTF_VIRTUALDESK` |
| Precise path steps | add `MOUSEEVENTF_MOVE_NOCOALESCE` |
| Clicks | `MOUSEEVENTF_LEFTDOWN` / `LEFTUP` / `RIGHTDOWN` / `RIGHTUP` / `MIDDLEDOWN` / `MIDDLEUP` |
| Scroll | `MOUSEEVENTF_WHEEL` with `mouseData = ±WHEEL_DELTA` (120) |

```rust
let mut input = INPUT {
    r#type: INPUT_MOUSE,
    Anonymous: INPUT_0 {
        mi: MOUSEINPUT {
            dx, dy,
            mouseData: 0,
            dwFlags: MOUSEEVENTF_MOVE,
            time: 0,
            dwExtraInfo: MAGIC_EXTRA_INFO,   // see gotcha 4
        },
    },
};
let sent = unsafe { SendInput(&[input], std::mem::size_of::<INPUT>() as i32) };
if sent == 0 { /* blocked — see gotcha 3 */ }
```

### Keyboard

`SendInput` with `INPUT_KEYBOARD` / `KEYBDINPUT`. Always send the matching `KEYEVENTF_KEYUP`
— a modifier left down is a stuck modifier for the whole session.

Default key: `VK_F15` (`0x7E`). Rationale in [FEATURES.md §A2](FEATURES.md).

### Idle detection

```rust
use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
use windows::Win32::System::SystemInformation::GetTickCount;   // 32-bit, deliberately

let mut lii = LASTINPUTINFO {
    cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
    dwTime: 0,
};
unsafe { GetLastInputInfo(&mut lii) };
let idle_ms = GetTickCount().wrapping_sub(lii.dwTime);   // see gotcha 1
```

### Keep awake — the most important API in the project

`windows::Win32::System::Power`

**Do not use `SetThreadExecutionState`.** It is what every tool in this category reaches for and
it is wrong here for two independent reasons — see gotcha 2 (undocumented thread affinity) and
**gotcha 0** (it does not hold a Modern Standby machine awake at all).

```rust
use windows::Win32::System::Power::*;

// acquire — the reason string is user-visible in `powercfg /requests`
let mut ctx = REASON_CONTEXT {
    Version: POWER_REQUEST_CONTEXT_VERSION,
    Flags:   POWER_REQUEST_CONTEXT_SIMPLE_STRING,
    Reason:  REASON_CONTEXT_0 { SimpleReasonString: PCWSTR(reason.as_ptr()) },
};
let req: HANDLE = PowerCreateRequest(&ctx)?;

PowerSetRequest(req, PowerRequestSystemRequired)?;      // don't sleep
PowerSetRequest(req, PowerRequestExecutionRequired)?;   // ← the Modern Standby one
PowerSetRequest(req, PowerRequestDisplayRequired)?;     // KeepPresenting only

// release
PowerClearRequest(req, PowerRequestDisplayRequired)?;
PowerClearRequest(req, PowerRequestExecutionRequired)?;
PowerClearRequest(req, PowerRequestSystemRequired)?;
CloseHandle(req)?;
```

| Request type | Effect |
|---|---|
| `PowerRequestSystemRequired` | The system does not enter sleep on idle |
| `PowerRequestDisplayRequired` | The display stays on; screensaver and idle lock suppressed |
| **`PowerRequestExecutionRequired`** | **The process keeps running on a Modern Standby (S0) system.** Windows 8+. Without it, S0 machines suspend the process even though the "system required" request is held |
| `PowerRequestAwayModeRequired` | Away Mode — media scenarios. Not used here |

Mode mapping is in [FEATURES A1](FEATURES.md#a1-the-two-modes).

### Read *other* processes' power requests

`PowerGetActiveScheme` is not what you want here. The user-facing answer to "why is my PC
awake?" is what `powercfg /requests` prints, and it comes from
`CallNtPowerInformation(PowerRequestInfo, ...)` — an undocumented-but-stable information class
that returns a `POWER_REQUEST_LIST` of requesters and reasons.

Two honest caveats before building on it:

- The structure is not in the Windows SDK headers and has to be declared by hand. It has been
  stable across Windows versions, but treat it as an implementation detail and degrade
  gracefully if the shape changes.
- **Full detail requires elevation**, which this app deliberately never has. Unelevated, expect
  a partial list. Design [FEATURES E1](FEATURES.md#e1-why-is-my-pc-awake) to show
  what it can see and say plainly that a complete list needs an elevated
  `powercfg /requests` — with a copy button for the command. That is a better product than
  silently showing an incomplete list, and it avoids asking for admin rights we promised never
  to need.

### Process enumeration

`windows::Win32::System::Diagnostics::ToolHelp`

`CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)` → `Process32FirstW` / `Process32NextW`
over `PROCESSENTRY32W`. Close the handle. ~1–3 ms, so cache it (5 s cadence).

### Foreground application

```
GetForegroundWindow()                        // Win32::UI::WindowsAndMessaging
  → GetWindowThreadProcessId(hwnd, &mut pid)
  → OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid)
  → QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, ...)
```

`PROCESS_QUERY_LIMITED_INFORMATION` (not `PROCESS_QUERY_INFORMATION`) — the limited right
works against elevated and protected processes where the full right is denied.

### Fullscreen / game / presentation detection

`windows::Win32::UI::Shell::SHQueryUserNotificationState`

| Value | State | Meaning |
|---|---|---|
| 1 | `QUNS_NOT_PRESENT` | **Screensaver showing, machine locked, or an inactive Fast User Switching session.** |
| 2 | `QUNS_BUSY` | Fullscreen app (video, etc.) |
| 3 | `QUNS_RUNNING_D3D_FULL_SCREEN` | Fullscreen exclusive Direct3D — a game |
| 4 | `QUNS_PRESENTATION_MODE` | Presentation mode |
| 5 | `QUNS_ACCEPTS_NOTIFICATIONS` | Normal |
| 6 | `QUNS_QUIET_TIME` | Quiet hours (Win7+) |
| 7 | `QUNS_APP` | A Store app is running full screen (Win8+) |

One call, four features. Do **not** reimplement this by comparing window rects to monitor
rects — that heuristic misfires on borderless windows and on multi-monitor layouts.

⚠️ **`QUNS_NOT_PRESENT` (1) is the one everybody forgets**, and it is the most useful value
here: it is the cheapest reliable signal for "screen is locked or the screensaver is up". Treat
it as a first-class state, not a leftover.

⚠️ Precedence: during quiet time, if another blocking mode also applies, only that other value
is returned — `QUNS_QUIET_TIME` does not mask it. Do not write the check as an if/else chain
that assumes quiet time wins.

### Monitors and virtual desktop

`Win32::UI::WindowsAndMessaging::GetSystemMetrics`:
`SM_XVIRTUALSCREEN`, `SM_YVIRTUALSCREEN`, `SM_CXVIRTUALSCREEN`, `SM_CYVIRTUALSCREEN`,
`SM_CMONITORS`, `SM_REMOTESESSION`.

`Win32::Graphics::Gdi`: `EnumDisplayMonitors`, `GetMonitorInfoW`, `MonitorFromPoint`.
Re-enumerate on `WM_DISPLAYCHANGE` (`0x007E`).

### Session lock / unlock

`Win32::System::RemoteDesktop::WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION)`
→ `WM_WTSSESSION_CHANGE` (`0x02B1`) with `wParam` of `WTS_SESSION_LOCK` (`0x7`) /
`WTS_SESSION_UNLOCK` (`0x8`). Unregister on shutdown.

### Sleep / resume

`WM_POWERBROADCAST` (`0x0218`) with `PBT_APMSUSPEND` (`0x4`) and `PBT_APMRESUMEAUTOMATIC`
(`0x12`). Re-arm all interval triggers on resume — the machine may have been asleep for hours.

### Battery and CPU

- `Win32::System::Power::GetSystemPowerStatus(&mut SYSTEM_POWER_STATUS)` — AC line status,
  battery percent
- `Win32::System::Threading::GetSystemTimes(idle, kernel, user)` — sample twice, compute the
  delta ratio. No PDH, no WMI: both are heavyweight for one number.

### Timer

`Win32::System::Threading`

```rust
let timer = CreateWaitableTimerExW(
    None, None,
    CREATE_WAITABLE_TIMER_MANUAL_RESET,
    TIMER_ALL_ACCESS.0,
)?;

SetWaitableTimerEx(timer, &due, 1000, None, None, None, 200);
//                                             tolerable delay ─┘
WaitForSingleObject(timer, INFINITE);
```

The tolerable delay is what allows Windows to coalesce this wake-up with others.
See [ARCHITECTURE.md §7](ARCHITECTURE.md#7-timing--how-idle-cpu-stays-at-000).

### Global hotkeys

`RegisterHotKey` / `UnregisterHotKey` → `WM_HOTKEY` on the message thread. Or
`tauri-plugin-global-shortcut`, which wraps the same thing.

### DPI awareness

`Win32::UI::HiDpi::SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2)`,
or declare it in the manifest (preferred — it applies before any window exists).

### Power throttling (EcoQoS)

```rust
let state = PROCESS_POWER_THROTTLING_STATE {
    Version:     PROCESS_POWER_THROTTLING_CURRENT_VERSION,
    ControlMask: PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
    StateMask:   PROCESS_POWER_THROTTLING_EXECUTION_SPEED,  // 0 to clear
};
SetProcessInformation(GetCurrentProcess(), ProcessPowerThrottling, &state as *const _ as _, size);
```

Enable at rest, clear while an action sequence runs.

### Memory reporting

`Win32::System::ProcessStatus::{GetProcessMemoryInfo, EmptyWorkingSet}` — the first for the
Diagnostics panel and the CI benchmark, the second for post-webview-teardown trimming
(cosmetic; see ARCHITECTURE §3).

### Single instance

`CreateMutexW` with a named mutex; `GetLastError() == ERROR_ALREADY_EXISTS` → signal the
running instance and exit. `tauri-plugin-single-instance` does this and forwards argv.

### Autostart

`HKCU\Software\Microsoft\Windows\CurrentVersion\Run` via `Win32::System::Registry`, or
`tauri-plugin-autostart`. Task Scheduler only for the "before login" case.

---

## 3. Gotchas

These are ordered by how much time each will cost if missed.

### Gotcha 0 — SetThreadExecutionState does not prevent Modern Standby

The highest-value gotcha in this document, and the one that explains why every competing tool
has an open bug.

`SetThreadExecutionState` **only resets idle timers**. On a Modern Standby (S0) system with the
display off, that is not enough — the machine enters connected standby anyway, and a
long-running job dies. The user-visible symptom is "the tool says it's running, and my laptop
still went to sleep."

Open right now in every major competitor:
[PowerToys #48965](https://github.com/microsoft/powertoys/issues/48965) ·
Mouse Jiggler #130 ("jiggle timer dies on Modern Standby display-off and never recovers") ·
[Move Mouse #109](https://github.com/sw3103/movemouse/issues/109).

The fix is `PowerSetRequest` with **`PowerRequestExecutionRequired`**, which has no
`SetThreadExecutionState` equivalent at all. There is no flag you can add to
`ES_CONTINUOUS | ES_SYSTEM_REQUIRED` that does this.

S3 sleep is disappearing from new laptops. This is not an edge case; it is the majority
platform.

**Test it properly.** A unit test cannot catch this. The acceptance test is physical: an S0
laptop, lid closed, display off, **Keep running** active, still reachable eight hours later.
That test goes in the release checklist, not in CI.

### Gotcha 1 — `LASTINPUTINFO.dwTime` is 32-bit and wraps

`dwTime` is a `DWORD` derived from `GetTickCount()`, which wraps every **49.7 days**.
`GetTickCount64()` does not wrap. Subtracting a wrapped 32-bit value from a 64-bit one gives
an idle time of roughly 49 days, and every idle rule fires immediately and permanently.

Compare in the same width, with wrapping arithmetic:

```rust
let idle_ms = GetTickCount().wrapping_sub(lii.dwTime);   // both u32 — correct
```

(`GetTickCount64()` truncated to `u32` is bit-identical to `GetTickCount()`, so that works
too. The error is comparing the *untruncated* 64-bit value.)

Uptimes past 49 days are not exotic on the exact machines this tool targets: servers,
wallboards, and workstations that are never allowed to sleep — because this tool is running.

**And clamp the result.** Microsoft explicitly documents that `dwTime` is *not guaranteed to be
incremental* — it can be lower than a prior event's tick, because of a timing gap between the
raw input thread and the desktop thread, or because `SendInput` supplies its own tick count.
So the subtraction can legitimately produce a huge wrapped value even without a 49-day wrap:

```rust
let raw = GetTickCount().wrapping_sub(lii.dwTime);
let idle_ms = if raw > SANITY_MAX { 0 } else { raw };   // treat nonsense as "just now"
```

Note the second cause in that list: **`SendInput` supplying its own tick count is our own code**.
This gotcha and the self-injection filter are the same bug wearing two hats.

### Gotcha 2 — SetThreadExecutionState is thread-affine

The execution state belongs to the **calling thread** and is discarded when that thread exits.

```rust
// WRONG — the state dies with the thread, seconds later
std::thread::spawn(|| unsafe {
    SetThreadExecutionState(ES_CONTINUOUS | ES_DISPLAY_REQUIRED);
});
```

Call it from the long-lived scheduler thread and keep that thread alive for the process
lifetime. Symptom when wrong: keep-awake "works" for a few seconds after toggling, then
silently stops, and only reproduces after a long idle period.

**Caveat worth knowing: this thread affinity is not actually documented.** The Remarks section
of `SetThreadExecutionState` says nothing about per-thread scope or cleanup on thread exit. The
behaviour is universally relied upon and consistent with the function's name, but it is
folklore, not contract.

Since the whole keep-awake feature depends on it, prefer the API that *does* document its
lifetime:

```rust
// Win32::System::Power — handle-scoped, documented, and it shows up in `powercfg /requests`
let req = PowerCreateRequest(&REASON_CONTEXT { ... })?;
PowerSetRequest(req, PowerRequestDisplayRequired)?;
PowerSetRequest(req, PowerRequestSystemRequired)?;
// release:
PowerClearRequest(req, PowerRequestDisplayRequired)?;
CloseHandle(req)?;
```

The request is tied to a **handle**, not a thread, so it survives any threading refactor. It
also appears in `powercfg /requests` with the reason string you supply — which means a user (or
an IT admin) can see exactly why the machine will not sleep, and attribute it to this app by
name. For a tool in this category, being legible to `powercfg` is a trust feature, not a
detail.

### Gotcha 3 — SendInput fails undetectably under UIPI

This is worse than "fails silently", and it is worth reading the documentation sentence
verbatim:

> "This function fails when it is blocked by UIPI. Note that neither `GetLastError` nor the
> return value will indicate the failure was caused by UIPI blocking."
> — [SendInput, Remarks](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput)

So when User Interface Privilege Isolation blocks injection — a process at higher integrity
owns the foreground window — `SendInput` typically reports the **full event count, as if it
succeeded**, while the input goes nowhere. There is no error code to check.

The documented "returns 0" case is a *different* failure: input already blocked by another
thread via `BlockInput`. Checking the return value is still correct and still necessary — it
just does not catch the case you most want to catch.

Situations where injection silently evaporates:

| Situation | Result |
|---|---|
| UAC consent prompt (secure desktop) | Goes nowhere; call reports success |
| Lock screen / login screen | Goes nowhere; call reports success |
| Elevated Task Manager or editor focused | Goes nowhere while it has focus |
| Some anti-cheat and DRM overlays | Blocked or ignored |

**The only reliable detection is verification, not error handling:** after a movement action,
read `GetCursorPos` and confirm the cursor actually moved. If it did not, mark the injector as
`Blocked` and surface that state in the tray tooltip and diagnostics rather than logging a
success. This is the only way the app can honestly tell the user "I am running but I cannot
currently do anything".

Do not respond to this by requesting elevation. Running a background input-injection utility as
administrator is a far worse trade than occasionally being unable to inject — and it makes
the AV-reputation problem in the README significantly harder.

### Gotcha 4 — tag your own input with `dwExtraInfo`

Both `MOUSEINPUT` and `KEYBDINPUT` carry a `dwExtraInfo: usize` that travels with the event.
Set it to a constant magic value.

This does not help with `GetLastInputInfo` (which ignores it), but it makes the app's own
events identifiable to anything that reads the input stream — including the app's own future
code, and any diagnostics. It costs nothing and it is impossible to retrofit into logs that
were never written.

The actual self-injection filter is the timestamp-window approach in
[ARCHITECTURE.md §6](ARCHITECTURE.md#6-the-self-injection-feedback-loop), because
`LLMHF_INJECTED` is only visible from a low-level hook, and hooks are out of scope.

### Gotcha 5 — absolute coordinates are normalised, and default to the primary monitor

`MOUSEEVENTF_ABSOLUTE` coordinates are **0..65535 normalised**, not pixels. Without
`MOUSEEVENTF_VIRTUALDESK` they normalise against the primary monitor only, so on a
multi-monitor setup the cursor can never leave display 1.

```rust
let vx = GetSystemMetrics(SM_XVIRTUALSCREEN);
let vy = GetSystemMetrics(SM_YVIRTUALSCREEN);
let vw = GetSystemMetrics(SM_CXVIRTUALSCREEN);
let vh = GetSystemMetrics(SM_CYVIRTUALSCREEN);

let nx = ((x - vx) as i64 * 65535 / (vw - 1) as i64) as i32;
let ny = ((y - vy) as i64 * 65535 / (vh - 1) as i64) as i32;

dwFlags: MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK
```

Note `vx` / `vy` can be **negative** — a monitor placed left of or above the primary. Code
that assumes the virtual desktop starts at (0, 0) breaks on a very common layout.

### Gotcha 6 — relative movement passes through pointer acceleration

`MOUSEEVENTF_MOVE` with `dx: 10` does not move exactly 10 pixels. It is fed through the
system's pointer speed and Enhanced Pointer Precision curve, so the actual displacement
depends on user settings and on the velocity implied by the step timing.

For motion that must be exact — a Bézier path, or "return to origin" — read `GetCursorPos`,
compute the target, and send an **absolute** move. Relative moves are fine for a jiggle where
only "something happened" matters.

### Gotcha 7 — `MOUSEEVENTF_MOVE` with `dx: 0, dy: 0` may be discarded

A zero-delta move can be coalesced away and may not register as input at all. Never rely on
it as a no-op nudge. Move by at least 1 pixel, or move 1 and back.

### Gotcha 8 — `SendInput` batches must be sent in one call

Events for one logical gesture (key down + key up, button down + button up) must go in a
single `SendInput` array. Separate calls can be interleaved with real user input by the
system, producing a click that spans a real user action.

### Gotcha 9 — `timeBeginPeriod`: still don't, but not for the reason you think

The folk wisdom is "it raises the global system timer resolution and burns battery
machine-wide". **That stopped being true in Windows 10, version 2004.**

> "Prior to Windows 10, version 2004, this function affects a global Windows setting… Starting
> with Windows 10, version 2004, this function no longer affects global timer resolution. For
> processes which call this function, Windows uses the lowest value requested by any process."
> — [timeBeginPeriod](https://learn.microsoft.com/en-us/windows/win32/api/timeapi/nf-timeapi-timebeginperiod)

The resolution is now **per-process**. So the old objection does not apply on any supported
Windows version.

The reason to still avoid it is different, and it is specific to this app:

> "Starting with Windows 11, if a window-owning process becomes fully occluded, minimized, or
> otherwise invisible or inaudible to the end user, Windows does not guarantee a higher
> resolution than the default system resolution."

A tray utility with no window is **permanently** in that state. On Windows 11, calling
`timeBeginPeriod` from this process buys nothing at all unless it explicitly opts out via
`SetProcessInformation` with `ProcessPowerThrottling` and
`PROCESS_POWER_THROTTLING_IGNORE_TIMER_RESOLUTION`.

**Which collides directly with the EcoQoS recommendation above.** Both are controlled through
the same `PROCESS_POWER_THROTTLING_STATE`, and they pull in opposite directions:

| Phase | `EXECUTION_SPEED` | `IGNORE_TIMER_RESOLUTION` |
|---|---|---|
| At rest (waiting for a trigger) | throttled — EcoQoS on | leave default |
| Executing a motion sequence | cleared | set, *only if* sub-15 ms steps are genuinely needed |

Simpler and better: **we do not need the precision.** Movement steps are jittered anyway
([FEATURES C5](FEATURES.md)), so a step that lands at 16 ms instead of 12 ms changes nothing
observable. Design the motion engine to tolerate timer jitter and this whole question
disappears.

If precision ever does become necessary, scope a `CREATE_WAITABLE_TIMER_HIGH_RESOLUTION` timer
to that sequence rather than changing process-wide state.

### Gotcha 10 — `GetLastInputInfo` is session-scoped

It reports input for the calling session only, and does not observe the secure desktop. Under
RDP, the semantics differ again, and `GetSystemMetrics(SM_REMOTESESSION)` is how you detect
that you are in one. A disconnected RDP session may have no interactive desktop at all, in
which case injection is meaningless — detect and pause rather than logging failures forever.

### Gotcha 11 — `QueryFullProcessImageNameW`, not `GetModuleFileNameExW`

`GetModuleFileNameExW` fails across 32/64-bit process boundaries and on some protected
processes. `QueryFullProcessImageNameW` with `PROCESS_QUERY_LIMITED_INFORMATION` works in
every case that matters here.

### Gotcha 12 — release keep-awake on exit

If the process exits without clearing `ES_CONTINUOUS` (or without `PowerClearRequest` +
`CloseHandle`), the state normally dies with the process — but a crash, a hung thread, or a
Tauri shutdown path that never runs the cleanup can leave a machine that will not sleep long
after the app is gone. Clear it in the `ExitRequested` handler *and* in a panic hook.

Test for it: `powercfg /requests` from an elevated prompt should list nothing attributable to
this app once it has exited. Using `PowerSetRequest` with a descriptive reason string (gotcha 2)
makes that check trivial instead of guesswork.
