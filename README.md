# project-mouse

An open-source Windows wake lock with a rules engine behind it.

> **A task-bound wake lock.** It keeps the machine awake for exactly as long as your work
> actually needs, and not one second longer.

It does not modify your power plan. It releases everything on exit. By default it synthesizes
no input at all.

*(`project-mouse` is a working title — see [PRODUCT.md §9](docs/PRODUCT.md#9-the-name).)*

---

## Why

Windows decides whether you are present by watching for **input**, not for **work**. A machine
can sit at 100% CPU compiling, rendering, or training a model, and still go to sleep, because
nobody touched the mouse.

Applications are supposed to tell the OS when they are busy. Most of them forget. The clearest
statement of the problem comes from a user filing a bug against Adobe Media Encoder after a
render queue died overnight:

> *"AME should alert the OS that it's busy and prevent sleep until the last job in the queue is
> finished."*

That is this entire product category in one sentence. We are the bolt-on for every application
that should have asserted a wake lock and didn't.

---

## Two modes, not one toggle

Most tools in this category have a single switch, and users complain in **both** directions —
"it won't let my screen sleep" and "my screen slept when I told it not to." Those are two
different needs, so there are two states, held independently:

| Mode | Keeps | Allows | For |
|---|---|---|---|
| **Keep running** | The system awake, work continuing | The screen may dim, blank, and lock | Builds, renders, training runs, transfers, backups |
| **Keep presenting** | The system awake **and** the display lit and unlocked | Nothing | Dashboards, wallboards, presentations, kiosks, reading |

---

## The three things worth building this for

### 1. Modern Standby is broken in every competing tool

`SetThreadExecutionState` — the API almost everything uses — only resets idle timers. On a
Modern Standby (S0) machine with the display off, it **fails to prevent connected standby.**
The correct call is `PowerSetRequest` with `PowerRequestExecutionRequired`.

Every major tool in the category has an open bug for this:
[PowerToys #48965](https://github.com/microsoft/powertoys/issues/48965) ·
Mouse Jiggler #130 · [Move Mouse #109](https://github.com/sw3103/movemouse/issues/109).

As S3 sleep disappears from new laptops, this is the whole market breaking at once. If this
project ships one thing correctly, it is this.

### 2. Windows has no Amphetamine

On macOS, [Amphetamine](https://apps.apple.com/us/app/amphetamine/id937984704) can stay awake
*while an app is running*, *while a USB device is connected*, *while you are on a given Wi-Fi
network*, *while CPU is above a threshold*, *until a set time*. On Windows, nothing comes close.

The most-requested and least-shipped feature across every issue tracker in this category is
some version of **"only when I actually need it"** —
[bind to a process](https://github.com/microsoft/PowerToys/issues/27980) ·
[bind to a schedule](https://github.com/microsoft/PowerToys/issues/42720) ·
bind to an expiry time · bind to a network. Users are explicit that they do not want *always
on*; they want *conditionally on*, and they say so for energy reasons as often as for
convenience.

Conditionality is the product.

### 3. Nobody can tell you why their PC is awake

[PowerToys #44501](https://github.com/microsoft/powertoys/issues/44501) — *"Awake? I'd prefer
Asleep!"* — asks for the inverse feature. `powercfg /requests` answers it, and essentially no
normal user knows that command exists.

A panel that lists every process currently holding a power request, in plain language, with the
offender named, is worth installing this software for even if you never turn the main function
on.

---

## Budgets

Not vanity. On locked-down machines — precisely the machines with the worst version of this
problem — *no installer, no runtime, single file, runs from a USB stick, works without admin*
is decisive. The nearest competitors are 267 KB (Don't Sleep) and 306 KB (Caffeine); the
heaviest ships 134 MB.

CI fails if these regress.

| Metric | Target |
|---|---|
| Idle private working set, tray only, no window | **≤ 8 MB** |
| With the settings window open | ≤ 150 MB, transient — the webview is destroyed on close |
| Idle CPU averaged over 10 min | **≤ 0.05 %** |
| Installed size | ≤ 8 MB |
| Cold start to tray icon | ≤ 250 ms |
| Runtime dependencies | **none** |
| Admin rights required | **never** |

---

## Stack

| Layer | Choice | Why |
|---|---|---|
| Core | Rust 2021 | No runtime, no GC, predictable footprint |
| Shell | Tauri v2 | Tray, updater, packaging — without shipping a browser |
| OS bindings | `windows` crate | Direct Win32, no marshalling layer |
| UI | React + TypeScript | Loaded only when a window exists |
| Config | JSON, atomic write | The whole dataset is tens of KB; SQLite would cost ~1 MB of binary for nothing |
| Logging | `tracing` + rolling file | Off at `debug` by default |

**The load-bearing idea:** the settings window is open about two minutes a day, so the webview
is created on demand and **destroyed** on close — never hidden. That single discipline is where
the 8 MB comes from.

⚠️ It also rests on behaviour Tauri does not document. **Measure it before building anything
else** — see [ROADMAP.md](docs/ROADMAP.md) M0.

Rejected: WinUI 3 (60–120 MB idle, heaviest of all despite looking modern), WPF (35–70 MB
floor, no NativeAOT), Avalonia (~30–50 MB — the best .NET option and a real fallback, but it
only ties the incumbent rather than beating it), Electron (not seriously considered).

Cross-platform is deliberately *not* v1, but the platform boundary is defined from commit one.
See [CROSS-PLATFORM.md](docs/CROSS-PLATFORM.md).

---

## What this will not do

Stating these plainly, because two of them appear on every product page in this category and
neither is achievable.

1. **It cannot intercept `Win + L`.** That combination is a system hotkey claimed by
   `winlogon`, which calls `LockWorkStation` directly. No user-mode hook can suppress it. What
   *is* possible is preventing *inactivity-based* lock, sleep, and screensaver.
2. **It cannot guarantee any chat client's presence state.** Teams presence is derived from the
   client's own signals, calendar state, device lock state, and tenant policy. Microsoft
   [documents](https://learn.microsoft.com/en-us/microsoftteams/presence-admins) that even a
   Teams administrator cannot customise the timings. Keeping the machine active usually helps.
   It is not a contract, and we will not sell it as one.
3. **It will never describe itself as undetectable.** It is detectable, by design, and by
   commercial products that
   [ship jiggler detection as a feature](https://support.activtrak.com/hc/en-us/articles/4406765537563-Detect-Mouse-Jigglers-and-Other-Activity-Mimicking-Tools).
   Any feature that only makes sense as detection evasion does not ship —
   see [PRODUCT.md §5](docs/PRODUCT.md#5-the-line).
4. **No global input hooks.** `SetWindowsHookEx` adds latency to every system keystroke and is
   the strongest single heuristic antivirus engines use to classify something as a keylogger.
   Idle detection uses `GetLastInputInfo` polling.
5. **No input injection into games.** Kernel-level anti-cheat bans for far less. Games are
   supported through power inhibition only.
6. **No kernel driver, no policy-key writes, no MDM bypass.** If a corporate policy is designed
   to detect synthetic input, this software will be detected by it.

---

## Distribution

`SendInput` plus process enumeration plus auto-start plus tray-only presence is, behaviourally,
a description of a RAT. Plan for the friction.

**A correction to the received wisdom:** since 2024, **EV certificates no longer bypass
SmartScreen.** Microsoft's own guidance now places EV, OV, and Azure Artifact Signing in the
same bucket — reputation accrues per file hash over time. No amount of money buys instant trust.

| Option | Cost | Notes |
|---|---|---|
| **[SignPath Foundation](https://signpath.org/)** | **Free** | Built for OSS. Needs an OSI license without commercial dual-licensing, no proprietary components, MFA on maintainer accounts, a published signing policy, and builds verifiable from source. |
| Azure Artifact Signing | ~$9.99/mo | **Individual developers: USA and Canada only.** |
| OV certificate | $150–300/yr | HSM or token required since June 2023. |
| EV certificate | $400+/yr | No longer buys a SmartScreen bypass. Not worth it. |

The SignPath list is not a hurdle so much as a description of a well-run project, which is why
the repo should satisfy it from the start.

Alongside: submit each signed release to the
[Microsoft Defender false-positive portal](https://www.microsoft.com/en-us/wdsi/filesubmission)
*before* shipping; ship a winget manifest; publish reproducible build instructions and the
release SHA. For a tool in this category, verifiability is the marketing.

**Defaulting to power inhibition is also the best antivirus mitigation available** — a tool
whose default behaviour is a documented Windows API call has far less reason to be flagged than
one whose default is injecting HID events.

---

## On use

The honest use cases are broad and boring: long builds and renders, overnight transfers,
training runs, monitoring dashboards and wallboards, presentations, Citrix and RDP sessions
that time out mid-task, lab instruments whose acquisition breaks if the machine sleeps, kiosks,
and people who read slowly enough that their screen keeps locking on them.

It can also be used to fake presence to an employer. That is between the user and their
acceptable-use policy, and this project takes no position on it — but the README will not
pretend the tool is something other than what it is, and nothing in the software is designed to
help with that specific goal.

---

## Roadmap

Milestones with checkable exit criteria are in [ROADMAP.md](docs/ROADMAP.md). The short version:

| | | |
|---|---|---|
| **M0** | Spike | Prove the two undocumented assumptions the whole stack rests on. Blocks everything. |
| **M1** | Wake engine | Tray, three modes, power requests, config. No rules, no UI, no input synthesis. |
| **M2** | Conditions | The rule engine. Process, schedule, expiry, power, session, presentation. |
| **M3** | UI | Settings window, rule builder, diagnostics, first run. |
| **M4** | Input engine | Opt-in. Virtual jiggle, stand-down, blocked detection. |
| **M5** | Ship | Signing, installer, updates, CI budget gate. |
| **M6** | Extend | CLI, Move Mouse import, load and network binding, macOS. |

## Docs

Read in this order.

| | |
|---|---|
| [PRODUCT.md](docs/PRODUCT.md) | **Start here.** The three mechanisms, who actually needs this, competitors, and the line we don't cross. |
| [FEATURES.md](docs/FEATURES.md) | The feature spec, with feasibility flags and priorities |
| [ARCHITECTURE.md](docs/ARCHITECTURE.md) | Process model, rule engine, memory discipline |
| [ROADMAP.md](docs/ROADMAP.md) | Milestones with acceptance criteria — where development starts |
| [UI-UX.md](docs/UI-UX.md) | Interaction model, motion budget, why not a radial menu |
| [WINDOWS-API.md](docs/WINDOWS-API.md) | Win32 mapping and twelve gotchas that will otherwise cost days |
| [TAURI-V2.md](docs/TAURI-V2.md) | Framework config, and the parts Tauri does not document |
| [UPDATES.md](docs/UPDATES.md) | The free auto-update pipeline, end to end |
| [MOVE-MOUSE.md](docs/MOVE-MOUSE.md) | Verified inventory of the incumbent, and the config importer spec |
| [CROSS-PLATFORM.md](docs/CROSS-PLATFORM.md) | The portability boundary and what each OS can actually do |

Interactive UI mockup: https://claude.ai/code/artifact/9551d990-b62a-463f-9937-34bbd7eecf4c

## License

MIT.
