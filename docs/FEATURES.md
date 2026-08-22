# Feature specification

Organised by **mechanism**, because that distinction is the product — see
[PRODUCT.md §2](PRODUCT.md#2-the-three-mechanisms).

- **Part A — the wake engine.** Power inhibition. Sanctioned APIs, zero policy risk, **on by
  default**. This is what the software is.
- **Part B — conditions.** What makes a wake lock *task-bound* instead of a permanent override.
  The largest unserved gap in the category, and the reason to build this at all.
- **Part C — the input engine.** Synthetic input. **Off by default**, opt-in, honestly
  labelled. Necessary for session and presence timers, which power inhibition cannot touch.
- **Part D — the app.** Tray, profiles, logging, settings, distribution.
- **Part E — diagnostics.** Explaining the system to the user. Cheap to build, unbuilt by
  everyone, and the best trust signal we have.

Every feature is a composition of the `Trigger` / `Condition` / `Action` model in
[ARCHITECTURE §5](ARCHITECTURE.md#5-the-rule-engine). If a feature cannot be expressed that
way, it belongs in the engine, not in this document.

Sections are marked **★ Flagship** where the feature is a reason to build this rather than use
something that exists, and **⚠️** where there is a caveat the user must be told about. Anything
we cannot do at all is in *What we will not build* at the end.

---

# Part A — The wake engine

**On by default. No input is synthesized. Nothing persistent is modified.**

## A1. The two modes

**★ Flagship.**

| Mode | System sleep | Display off | Lock / screensaver |
|---|---|---|---|
| **Keep running** | blocked | allowed | allowed |
| **Keep presenting** | blocked | blocked | blocked |
| Off | — | — | — |

Two independent, independently-persisted states, borrowed from
[wakepy](https://wakepy.readthedocs.io/stable/), which has the cleanest model in the category.

Implemented with `PowerCreateRequest` / `PowerSetRequest`:

| Mode | Requests |
|---|---|
| Keep running | `PowerRequestSystemRequired` + `PowerRequestExecutionRequired` |
| Keep presenting | the above + `PowerRequestDisplayRequired` |

Getting this wrong generates bug reports in *both* directions. PowerToys has one open issue for
the screen not staying on and another for it staying on when it shouldn't.

## A2. Modern Standby (S0) correctness

**★ Flagship.**

The single highest-value thing in the project.

`SetThreadExecutionState` only resets idle timers. On a Modern Standby machine with the display
off, it **fails to prevent connected standby**. `PowerRequestExecutionRequired` is the flag
that actually holds a modern system awake, and it exists only on the
`PowerCreateRequest`/`PowerSetRequest` path.

Every major competitor has an open bug for this. It is not a differentiator that needs
marketing — it is the difference between the software working and not working on any laptop
sold in the last several years.

Acceptance test: on an S0 machine, display off, lid closed, the machine must still be reachable
after 8 hours with **Keep running** active. This is in CI as a manual gate per release.

## A3. Handle-scoped, self-releasing, auditable

`PowerSetRequest` takes a `REASON_CONTEXT` string. Use it, and make it specific:
`"project-mouse: Keep running — bound to msbuild.exe (PID 8124)"`.

Three things follow, all of them trust signals:

- The request appears in **`powercfg /requests`**, attributed to us by name, with the reason a
  human can read. An IT administrator can audit exactly what we are doing and why.
- The request is tied to a **handle**, not a thread — unlike `SetThreadExecutionState`, whose
  thread affinity is undocumented folklore (see
  [WINDOWS-API gotcha 2](WINDOWS-API.md#gotcha-2--setthreadexecutionstate-is-thread-affine)).
- It is released cleanly. *"Does not modify your power plan; releases everything on exit"* is
  the sentence administrators want, and it must remain literally true.

⚠️ Release on exit is a correctness requirement, not politeness. A crash that leaks the request
leaves a machine that will not sleep with no visible cause. Clear it in the exit handler **and**
in a panic hook, and assert in CI that `powercfg /requests` is clean after the process ends.

## A4. Block shutdown / restart / logoff

**⚠️ Caveats below.**

Don't Sleep's distinguishing feature, and genuinely useful for an unattended overnight job.
Also genuinely obnoxious if it surprises someone.

Ship it, off by default, time-bounded (never indefinite), with a visible countdown, and always
overridable. A tool that silently refuses to let a machine restart is a tool an administrator
uninstalls organisation-wide.

## A5. Screensaver suppression

Covered by `PowerRequestDisplayRequired` in **Keep presenting**. Do not additionally poke
`SPI_SETSCREENSAVEACTIVE` — that mutates a persistent system setting, which violates A3.

---

# Part B — Conditions

**This is the product.** Amphetamine has roughly fourteen trigger conditions on macOS. Windows
has essentially none. Every issue tracker in the category is full of variations on *"only when
I actually need it"*, and users volunteer energy-consciousness as the reason.

A condition can gate the wake engine, the input engine, or both.

## B1. Bind to a process

**★ Flagship.**

Stay awake while a named process is running; release the moment it exits.

Match by **name**, not PID. This is the explicit ask in
[PowerToys #27980](https://github.com/microsoft/PowerToys/issues/27980) — the requester's sync
tool respawns itself under a new PID, so PID-binding breaks. Both that issue and
[#44512](https://github.com/microsoft/powertoys/issues/44512) are open.

Support:

- process name, case-insensitive (`msbuild.exe`)
- full path, to disambiguate — several `Update.exe` and `chrome.exe` exist on a normal machine
- multiple processes: hold while *any* named process is alive
- a **"pick from running processes"** UI, since typing an executable name correctly is a
  surprisingly high failure rate

This one feature covers builds, renders, training runs, transfers, backups, and AI agents —
most of §3 of PRODUCT.md — with no user configuration beyond choosing an application.

## B2. Bind to a schedule

Days of week plus a time window. The single most-requested unshipped feature across
PowerToys, Move Mouse, and Mouse Jiggler.

The canonical statement is [PowerToys #42720](https://github.com/microsoft/PowerToys/issues/42720):
*"I have dashboards and automations on my system that I actively monitor during office hours…
I don't require monitoring off-hours and would rather set & forget a schedule."*

Store local time plus the IANA zone; recompute on resume and on `WM_TIMECHANGE`, so a laptop
crossing a timezone does not run a 9–5 rule at the wrong hours.

## B3. Bind to an expiry

"Stay awake for 2 hours", "until 18:00", "until this download finishes". Requested as
[PowerToys #46646](https://github.com/microsoft/powertoys/issues/46646).

Always show the remaining time in the tray tooltip. An expiring lock the user has forgotten
about is the same problem as a permanent one.

## B4. Bind to load

CPU above a threshold, or network throughput above a threshold — Don't Sleep is currently the
only Windows tool with either.

This is the *"the machine is at 100% CPU and still went to sleep"* case solved without the user
having to name a process. Sample via `GetSystemTimes`; no PDH, no WMI.

Include a **grace period** (default 60 s) so a build's momentary gaps between compilation units
do not release the lock.

## B5. Bind to power source

On AC only, or above a battery percentage. Move Mouse has AC/DC only, with no percentage
threshold. A wake lock that flattens a laptop in a bag is a bug report.

Default: pause below 20% on battery.

## B6. Bind to session state

Locked, unlocked, remote session, console session. `WTSRegisterSessionNotification` plus
`GetSystemMetrics(SM_REMOTESESSION)`.

Most rules should stop when the workstation locks. Some — an overnight build — should not.
Make it explicit rather than implicit.

## B7. Stand down while presenting or screen-sharing

**★ Flagship.**

`SHQueryUserNotificationState` returns, in one cheap call:

| Value | State | Meaning |
|---|---|---|
| 1 | `QUNS_NOT_PRESENT` | Screensaver up, machine locked, or an inactive Fast User Switching session |
| 2 | `QUNS_BUSY` | A fullscreen application |
| 3 | `QUNS_RUNNING_D3D_FULL_SCREEN` | Fullscreen exclusive Direct3D — a game |
| 4 | `QUNS_PRESENTATION_MODE` | Presentation mode |
| 5 | `QUNS_ACCEPTS_NOTIFICATIONS` | Normal |
| 6 | `QUNS_QUIET_TIME` | Quiet hours |
| 7 | `QUNS_APP` | A Store app running full screen |

That single call covers "pause during games", "pause during presentations", "pause during
fullscreen video", **and** "is the screen locked" — four separate features in competing tools.

The direct motivation is [Move Mouse #97](https://github.com/sw3103/movemouse/issues/97): the
tool resumed during a presentation and the audience watched the cursor twitch. Note the shape
of the requirement — during a presentation the user wants the display kept on and input
synthesis **absolutely suppressed**. One toggle cannot express that; two mechanisms can.

⚠️ Do not reimplement this by comparing window rectangles to monitor rectangles. That
heuristic misfires on borderless windows and multi-monitor layouts.

## B8. Bind to network

**Milestone 4.**

Wi-Fi SSID, or connected/disconnected. [Move Mouse #105](https://github.com/sw3103/movemouse/issues)
asks for "only activate when not connected to a specific Wi-Fi network" — run at home, not at
the office. Amphetamine has had this for years.

## B9. Bind to a device

**⚠️ Caveats below · Milestone 4.**

A USB or Bluetooth device connected or disconnected — a dock, a headset, an instrument.
Amphetamine has it; nothing on Windows does. `RegisterDeviceNotification`.

## B10. Foreground application

Hold, or suppress, while a given application has focus. This is the "never synthesize input
while Photoshop or a game is focused" guard.

Use `QueryFullProcessImageNameW` with `PROCESS_QUERY_LIMITED_INFORMATION` — **not**
`GetModuleFileNameExW`, which fails across 32/64-bit boundaries and on elevated processes.

## B11. Idle-based gating

Both directions:

- **Act only after** the user has been idle for N minutes (the classic jiggler trigger)
- **Stand down immediately** when the user returns — see C6, which is P0

## B12. Composition

`AllOf` / `AnyOf` / `Not`. A rule like *"weekdays, 09:00–18:00, while `msbuild.exe` is running,
on AC power, unless I'm presenting"* must be expressible without a special-case code path.

---

# Part C — The input engine

**Off by default.** Enabling it presents a plain-language explanation of what changes: the
software begins synthesizing input, which is detectable, may violate an acceptable-use policy,
and is the only way to defeat a session or presence timer.

No apology, no warning triangle theatre — one honest paragraph, once.

## C1. Virtual jiggle (default when input is enabled)

**★ Flagship.**

Reset the session idle timer **without the visible cursor moving at all**. A relative
`SendInput` of (0, 0), or a movement immediately reversed within one frame.

arkane-systems calls this **Zen mode**; Move Mouse calls it **Stealth** and reaches it by the
non-obvious route of setting Direction to None on a movement action, leaving a Distance field
visible but ignored. Here it is its own action, and it is the **default**.

It solves three complaint categories at once:

| Complaint | Source |
|---|---|
| The cursor jumps across monitors instead of jiggling | jetKVM #817 — "unusable" |
| The fast version makes the mouse unusable while you work | AnandTech forum |
| The audience watched my cursor twitch during a presentation | Move Mouse #97 |

If it is not visible, none of those can happen.

## C2. Visible movement

For the cases where invisible motion is not enough — notably remote sessions, where Move
Mouse's own documentation warns that *cursor movement often is not detected inside the remote
session* and recommends a click instead.

| Mode | |
|---|---|
| Relative nudge | N pixels, direction or pattern |
| Return to origin | capture at *sequence* start, restore at end |
| Absolute position | to a chosen point on a chosen monitor |
| Path | line, square, or curve between two points |

⚠️ **Relative movement passes through pointer acceleration.** `dx: 10` does not move exactly
10 pixels; it goes through the system pointer-speed and Enhanced Pointer Precision curve. For
anything that must be exact, read `GetCursorPos` and send an **absolute** move.

⚠️ **Absolute coordinates are normalised 0..65535 and default to the primary monitor.** Without
`MOUSEEVENTF_VIRTUALDESK` the cursor can never leave display 1 — the classic multi-monitor
jiggler bug. Store targets as (monitor identity, normalised position) so a configuration
survives a resolution change or a monitor being rearranged.

## C3. Keystroke — and the default matters

Any virtual key, plus chords, plus sequences with a configurable inter-key delay.

**Default: `VK_F15` (0x7E).** Not `Shift`, not `Scroll Lock`.

| Key | Problem |
|---|---|
| `Shift` | Modifies the focused control. A stray `Shift` into a text field, spreadsheet, or game is a real bug users report as "it typed something." |
| `Scroll Lock` / `Num Lock` / `Caps Lock` | Toggles real keyboard state and LEDs. Caps Lock corrupts typing. |
| `Ctrl` / `Alt` | A bare modifier press opens menus in some applications. |
| **`VK_F15`** | A real virtual key that resets the idle timer. Essentially nothing binds F13–F24. |

⚠️ **F15 is not universally safe either.** Caffeine's own documentation records that its F15
default breaks in **PuTTY, PowerPoint, Google Docs, and Smartsheet**. So the key must be
configurable, and the UI should say why someone might change it — a fact that only shows up
after users hit it.

## C4. Click and scroll

**⚠️ Caveats below.**

Left, right, middle, double; scroll by wheel notches.

⚠️ **A synthetic click lands on whatever is under the cursor.** If the user left a dialog
focused, an automated click can answer it. A movement-only tool cannot destroy anything; a
clicking one can.

Off by default, gated behind a foreground-application condition by default, and the UI says so
in one line. Pair with C2 absolute positioning so the click lands somewhere deliberately safe —
which is exactly the recipe Move Mouse's own remote-session guide gives ("hover it over the
Start Button in the remote session").

## C5. Randomisation

**⚠️ Read this before implementing.**

Interval jitter, distance range, and path variation.

**The honest framing, which is a product decision and not a technical one:**
[ActivTrak's detection signal #2](https://support.activtrak.com/hc/en-us/articles/4406765537563-Detect-Mouse-Jigglers-and-Other-Activity-Mimicking-Tools)
is *"uniform, machine-regular input timing."* Which means a feature marketed as **"randomised
so it looks human"** is, by construction, an anti-detection feature — and fails Test 3 in
[PRODUCT.md §5](PRODUCT.md#5-the-line).

Randomisation still ships, because it has real benign purposes:

- a cursor that always lands on the same pixel will eventually land somewhere destructive
- a fixed interval synchronises badly with other periodic events
- varied motion is less startling to look at

But its purpose changes, and so does every word describing it. It is **"vary the movement so it
is less intrusive."** It is not a flagship. And no UI string, README line, or release note ever
describes it as human-like, natural, or undetectable.

An earlier draft of this document had Bézier-curve human-motion synthesis with overshoot and
micro-jitter as a headline feature. That is cut. If we cannot describe a feature honestly in
the UI, we do not ship it.

## C6. Stand down on real user input

**★ Flagship · P0.**

The moment genuine human input arrives, cancel any in-flight sequence and suppress the next
trigger. Resume only after the idle threshold is met again.

**This is P0 despite being a "new" feature.** It is the difference between a tool that behaves
and a tool that fights its user, and it is the number-one usability complaint about everything
in the category.

It requires the self-injection filter in
[ARCHITECTURE §6](ARCHITECTURE.md#6-the-self-injection-feedback-loop) — without it the software
can never tell that the user came back, because it manufactures input indistinguishable from
theirs.

Move Mouse implements a version of this by polling `GetLastInputInfo` every 250 ms, which makes
"activity detected" racy and forces three separate hacks elsewhere in its codebase. Tagging our
own injected input and tracking the injection timestamp removes all three.

## C7. Detect and report when injection is silently failing

**★ Flagship.**

`SendInput` fails **undetectably** under UIPI. Microsoft's own words:

> *"This function fails when it is blocked by UIPI. Note that neither GetLastError nor the
> return value will indicate the failure was caused by UIPI blocking."*

So when a process at higher integrity owns the foreground window — a UAC prompt, an elevated
editor, the lock screen — the call reports success and the input goes nowhere.

Move Mouse discovered this the hard way and added a detector in v4.16.3: compare
`GetLastInputInfo` before and after any action that should have reset it. If it did not move,
log a warning. **Adopt that technique, and go further:** surface it in the tray icon, the
tooltip, and the status page as a first-class **Blocked** state.

"It's running but nothing is happening" is the single most common support complaint about every
tool in this category. A tooltip answers it for free.

---

# Part D — The application

## D1. Tray-first

Tray icon with four visually distinct states — Active / Paused / Auto-paused / Blocked —
distinguishable at 16×16 **in greyscale**. Native context menu carries mode switching, pause,
and profile selection. See [UI-UX.md](UI-UX.md).

## D2. Rules and profiles

A profile is a named set of rules; one active at a time. Switchable from the tray, a hotkey, or
a rule — so *"when Zoom starts, switch to the Presentation profile"* needs no special feature.

Presets that map to the real use cases in [PRODUCT.md §3](PRODUCT.md#3-who-actually-needs-this):
**Long build** · **Presentation** · **Dashboard / wallboard** · **Remote session** ·
**Reading** · **Gaming (power only)**.

## D3. Global hotkey

Toggle without opening anything. An open request against both Move Mouse (#118) and Mouse
Jiggler (#47); neither ships it — Move Mouse's implementation exists in the source but is
entirely commented out.

## D4. Auto-start

`HKCU\...\Run` by default — no elevation, easy to remove. Task Scheduler only for the "before
login completes" case. Always starts minimised to tray, never showing a window.

## D5. Portable mode

**★ Flagship.**

Single `.exe`, config beside it, **no registry writes, no installer, no admin**.

This is not a nice-to-have. The users with the worst version of this problem are on the most
locked-down machines, and Move Mouse's own installation page says the quiet part out loud: the
Store build is *"not normally available on work/corporate machines"* and the portable build is
the *"better option for work/corporate machines."*

## D6. Logging

`tracing` to a rolling file, plus an in-memory ring buffer for the live view.

⚠️ **Never log cursor coordinates at `info`.** A log of everywhere the cursor went is a privacy
problem in a file users attach to bug reports.

## D7. Notifications

Native toast on error and on state changes that would otherwise be invisible — auto-pause,
lock released, blocked-injection detected. **Errors only by default.** A background utility
that notifies on every action is one the user uninstalls.

## D8. Config

Versioned JSON with a migration chain from v1. Atomic write — temp file, fsync, `MoveFileEx`
with replace. Debounced 500 ms.

⚠️ **Never silently fall back to defaults on a parse error.** That is Move Mouse's behaviour and
it is why its documented reset procedure is "delete Settings.xml while the app is closed."
Surface the error, keep the broken file, offer a reset button.

Import and export as first-class features, which also makes the Move Mouse importer symmetric.

## D9. Import from Move Mouse

Read `Settings.xml` and translate it into rules. Cheap to build, and it removes the only real
reason for an existing user not to switch. Full schema in [MOVE-MOUSE.md](MOVE-MOUSE.md).

## D10. CLI

Move Mouse has exactly one switch (`/WorkingDirectory:`), available only in one of its two
distribution channels. Ours:

```
project-mouse --keep running --while-process msbuild.exe
project-mouse --keep presenting --until 18:00
project-mouse --profile "Long build"
project-mouse --status          # JSON, for scripting
project-mouse --release
```

This makes the software scriptable into a build pipeline or an agent hook — which is the
Insomnia use case, and the fastest-growing framing in the category.

## D11. Auto-update

Tauri updater against our own endpoint with a GitHub fallback. Background download, then a
"restart to update" affordance. See [UPDATES.md](UPDATES.md).

## D12. Scripting escape hatch

**⚠️ Caveats below · Milestone 6.**

Move Mouse's extensibility is a **PowerShell Script** action that spawns
`powershell.exe -ExecutionPolicy Bypass -File` on every interval. That is hundreds of
milliseconds and a new process each time, it normalises bypassing execution policy, it cannot
pass parameters, and it captures neither output nor exit code.

Ours instead:

- **`RunCommand`** action with `ShellExecute` semantics, environment-variable expansion, an
  async wait with a **timeout** (never an unbounded `WaitForExit` on the action thread), and an
  explicit one-time user confirmation per distinct command
- **Rhai scripts** for custom conditions — sandboxed, no filesystem, no network

⚠️ A DLL-loading plugin ABI is explicitly rejected. Third-party native code inside a process
that injects input is a bad trade in a project already fighting an antivirus-reputation battle.

---

# Part E — Diagnostics

Cheap, unbuilt by everyone, and the strongest trust signal available.

## E1. "Why is my PC awake?"

**★ Flagship.**

[PowerToys #44501](https://github.com/microsoft/powertoys/issues/44501) asks for exactly this.
`powercfg /requests` answers it and essentially no normal user knows the command exists.

A panel listing every process currently holding a power request, in plain language, with the
offender named. Worth installing the software for even if you never turn the main function on —
and it demonstrates, inside the product, the transparency we are asking administrators to
trust us on.

## E2. Live idle clocks

Two numbers, always visible on the status page:

- **System idle** — what Windows, the screensaver, and any presence client see
- **Human idle** — what the *user* actually did, with our own injections filtered out

This diagnoses, in one glance, the two failure modes that generate the most confused bug
reports across the category:

- **The NVIDIA GeForce Experience bug**, which Move Mouse's wiki documents as still arriving in
  its inbox years later: the idle timer constantly resets to zero, so anything idle-dependent —
  screensavers included — *falsely detects user activity when there is none*, and auto-pause
  never lets the tool run.
- **UIPI blocking** (C7): idle time does not move after an injection that reported success.

Move Mouse only added an idle-time readout in v4.19.0, buried in its About box. It should be on
the front page.

## E3. Effect readout

Not "the tool is running" but **what is currently true of the machine**:

```
System sleep      blocked      until msbuild.exe exits
Display off       allowed
Screen lock       allowed
Input synthesis   off
```

## E4. Memory and tick timing

The README makes a public promise about footprint. The application should be willing to be
checked against it, in the place the user is already looking — and CI reads the same numbers.

---

# What we will not build

| | Why |
|---|---|
| Global keyboard/mouse hooks | System-wide input latency; the strongest antivirus keylogger heuristic |
| `Win + L` interception | Impossible — a `winlogon` system hotkey, no user-mode hook can suppress it |
| Disabling `Win + L` via the `DisableLockWorkstation` policy | Possible, deliberately not built. It removes the user's own lock shortcut entirely, is a security downgrade on a shared machine, and writing policy keys is exactly what reclassifies an app as potentially-unwanted |
| Anything marketed as undetectable, or as evading monitoring | [PRODUCT.md §5](PRODUCT.md#5-the-line), Test 3 |
| Input injection into games | Kernel anti-cheat bans for far less |
| A DLL plugin ABI | Third-party native code in a process that injects input |
| Boss-key disguise — override window title, override tray icon, hide both window and tray | Move Mouse ships all of these. They are exactly the traits corporate EDR and DLP tooling flags, and hiding both window and icon leaves the user with no way to reach settings at all |
| Adjusting system volume while running | Move Mouse does this. It is a historical artefact, and mutating the default playback device is a surprising side effect for a wake lock |
| Screen-burn jitter on our own window | A workaround for an app that expects to sit visible for days. Tray-only by default, so the problem does not exist |
| Telemetry or analytics | A tool that watches your input must not phone home. Ever. |
| Cloud sync | Same reason. Profiles are a local JSON file the user can copy. |
| A mobile companion app | Requires a network-reachable endpoint on the user's machine |

---

# Priority

| Tier | Features |
|---|---|
| **P0 — the product works** | A1 two modes · A2 Modern Standby · A3 handle-scoped and auditable · B1 process binding · B3 expiry · D1 tray · D4 auto-start · D5 portable · D6 logging · D8 config · **C6 stand-down** · **C7 blocked detection** · **E2 idle clocks** |
| **P1 — the product is worth choosing** | B2 schedule · B5 power source · B6 session state · B7 presentation stand-down · B10 foreground app · B11 idle gating · B12 composition · C1 virtual jiggle · C3 keystroke · D2 profiles · D3 hotkey · **E1 why-is-my-PC-awake** · E3 effect readout |
| **P2 — polish** | A4 block shutdown · A5 screensaver · B4 load binding · C2 visible movement · C4 click and scroll · C5 randomisation · D7 notifications · D10 CLI · D11 updates · E4 diagnostics |
| **P3 — later** | B8 network · B9 device · D9 Move Mouse import · D12 scripting |

Two notes on the ordering, because both look wrong at first glance:

**C6 and C7 are P0 despite living in the opt-in half of the product.** They are not
enhancements to input synthesis; they are the conditions under which shipping input synthesis
at all is defensible. Without C6 the tool fights its user. Without C7 it lies about working.

**E2 is P0 because it is how everything else gets debugged.** Both of the failure modes that
generate the most confused support traffic in this category are invisible without it.
