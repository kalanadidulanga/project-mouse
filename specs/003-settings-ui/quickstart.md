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
2. Left-click the tray → the window opens. **MANUAL**: it should be interactive in well under a
   second; anything that feels like a beat is a regression worth timing properly.
3. Close it. **MANUAL**: in Task Manager, the `msedgewebview2.exe` children disappear and the
   app's private working set returns toward the tray-only baseline within ~30 s. If they linger,
   something replaced `destroy()` with `hide()` — that is the M0 finding the whole memory budget
   rests on.

## SC-002 — rule builder round-trips

1. Rules → build `Keep presenting while Mon–Fri between 09:00 and 18:00`.
2. Open `%APPDATA%\project-mouse\config.json` (or the portable path beside the exe) and read the
   `TimeWindow` condition — days array, `from: 540`, `to: 1080`.
3. Quit from the tray, relaunch, reopen Rules → the sentence is identical.

## SC-003 — "Why is my PC awake?"

The interesting cases are all reachable without admin:

| Setup | Expected panel text |
|---|---|
| Fresh boot, mode Off, nothing holding | "Nothing is holding this machine awake." |
| Set mode **Keep presenting** | Attributes it to us by name — not to "something". |
| Mode Off, but a video playing full-screen in a browser | "Something other than project-mouse is keeping your display on." |
| Any state | The `powercfg /requests` command is shown as copyable text, with the note that it needs an elevated prompt. |

**MANUAL cross-check**: open an elevated prompt, run `powercfg /requests`, and confirm the panel
was telling the truth about *whether* a request is held. It cannot name the holder — that is the
documented limit (research R1), not a bug.

**MANUAL, implementation gate (T004b)**: with the app holding **Keep running**, confirm the
aggregate read reflects our own `PowerSetRequest`. If it does not, the "and it isn't us"
subtraction is wrong and must be dropped — see research R1 "Open verification".

## SC-004 — idle clocks

Status shows System idle and Human idle. Leave the machine alone for two minutes: both climb
together. Move the mouse: both reset. With input synthesis **on** and the app jiggling, system
idle resets while **human idle keeps climbing** — that is the self-injection filter, and it is
the test that proves the tool can be interrupted at all.

## SC-005 — keyboard and High Contrast **MANUAL**

- Tab through every page. Every control takes focus, and focus is visible on every one. `rg -n "outline: *none" src` must find nothing.
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

## Profile switching (002 T017)

1. Create a second profile, add a rule to it, switch back to the first.
2. **The regression this guards**: reopen the second profile — its rule is still there. Before
   the fix in T014, saving flattened the collection to whichever profile the engine held and the
   other one was destroyed on the next write.
3. The tray submenu lists both and switching from it matches the UI.

## Tray tooltip expiry (002 T020)

Start a 15-minute timer on Status, then hover the tray icon: the tooltip names the mode and the
remaining time, and counts down. Cancel the timer → the tooltip drops back to the plain mode text.
