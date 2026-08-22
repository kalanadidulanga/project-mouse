# Roadmap

Every milestone has **exit criteria that can be checked**, not a feature list. A milestone is
done when someone other than the author could verify it.

The ordering is not by feature importance. It is by **what invalidates what** — the things that
could send us back to a different stack come first.

---

## M0 — The spike

**1–2 days. ⚠️ Blocks everything.**

Two assumptions carry the entire design, and **neither is documented anywhere**. If either
fails, the stack choice changes and everything downstream is wasted work.

Throwaway code. Do not build anything reusable here.

### What to prove

**1. Does a destroyed Tauri webview actually return the memory?**

Tauri's documentation says nothing whatsoever about memory or resource release on window
destruction, and each WebView2 window runs its own browser and GPU processes whose shared
environment may or may not be torn down with the last webview.

The entire ≤8 MB budget rests on this. See
[ARCHITECTURE §3](ARCHITECTURE.md#3-window-lifecycle--where-the-memory-budget-lives).

**2. Does `PowerRequestExecutionRequired` actually hold a Modern Standby machine awake?**

This is the flagship feature and the thing every competitor gets wrong. It cannot be verified
in CI — it needs a real S0 laptop with the lid closed.
See [WINDOWS-API gotcha 0](WINDOWS-API.md#gotcha-0--setthreadexecutionstate-does-not-prevent-modern-standby).

### Exit criteria

| | Test | Pass |
|---|---|---|
| 1 | Tray app starts with no window; read private working set | ≤ 10 MB |
| 2 | Open a window, close it with `destroy()`, wait 30 s, read again | back within 2 MB of the starting figure |
| 3 | Repeat open/destroy 20 times | no upward drift |
| 4 | Same cycle using `hide()` instead, for comparison | documented, so the difference is on record |
| 5 | S0 laptop: `PowerRequestSystemRequired` + `ExecutionRequired`, lid closed, 8 h | still reachable; a running job completed |
| 6 | Same test with `SetThreadExecutionState` instead | documented — this is the bug every competitor has |
| 7 | `powercfg /requests` while holding | shows our reason string, attributed to us |
| 8 | Kill the process with Task Manager, then `powercfg /requests` | clean; no leaked request |

Measure with Task Manager **and** VMMap. Task Manager alone hides too much.

### If it fails

- **Test 2 or 3 fails** → the webview is not reclaimable. Fall back to **Avalonia**: ~30–50 MB
  idle, which ties Move Mouse rather than beating it, and the footprint argument in
  [PRODUCT.md §6](PRODUCT.md#6-competitors) weakens considerably. Better to know on day two.
- **Test 5 fails** → the flagship feature does not work as understood. Stop and re-research
  before designing around it.

**Write the numbers down in the repo.** They become the CI baseline in M5, and the "why did we
pick this stack" answer for the next person who asks.

---

## M1 — The wake engine (1–2 weeks)

The product's actual purpose. No rules, no UI, no input synthesis.

**Scope:** tray icon with three modes · `PowerCreateRequest` / `PowerSetRequest` with reason
strings · release on exit and in a panic hook · JSON config with schema version and atomic
write · `tracing` to a rolling file · single instance · auto-start · portable mode.

Referencing [FEATURES](FEATURES.md): A1, A3, A5, D1, D4, D5, D6, D8.

### Exit criteria

- [ ] Tray icon, three modes, switchable from the native menu. No window exists.
- [ ] **Keep running**: system sleep blocked, display allowed to sleep, lock allowed.
- [ ] **Keep presenting**: sleep, display-off, and screensaver all blocked.
- [ ] `powercfg /requests` shows our reason string with the mode named.
- [ ] Quit via the tray → `powercfg /requests` clean.
- [ ] `taskkill /F` → `powercfg /requests` clean.
- [ ] Config survives a forced power-off mid-write (test with a kill during the debounce window).
- [ ] A corrupt config surfaces an error and keeps the file — it never silently resets.
- [ ] Portable: single exe on a USB stick, config beside it, **zero registry writes**, runs
      without admin. Verify with Process Monitor.
- [ ] Idle for 10 minutes: CPU ≤ 0.05 %, working set ≤ 8 MB.

⚠️ **The Quit-and-kill tests are not optional.** A leaked power request leaves a machine that
will not sleep with no visible cause and no running app to explain it.

---

## M2 — Conditions (2–3 weeks)

The reason to build this rather than use PowerToys Awake.

**Scope:** the rule engine · process binding · expiry · schedule · power source · session state ·
presentation stand-down · foreground app · composition.

Referencing [FEATURES](FEATURES.md): B1, B2, B3, B5, B6, B7, B10, B12, D2, D3.

### Exit criteria

- [ ] `--while-process msbuild.exe` holds while it runs, releases within 5 s of it exiting.
- [ ] Process matching survives a **PID change** — the process exits and respawns under a new
      PID, and the lock is held continuously. This is the specific case
      [PowerToys #27980](https://github.com/microsoft/PowerToys/issues/27980) reports.
- [ ] Expiry releases at the stated time; the remaining time is visible in the tooltip.
- [ ] A schedule survives sleep/resume and a timezone change (`WM_TIMECHANGE`).
- [ ] Battery below 20% releases; back on AC re-acquires.
- [ ] Fullscreen game, presentation mode, and a locked screen are each detected via
      `SHQueryUserNotificationState` and act as configured.
- [ ] Two rules wanting different modes → the machine holds the **stronger** one. A rule can
      never weaken what another holds.
- [ ] The rule engine is fully unit-tested against a `MockPlatform` — no Win32 calls in the
      test suite.
- [ ] Global hotkey toggles without any window existing.

---

## M3 — The UI (2–3 weeks)

**Scope:** the settings window, per [UI-UX.md](UI-UX.md) and the
[mockup](https://claude.ai/code/artifact/9551d990-b62a-463f-9937-34bbd7eecf4c) · rule builder ·
diagnostics · first run.

Referencing [FEATURES](FEATURES.md): E1, E2, E3, E4.

### Exit criteria

- [ ] Window is created on demand and **destroyed** on close. Never hidden. Assert it.
- [ ] Working set returns to the M1 baseline within 30 s of closing.
- [ ] Window open to interactive ≤ 400 ms.
- [ ] Rule builder round-trips: build a rule in the UI, verify the JSON, reload, unchanged.
- [ ] **"Why is my PC awake?"** lists other processes' power requests, and says plainly when
      the list is partial because we are not elevated.
- [ ] Both idle clocks visible and correct — verify against `powercfg /requests` and a manual
      stopwatch.
- [ ] Full keyboard navigation; visible focus on every control; no `outline: none`.
- [ ] Renders correctly in Windows High Contrast mode.
- [ ] Nothing in the UI loops or animates at rest. Check with a frame profiler, not by eye.
- [ ] First run creates a working profile in under 10 seconds and **synthesizes no input**.

---

## M4 — The input engine (2 weeks)

Off by default. Everything here is gated behind one explicit user choice.

**Scope:** virtual jiggle · visible movement · keystrokes · stand-down · blocked detection ·
idle gating · randomisation.

Referencing [FEATURES](FEATURES.md): C1, C2, C3, C5, C6, C7, B11, B4, D7.

### Exit criteria

- [ ] Input synthesis is **off** on a fresh install, and enabling it presents one honest
      paragraph explaining what changes.
- [ ] **Virtual jiggle is the default action.** The system idle timer resets; the visible cursor
      does not move by a single pixel. Verify with a screen recording.
- [ ] **Stand-down**: real input cancels an in-flight sequence within 250 ms.
- [ ] The self-injection filter is correct — `human_idle_ms` keeps counting up while the app is
      jiggling. This is the test that proves the tool can be interrupted at all.
- [ ] **Blocked detection**: focus an elevated window, confirm the idle timer does not reset,
      confirm the tray icon and tooltip both report Blocked.
- [ ] Multi-monitor: absolute positioning lands correctly on a **negative-origin** monitor
      (one placed left of or above the primary). This is the classic bug.
- [ ] Mixed-DPI: coordinates correct with a 100% and a 150% display side by side.
- [ ] 49-day wrap: the idle calculation is correct across a simulated `GetTickCount` wrap.
- [ ] No UI string, doc line, or release note describes any feature as human-like, natural, or
      undetectable. Grep for it in CI.

---

## M5 — Ship (2 weeks)

**Scope:** signing · installer · updates · CI gates.

Referencing [FEATURES](FEATURES.md): D10, D11. See [UPDATES.md](UPDATES.md).

### Exit criteria

- [ ] NSIS installer, `installMode: currentUser` — installs and updates with **no UAC prompt**.
- [ ] Update flow end to end: background download, tray affordance, restart, new version
      running. Test from a genuinely older installed build, not a simulated one.
- [ ] `on_before_exit` releases the power request before the installer force-exits the app.
- [ ] Updater endpoint on our own domain, GitHub as fallback. **Test the 204 path** — a bug
      there silently freezes every user on their current version.
- [ ] Minisign private key backed up offline, in two places, verified restorable.
- [ ] Signed — [SignPath Foundation](https://signpath.org/) application submitted, and the repo
      satisfies its conditions: OSI license, MFA, published signing policy, reproducible build.
- [ ] Submitted to the Microsoft Defender false-positive portal **before** release.
- [ ] winget manifest.
- [ ] **CI budget gate**: launch the release binary, idle 10 min, sample working set and CPU
      time, fail the build on regression against the M0 numbers.
- [ ] Release checklist includes the **manual S0 test** — it cannot be automated.
- [ ] Download page carries a screenshot of the SmartScreen warning and an explanation.

---

## M6 — Extend (open-ended)

Ordered by ratio of demand to effort.

| | Feature | Why here |
|---|---|---|
| 1 | **CLI** (D10) | Makes the tool scriptable into build pipelines and agent hooks — the fastest-growing use case in the category, and cheap |
| 2 | **Move Mouse importer** (D9) | Removes the only real reason an existing user would not switch. Schema in [MOVE-MOUSE.md](MOVE-MOUSE.md) |
| 3 | **Load binding** (B4) | CPU and network thresholds. Only Don't Sleep has this on Windows |
| 4 | **Block shutdown** (A4) | Don't Sleep's differentiator. Time-bounded and overridable, or not at all |
| 5 | **Network binding** (B8) | Amphetamine has had SSID conditions for years |
| 6 | **Device binding** (B9) | Ditto, USB and Bluetooth |
| 7 | **Scripting** (D12) | Sandboxed Rhai conditions and a confirmed `RunCommand`. Not a DLL plugin ABI |
| 8 | **macOS** | See [CROSS-PLATFORM.md](CROSS-PLATFORM.md). Budget the Accessibility permission flow and the Apple tax |
| 9 | **Linux, power only** | The default engine ports cleanly to both X11 and Wayland. Ship it with input synthesis marked unavailable |

---

## Things that are never "later"

Five properties that are cheap while the codebase is small and expensive to retrofit. They are
not milestones; they are conditions on every commit.

1. **The platform trait boundary.** No `#[cfg(windows)]` outside `platform/`. Make it a CI lint.
2. **Power requests are released.** Every exit path, plus a panic hook, plus the test.
3. **Config migrations.** A `schema_version` and a migration chain from the very first release.
4. **No telemetry.** Not opt-in, not anonymous, not "just crash reports." None. A tool that
   watches your input must not phone home, and the moment there is a reporting path someone will
   want to extend it.
5. **Honest naming.** If a feature cannot be described accurately in a UI string, it does not
   ship — see [PRODUCT.md §5](PRODUCT.md#5-the-line).

## And one decision to make before M5

**The name.** `project-mouse` names the mechanism we have just decided is *not* the default, and
it lands in the cheap register. The binary name ends up in blocklists, winget manifests, and
`powercfg /requests` output, and changing it after release is expensive.
See [PRODUCT.md §9](PRODUCT.md#9-the-name).
