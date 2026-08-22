# M1 quickstart — build, run, verify

Prereqs: Rust (MSVC toolchain), Node + npm, WebView2 runtime (ships with Win10 1809+/11).

## Build & test

```powershell
npm install                       # once, for the frontend toolchain
npm run build                     # produce ../dist (embedded by generate_context)
cd src-tauri
cargo test                        # 18 unit tests, all on MockPlatform (no Win32)
cargo build --release             # -> src-tauri/target/release/project-mouse.exe (~3 MB)
```

## Run

```powershell
./target/release/project-mouse.exe
```

Look for the tray icon (no window opens — by design). Right/left-click it for the menu:
**Off · Keep running · Keep presenting · Start with Windows · Quit**. The tooltip states what is
currently true of the machine.

## Verify the exit criteria

### SC-001..SC-004 — modes + auditable + clean release  *(needs one elevated prompt)*

1. Click **Keep running**. In an **elevated** terminal:
   ```powershell
   powercfg /requests
   ```
   Expect our exe under **SYSTEM** and **EXECUTION** with the reason
   `project-mouse: Keep running ...`.
2. Click **Keep presenting** → `powercfg /requests` now also lists **DISPLAY**.
3. Click **Off** → those entries disappear.
4. Click **Keep running** again, then **Quit** from the tray → `powercfg /requests` is clean.
5. Repeat step 4 but `taskkill /F /IM project-mouse.exe` instead of Quit → still clean (the
   handle-scoped request dies with the process; a panic hook covers `panic=abort`).

### SC-005 — config safety

- Set a mode, then hard-kill the process during use. Relaunch → the mode is restored (config is
  written atomically: temp + fsync + rename).
- Corrupt the `config.json` (beside the exe, or `%APPDATA%\project-mouse\`) by hand → relaunch.
  The app logs an error, **keeps the broken file**, starts Off, and does not overwrite it.

### SC-006 — portable / no admin

- Copy just `project-mouse.exe` to a USB stick and run it (no installer, no admin). Config and
  `logs/` appear beside it. With Process Monitor, confirm **zero registry writes** until you
  toggle **Start with Windows** (which adds one `HKCU\...\Run` entry).

### SC-007 — footprint

- Idle in the tray for 10 min. Working set should settle low after the post-startup
  `EmptyWorkingSet` trim. NOTE: the ≤8 MB budget is **private working set** — measure it with
  VMMap (Sysinternals) or Task Manager's "Memory (active private working set)" column, not the
  plain working-set number. The precise CI budget gate lands in M5.

### SC-009 — Modern Standby (S0)  *(manual, hardware)*

On an S0 laptop: **Keep running**, close the lid, leave 8 h. The machine must still be reachable
and a running job must have completed. Compare against `SetThreadExecutionState` (which fails) —
this is the flagship correctness test and cannot be automated.
