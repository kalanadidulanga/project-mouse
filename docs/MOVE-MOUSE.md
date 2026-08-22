# Move Mouse — verified inventory and importer spec

Everything here was checked against **both** the wiki (all nine pages) and the shipping source
at `AssemblyVersion 4.20.0.0` (.NET Framework 4.7.2, WPF, `asInvoker` manifest).

Two reasons this document exists:

1. **Parity.** So no feature gets missed by accident, and so every deliberate omission is a
   recorded decision rather than an oversight.
2. **The importer.** Reading `Settings.xml` and translating it into our rules is the single
   cheapest thing we can do to remove the reason an existing user would not switch.

**Tags:** `[wiki]` documented · `[code]` verified in source but **not** documented.

> The wiki is significantly behind the product. Send Keystrokes, action Copy, RepeatMode,
> IntervalThrottle, Abortable, random distances, the log level controls and the
> Blackout/Schedule enable toggles are all undocumented — while the wiki still documents a
> `#StandWithUkraine` theme that no longer exists in the code.

---

## 1. Architecture, in one line

**Move Mouse has no power API integration whatsoever.** No `SetThreadExecutionState`, no
`PowerCreateRequest`, no display request, no screensaver suppression, no monitor enumeration.
Every feature hangs off synthetic input resetting `GetLastInputInfo`.

That is the whole gap, and it is the source of its own number one support complaint:

> *"By far the most common complaint I get from users is 'Move Mouse is running, but my
> computer still went to sleep.'"* — Troubleshooting

Its remedy is to have the user download a PowerShell script and watch the idle counter. It
cannot be fixed inside that architecture.

---

## 2. Actions

Nine action types. Each has a **Test** button, can be reordered, copied, and individually
disabled. The last action cannot be removed.

**Common properties, all types:**

| Property | Values | Default |
|---|---|---|
| `Name` | free text | null |
| `Trigger` | `Start` / `Interval` / `Stop` | **Interval** |
| `Repeat` | bool, Interval only | true |
| `IsEnabled` | bool | true |
| `RepeatMode` `[code]` | `Forever` / `Throttle` | Forever |
| `IntervalThrottle` `[code]` | run only for the first N intervals | 1 |
| `Id` | GUID | auto |

`[wiki]` Trigger semantics, verbatim — and note the surprise in both:

- **Start** — "…whether this be from clicking Move Mouse, resuming from being paused, or a
  Scheduled start. **This does not include leaving a Blackout window.**"
- **Stop** — "…including clicking Move Mouse, entering a paused state, or a Scheduled stop.
  **This does not include entering a Blackout window.**"

`[code]` Actions run **synchronously on one thread** in list order. If any action reports
`Aborted`, the rest of that pass is skipped and Move Mouse stops.

### 2.1 Move Mouse Cursor

| Property | Detail |
|---|---|
| `Distance` | px, 1–9999, default **10** |
| `UpperDistance` + `Random` `[code]` | random in `[Distance, Upper)`, upper default **20** |
| `Direction` | `Square` (default, E→S→W→N, returns to origin), `None`, `Random` (random legs, each capped at 150 px), the eight compass points, `UpAndDown`, `DownAndUp`, `LeftAndRight`, `RightAndLeft` |
| **Stealth** | `[wiki]` "Sets the Direction to None so that the mouse cursor does not visibly move on screen, although it will still reset the session idle time" |
| `Speed` | `Slow` / **`Normal`** / `Fast` / `Custom` → per-pixel delay of 10 / 5 / 0 ms, or `Delay` 0–5000 ms |
| `AbortIfUserActivityDetected` `[code]` | aborts mid-movement if the user moves the mouse |

