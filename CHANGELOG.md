# Changelog

## v0.2.0

The window became the control surface. v0.1.0 shipped an engine that understood far more than its
UI would let you say — eleven condition types reachable through one text box, and an input engine
whose interval was a constant in the source. That is fixed.

### Added

- **Rule builder covering every condition.** A process running · a time window · AC power · a
  battery level · the session being unlocked · the screen's state (presentation, fullscreen, game,
  quiet hours) · an app being in the foreground, or not being in it. Conditions stack on one rule
  (all must hold); separate rules combine by taking the strongest mode. Rules read as English
  sentences.
- **Timer.** Keep running or presenting for 15 m / 30 m / 1 h / 2 h / 4 h, with a countdown in the
  window and in the tray tooltip. It is built as a rule that expires, so it releases itself.
- **"Why is my PC awake?"** Two lines, kept separate on purpose: what project-mouse is holding
  (exact — we made the request), and what Windows will report to a program running without
  administrator rights (verbatim, labelled a hint rather than an inventory). Plus `powercfg
  /requests` as copyable text for the full list with names.

  Naming the program that holds a request requires administrator rights, and this app does not take
  them. Measured on Windows 11: `GetPowerRequestList` refuses an unelevated caller outright, and
  `powercfg /requests` will not run at all without an elevated prompt. Saying so plainly is the
  feature.
- **Profiles.** Several rule sets, switchable from the window or the tray menu.
- **First run.** One question, three answers, each creating a working profile in seconds. None of
  them synthesizes input.
- **Input engine settings**, previously hardcoded: how often, how long you must be idle first, and
  what to send — mouse movement or an F15 / Scroll Lock / Shift keypress.
- **Visible cursor movement.** Back and forth, around a square, or around a circle, with a
  configurable distance. Every path is closed: a full cycle returns the cursor to exactly where you
  left it. The default remains the virtual jiggle, which resets the idle timer without the cursor
  moving at all.
- **Variation.** Vary the interval and distance by a percentage, so this does not line up with other
  things running on a timer and the cursor does not land on the same pixel every time.
- **Remote-session detection.** The status page says when you are in an RDP or Citrix session.
- **An icon of its own**, replacing the framework's default.

### Fixed

- **Saving a profile destroyed the others.** The config write flattened the whole collection to
  whichever profile was loaded. Data loss the moment a second profile existed.
- **Memory did not come back after closing the window.** The working set was trimmed once at
  startup and never again, so it stayed at ~28 MB. Now: 2.2 MB with only the tray, 34.5 MB with the
  window open, 4.6 MB again shortly after closing it.
- **A config file with a UTF-8 byte-order mark read as corrupt.** Notepad and PowerShell both write
  one. The app correctly refused to overwrite a file it could not parse — but the file was fine.
- The window's permissions are narrowed to the two it actually uses.

### Notes

- Input synthesis remains **off by default**. Turning it on presents one honest paragraph about what
  changes. It stands down the moment you touch the machine, and it reports when Windows is silently
  discarding it.
- Not built: synthetic clicking. It is specified, but a synthetic click lands on whatever is under
  the cursor and can answer a dialog you left open, so it waits until it can be properly gated.

## v0.1.0

First release. Tray application, three modes (off / keep running / keep presenting) built on
`PowerCreateRequest`, a rule engine with process, schedule, expiry, power-source, session and
foreground conditions, the opt-in input engine with its self-injection filter and blocked
detection, a settings window, a CLI, a Move Mouse importer, and background updates.
