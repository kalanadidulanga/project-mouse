# Quickstart: validating M3

Runnable passes for the spec's success criteria. Anything that cannot be automated is marked
**MANUAL** and says what to look at — a milestone is done when *someone other than the author*
can verify it (constitution, Development Workflow).

## Prerequisites

```powershell
# from the repo root
npm install
cd src-tauri; cargo build; cd ..
```

## Automated

```powershell
cd src-tauri
cargo test                                  # engine logic, MockPlatform only, no Win32
cargo clippy --all-targets -- -D warnings
cargo fmt --check
cd ..
npx tsc --noEmit
npm run build
```

Plus the two CI gates, which are grep and must stay silent:

```powershell
# honest naming (constitution II)
rg -in "undetectable|human-like|looks human|natural motion" src src-tauri/src
# platform boundary (constitution IV)
rg -n "cfg\(windows\)" src-tauri/src | rg -v "src-tauri/src/platform/"
```

## SC-001 — window created on demand, destroyed on close

```powershell
npm run tauri dev
```

1. No window at startup; a tray icon only.
2. Left-click the tray → the window opens.
3. Close it, wait ~30 s.

**Measured 2026-08-28** (debug build, `sc001.ps1`): tray-only **2.2 MB** · window open **34.5 MB**
· settled after close **4.6 MB**. The settings window is genuinely destroyed — enumerating the
process's top-level windows afterwards leaves only `tray_icon_app`, `global_hotkey_app`, the
single-instance window and the IME default.

Two traps if you re-run this by hand:

- `FindWindow` and `Process.MainWindowHandle` both mislead here — the app owns several helper
  windows. Enumerate and match the title `project-mouse`.
- Any P/Invoke of `GetWindowTextW` **must** declare `CharSet.Unicode`. Without it the marshaller
  reads the wide title as ANSI and truncates `project-mouse` to `p`, and you will conclude the
  window is gone when it is not.

4.6 MB is 2.4 MB above baseline — outside M0's 2 MB tolerance, inside the 8 MB budget. The residue
is WebView2's, left in our own process after the children exit.

## SC-002 — rule builder round-trips

1. Rules → build `Keep presenting while Mon–Fri between 09:00 and 18:00`.
2. Open `%APPDATA%\project-mouse\config.json` (or the portable path beside the exe) and read the
   `TimeWindow` condition — days array, `from: 540`, `to: 1080`.
3. Quit from the tray, relaunch, reopen Rules → the sentence is identical.

## SC-003 — "Why is my PC awake?"

The panel states two things on separate lines and never merges them (research R1 / T014):

| Line | Source | Example |
|---|---|---|
| What we hold | our own engine state — exact | *"project-mouse is keeping this machine awake and the display on."* / *"project-mouse is holding nothing."* |
| What Windows reports | the `EXECUTION_STATE` aggregate, verbatim | *"Windows also reports a request on this machine to keep the system awake and keep the display on."* / *"Windows reports no other request it will show us."* / *"Windows would not tell us what else is holding a power request."* |

Below them, always: the caveat that line 2 does not name the program and does not cover every kind
of request, and `powercfg /requests` as copyable text labelled as needing an elevated prompt. The
panel never runs it.

**Verified 2026-08-28** on screen, with the first-run "Keep a screen up" profile active: both
lines rendered as above and the Copy button is present.

**MANUAL cross-check**: open an elevated prompt, run `powercfg /requests`, and confirm line 2 was
telling the truth about *whether* a request is held. It cannot name the holder — that is the
documented limit (research R1), not a bug.

## SC-004 — idle clocks

Status shows System idle and Human idle. Leave the machine alone for two minutes: both climb
together. Move the mouse: both reset. With input synthesis **on** and the app jiggling, system
idle resets while **human idle keeps climbing** — that is the self-injection filter, and it is
the test that proves the tool can be interrupted at all.

## SC-005 — keyboard and High Contrast **MANUAL**

- Tab through every page. Every control takes focus, and focus is visible on every one.
  `rg -n "outline: *none" src` must find nothing. **Verified 2026-08-28**: Tab from a cold
  first-run window focuses the first choice with a clearly visible ring (captured).
- Windows Settings → Accessibility → Contrast themes → apply one. Text stays legible, the status
  line and the switch stay distinguishable, no invisible-on-invisible.

## SC-006 — nothing animates at rest **MANUAL**

Leave the window open and idle. Nothing pulses, spins, or fades. The idle/memory readouts change
text every 2 s — text only, no layout thrash, no transition.

## SC-007 — first run

1. Move the config file aside: `mv $env:APPDATA\project-mouse\config.json $env:TEMP\`
2. Launch. The window opens on one question with three answers.
3. Pick one; a working profile exists in under 10 seconds.
4. **Confirm no input was synthesized**: Settings → Synthesize input is still **Off**, and
   Activity contains no injection lines.

**Verified 2026-08-28**: choosing "Keep a screen up" wrote a `keep-screen` profile with one
enabled `KeepPresenting` rule, set it active, left `input_enabled: false`, and the Status page
immediately read "Keeping the display on".

## Profile switching (002 T017)

1. Create a second profile, add a rule to it, switch back to the first.
2. **The regression this guards**: reopen the second profile — its rule is still there. Before
   the fix in T003, saving flattened the collection to whichever profile the engine held and the
   other one was destroyed on the next write. **Verified 2026-08-28** against the built binary:
   a three-profile config survives a save that changes the mode.

   Watch the encoding when hand-writing a config for this test. PowerShell's
   `Set-Content -Encoding utf8` writes a BOM; the app used to read that as corruption and disable
   saving, which makes the test *look* like it passed. Fixed, and pinned by a test — but use
   `[System.IO.File]::WriteAllText` with a BOM-less `UTF8Encoding` anyway.
3. The tray submenu lists both and switching from it matches the UI.

## Tray tooltip expiry (002 T020)

Start a 15-minute timer on Status, then hover the tray icon: the tooltip names the mode and the
remaining time, and counts down. Cancel the timer → the tooltip drops back to the plain mode text.