→ **Ours:** [FEATURES C1](FEATURES.md#c1-virtual-jiggle-default-when-input-is-enabled)
and [C2](FEATURES.md#c2-visible-movement). Stealth becomes its own first-class action and the
default, rather than a magic value of a Distance/Direction pair that leaves Distance visible
but ignored.

### 2.2 Click Mouse Button

`Button`: `Left` (default) / `Middle` / `Right` — no X1/X2. `Hold` + `HoldInterval` seconds
(0.1–9999, disabled by default).

`[code]` Uses legacy `mouse_event`, at the **current cursor position** — no coordinates of its
own, so it must be paired with Position Mouse Cursor.

### 2.3 Scroll Mouse Wheel

`Distance` raw wheel units (120 = one notch), default **100**; `UpperDistance` default 200 with
`Random`; `Direction` `Up` / **`Down`** / `Left` / `Right` / `Random`.

### 2.4 Position Mouse Cursor

`X`, `Y` absolute virtual-screen pixels via `SetCursorPos`.

**Track** `[wiki]`: "Use the Track button to automatically track the cursor location as you
move it… Once the mouse cursor has remained in position for **three seconds**, the tracking
will stop and your coordinates will be locked-in."

`[code]` Polls every 100 ms; the three-second timer is measured off `GetLastInputInfo`, so *any*
input — including a keypress — resets it. No DPI awareness, no monitor identity.

→ **Ours:** store as (monitor identity, normalised position) so a config survives a resolution
change or monitors being rearranged.

### 2.5 Activate Application

`Mode`: `Process` (default) / `Window`. `Application` (the UI calls this field **Name**) —
a dropdown of running processes with a non-empty `MainWindowTitle`.

Window mode supports leading and/or trailing `*` wildcards, case-insensitive, first match wins.

`[code]` Resolves a window **title**, then `FindWindow(null, title)` + `AppActivate`. Even in
Process mode it goes via the process's `MainWindowTitle`, so identical titles are ambiguous.
The author's own advice on the Scenarios page: *"I would suggest Process as window titles can
change and cause the Action to fail."*

→ **Not building.** This crosses Test 2 in [PRODUCT.md §5](PRODUCT.md#5-the-line) — targeting
another program's UI. Its main documented purpose (forcing a remote-session client to the
foreground so a click lands inside it) is better served by conditions.

### 2.6 Run Command

`FilePath` (must satisfy `File.Exists` — **no environment-variable expansion, no folders, no
URLs, no shell verbs**), `Arguments`, `WaitForExit`, `Hidden`.

⚠️ `[wiki]` "Long running/unstable processes may cause Move Mouse to hang or freeze." `[code]`
An unbounded `process.WaitForExit()` on the action thread.

→ **Ours:** [FEATURES D12](FEATURES.md#d12-scripting-escape-hatch) — `ShellExecute`
semantics, expansion, async with a timeout.

### 2.7 PowerShell Script

`ScriptPath` (must end `.ps1`), `WaitForExit`, `Hidden`.

`[code]` Launches
`powershell.exe -ExecutionPolicy Bypass -File "<path>"` — **Windows PowerShell 5.1 only**
(never `pwsh`), **always bypassing execution policy**, no parameters, no output or exit code
captured, and the script cannot feed anything back.

The wiki supplies a [PowerShell Snippets](https://github.com/sw3103/movemouse/wiki/PowerShell-Snippets)
cookbook: minimise all windows, send a keystroke combination, lock the session, terminate a
process, start/stop/restart a service, delete/copy/move a file, minimise/maximise/restore a
window, position a window, click-and-drag, and a silent `SendInput` jiggle loop.

→ **Not building this shape.** Spawning a process per interval is heavyweight, normalises
bypassing execution policy, and captures nothing. Sandboxed Rhai for conditions instead.

### 2.8 Sleep

`Seconds` / `UpperSeconds`, 0.1–9999, defaults 1 and 2, with `Random`.

`[code]` Two bugs worth recording: it is a `Thread.Sleep` that blocks the whole action pass
rather than a countdown pause, and the random path does
`Random.Next(Convert.ToInt32(Seconds), Convert.ToInt32(UpperSeconds))` — so **randomised sleeps
are silently truncated to whole seconds** despite the UI accepting 0.1 steps.

### 2.9 Send Keystrokes `[code]` — undocumented

Added in v4.19.0; the wiki's action list was never updated and still tells users to use a
PowerShell `SendKeys` snippet instead.

`Keystrokes` (ordered VK codes), `Method` `Sequential` / `Simultaneous` (chord), `Pause` +
`PauseInterval` (0.001–9999 s, default 0.1), `AbortIfUserActivityDetected`.

`[code]` Uses legacy `keybd_event` — no text-typing mode, no Unicode, no scancode or
extended-key handling. And the abort check watches the **cursor position**, not the keyboard,
which is a tell that the self-injection problem was never solved cleanly.

---

## 3. Scheduling

### Interval

`[wiki]` "By default, Move Mouse is configured to move your mouse cursor **every 30 seconds**."
`LowerInterval` 30 s, `UpperInterval` 60 s, `RandomInterval` false; range 1–99999 s; uniform
draw in `[Lower, Upper)`, re-drawn every cycle.

### Simple Schedule

`Action` **Start**/**Stop**, `Time` (00:00:00–23:59:59), seven day booleans (all true; the UI
refuses to let you clear the last one), `Delay` 0–99999 s of random jitter, `IsEnabled`
(added v4.20.0).

`[code]` Compiled internally to a Quartz cron string — with the jitter baked in at build time
rather than applied at firing.

### Advanced Schedule

A raw **Quartz.NET cron expression** (seven-field dialect), plus `Action` and `IsEnabled`. No
jitter.

`[code]` In-process Quartz `StdSchedulerFactory`. Schedules do **not** survive the app being
closed — Move Mouse must be running. A `Stop` schedule stops *actions*, not the app.

### Blackouts

`[wiki]` "Blackouts allow you to specify windows of time when you would like disable all Move
Mouse activity and enter into the **Sleeping** state." Per blackout: `Time` (start), `Duration`
(default 1 hour), seven day booleans, `IsEnabled`.

Durations may cross midnight — the check also tests yesterday's occurrence, which is how the
wiki's own "18:00 for 14 hours" example works. Polled every second.

`[wiki]` worked example for working-hours-only operation: two blackouts — 18:00 for 14 h every
day, plus 08:00 for 10 h on Saturday and Sunday.

→ **Ours:** blackouts and schedules are two separate systems here, with different data models
(day flags + duration vs cron), different semantics (a Sleeping state vs Start/Stop), and the
surprising rule that blackout edges do not fire Start/Stop actions. **One condition model
replaces both**, with real timezone and DST handling —
[FEATURES B2](FEATURES.md#b2-bind-to-a-schedule) and
[B12](FEATURES.md#b12-composition).

### Startup and hotkeys

Two separate switches: **Launch Move Mouse at start-up** (`HKCU\...\Run`, or a UWP
`StartupTask` in the Store build) and **Start actions when Move Mouse is launched**.

**No hotkeys ship.** `[code]` A "Start/Stop Move Mouse using the keyboard" toggle exists in the
XAML and settings class but is entirely commented out.

---

## 4. Conditions

The whole list, and it is short:

| Condition | Behaviour |
|---|---|
| **Auto-pause on user activity** | `GetLastInputInfo` polled at **250 ms**; pauses when idle < 250 ms |
| **Auto-resume after N seconds** | default 30 s of *inactivity* (the UI label says "activity" — the wiki text contradicts itself; the code implements inactivity) |
| **Continue when session locked** | default **off** — locked state ticks but performs no actions |
| **Pause on battery** | AC/DC only, **no percentage threshold** |
| **Blackouts** | time windows, above |

`[code]` There is **no** process condition, **no** active-window condition, **no** CPU,
fullscreen, presentation, network, or device condition, and **no** screensaver-aware logic.

→ This is the gap [FEATURES Part B](FEATURES.md#part-b--conditions) exists to fill.

---

## 5. Behaviour and appearance settings

**Behaviour:** repeat interval (constant or random), auto-pause, auto-resume + seconds, launch
at startup, start actions at launch, **adjust volume when running** (0–100, sets the default
playback device's volume and restores it on stop), continue when locked, pause on battery,
enable logging (+ a log level dropdown and Open/Follow buttons, none of which the wiki mentions).

**Appearance:** hide main window · show topmost when running · minimise when not running ·
hide from taskbar · hide from Alt+Tab · **override window title** · enable screen burn
prevention · **hide system tray icon** · show tray notifications · show status on main window ·
disable button animations · `[code]` show status on taskbar (**the only setting that defaults
to true**) · `[code]` override icon.

⚠️ `[wiki]` "If you opt to hide **both** the Move Mouse window and system tray icon, **there
will be no way to access the Settings**… The Move Mouse system tray icon will be visible
**momentarily at launch**." `[code]` The escape hatch is a hard-coded ten-second window.

**Status colours:** Idle (blue) · Running (green) · Executing (red) · Paused (yellow) ·
Sleeping (purple) · Paused On Battery (orange) · `[code]` **Locked**, which the wiki's list
omits.

→ **Not building:** override window title, override icon, hide-both. These are boss-key
disguise, they are exactly the traits corporate EDR and DLP tooling flags, and the last one
locks the user out of their own settings. Screen-burn jitter and volume adjustment are
historical artefacts of an app that expected to sit visible on screen —
see [FEATURES: what we will not build](FEATURES.md#what-we-will-not-build).

---

## 6. Documented limitations

Straight from the wiki, and all of them useful:

1. **UIPI / elevated foreground windows** — "If an application which has been opened in the
   Administrator or another user's context is in the foreground, then the Actions that are
   being executed **may not reset the system idle time**." Version 4.16.3 added detection that
   logs a warning. `[code]` It compares `GetLastInputInfo` before and after any action whose
   `InterruptsIdleTime` is true.
   → **Adopt this technique and surface it in the UI**, not only in a log the user must first
   enable — [FEATURES C7](FEATURES.md#c7-detect-and-report-when-injection-is-silently-failing).
2. **NVIDIA GeForce Experience bug** — "causing the idle time to constantly reset to 0… could
   cause anything that is reliant on the system idle time, such as screen savers, Move Mouse,
   etc. to **falsely detect user activity when there is none**." The author notes he still gets
   users contacting him about a years-old thread. Practical effect: auto-pause never lets it run.
   → A visible idle clock diagnoses this in one glance — [FEATURES E2](FEATURES.md#e2-live-idle-clocks).
3. **Mouse movement is often not detected inside remote sessions** — use a click instead.
4. **Window-title matching is fragile** — prefer Process mode.
5. **Topmost breaks the capture-previous-window recipe.**
6. **CLI is GitHub-build only.** The Store build has no CLI.
7. **The Store build is "not normally available on work/corporate machines"**, and the portable
   build is the "better option for work/corporate machines."
8. **Reset = delete `Settings.xml` while the app is closed**, because a parse failure silently
   falls back to defaults.

`[code]` Also: single instance via a global mutex; auto-stop if no action repeats forever;
invalid actions silently skipped; `ServicePointManager.SecurityProtocol` still enables **SSL3
and TLS 1.0**.

---

## 7. The importer

### Where the file lives

| Build | `Settings.xml` |
|---|---|
| GitHub / portable | `%AppData%\Ellanet\Move Mouse` |
| Store | `%LocalAppData%\Packages\1258EllAbi.MoveMouse_hjfwaxvfbwh7t\LocalCache\Roaming\Ellanet\Move Mouse` |

Overridable per launch with `/WorkingDirectory:` in the GitHub build only.

### Format

Plain .NET `XmlSerializer` output, root `<Settings>`, element names identical to property names.

```xml
<Settings>
  <LowerInterval>30</LowerInterval>          <!-- int, seconds -->
  <UpperInterval>60</UpperInterval>
  <RandomInterval>false</RandomInterval>
  <AutoPause>false</AutoPause>
  <AutoResume>false</AutoResume>
  <AutoResumeSeconds>30</AutoResumeSeconds>
  <ActiveWhenLocked>false</ActiveWhenLocked>
  <PauseOnBattery>false</PauseOnBattery>
  <StartAtLaunch>false</StartAtLaunch>
  <EnableLogging>false</EnableLogging>
  <LogLevel>Verbose</LogLevel>
  <!-- appearance flags omitted — none map to our model -->
  <Actions>   <!-- element name = the action class name --> </Actions>
  <Schedules> <!-- SimpleSchedule | AdvancedSchedule --> </Schedules>
  <Blackouts> <!-- Blackout --> </Blackouts>
</Settings>
```

**Action element names** — exactly these nine, and note they differ from the UI labels:

| XML element | UI label |
|---|---|
| `MoveMouseCursorAction` | Move Mouse Cursor |
| `ClickMouseAction` | Click Mouse Button |
| `ScrollMouseAction` | Scroll Mouse Wheel |
| `PositionMouseCursorAction` | Position Mouse Cursor |
| `ActivateApplicationAction` | Activate Application |
| `CommandAction` | **Run Command** |
| `ScriptAction` | **PowerShell Script** |
| `SleepAction` | Sleep |
| `KeystrokeAction` | Send Keystrokes |

Per-type fields:

| Type | Fields |
|---|---|
| `MoveMouseCursorAction` | `Distance`, `UpperDistance`, `Random`, `Direction`, `Speed`, `Delay`, `AbortIfUserActivityDetected` |
| `ClickMouseAction` | `Button`, `Hold`, `HoldInterval` |
| `ScrollMouseAction` | `Distance`, `UpperDistance`, `Random`, `Direction` |
| `PositionMouseCursorAction` | `X`, `Y` |
| `ActivateApplicationAction` | `Mode`, **`Application`** |
| `CommandAction` | `FilePath`, `Arguments`, `WaitForExit`, `Hidden` |
| `ScriptAction` | `ScriptPath`, `WaitForExit`, `Hidden` |
| `SleepAction` | `Seconds`, `UpperSeconds`, `Random` |
| `KeystrokeAction` | `Keystrokes` (int VK codes), `Method`, `Pause`, `PauseInterval`, `AbortIfUserActivityDetected` |

Schedules and blackouts:

- `SimpleSchedule` — `Action`, `Time`, `Delay`, `Monday`…`Sunday`, `IsEnabled`
- `AdvancedSchedule` — `Action`, `Schedule` (Quartz cron string), `IsEnabled`
- `Blackout` — `Time`, `Duration`, `Monday`…`Sunday`, `IsEnabled`

### ⚠️ Parser gotchas

1. **`Time` and `Duration` are `xs:duration`, not `HH:mm`.** `PT9H30M`, not `09:30`.
2. **Enums serialize as names, not integers.** `Square`, not `0`.
3. **Day-of-week is seven booleans, not a bitmask.**
4. **`Application` ≠ the UI's "Name".** The importer must map the element, not the label.
5. **There is no version or schema attribute.** Detect by which elements are present.
6. `LaunchAtLogon` is legacy and does not reflect reality — the real state is in the Run key or
   the UWP `StartupTask`.

### Mapping

| Move Mouse | Ours |
|---|---|
| Interval + Random | `Trigger::Interval { base, jitter }` |
| Auto-pause / auto-resume | `Condition::UserIdleFor` + C6 stand-down |
| Continue when locked | `Condition::SessionUnlocked`, inverted |
| Pause on battery | `Condition::OnACPower` |
| Blackout | `Condition::TimeWindow`, negated |
| Simple Schedule Start/Stop | `Condition::TimeWindow` |
| Advanced Schedule (cron) | best-effort translation; **on failure, import the rule disabled with the cron string preserved as a note** rather than dropping it |
| Move Mouse Cursor, Direction=None | `Action::VirtualJiggle` |
| Move Mouse Cursor, other | `Action::MoveRelative` / `MoveAlongPath` |
| Click / Scroll / Position / Keystroke | direct equivalents |
| Activate Application | **not imported** — report it |
| Run Command / PowerShell Script | **not imported by default** — report it |

The import report is part of the feature, not an afterthought. It must say plainly what was
imported, what was translated approximately, what was dropped and why — and offer to keep the
original file. Silently discarding a user's configuration is worse than refusing to import it.

**And every imported configuration gets a power-mode suggestion.** Someone whose Move Mouse
config is a 30-second jiggle to stop the machine sleeping should be told, on the import screen,
that **Keep running** does that with no synthetic input at all. That is the moment the whole
argument in [PRODUCT.md](PRODUCT.md) becomes concrete for the person it matters to.

---

## Sources

[Wiki home](https://github.com/sw3103/movemouse/wiki) ·
[CLI](https://github.com/sw3103/movemouse/wiki/Command-Line-Interface-(CLI)) ·
[File Locations](https://github.com/sw3103/movemouse/wiki/File-Locations) ·
[Installation](https://github.com/sw3103/movemouse/wiki/Installation) ·
[PowerShell Snippets](https://github.com/sw3103/movemouse/wiki/PowerShell-Snippets) ·
[Scenarios](https://github.com/sw3103/movemouse/wiki/Scenarios) ·
[Troubleshooting](https://github.com/sw3103/movemouse/wiki/Troubleshooting) ·
[Uninstall](https://github.com/sw3103/movemouse/wiki/Uninstall) ·
[Privacy Policy](https://github.com/sw3103/movemouse/wiki/Privacy-Policy) ·
[Releases](https://github.com/sw3103/movemouse/releases)
