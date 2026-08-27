# Phase 1 Contract: the IPC surface this feature adds

Every command is **synchronous** (TAURI-V2 §0.2 — tokio stays dormant) and a thin wrapper over a
`core` call (constitution IV). Argument names are what the JS side passes.

## Existing (unchanged)

`get_state` · `set_mode` · `pause_all` · `resume_all` · `get_diagnostics` · `get_logs` ·
`get_rules` · `upsert_rule` · `delete_rule` · `set_rule_enabled` · `set_input_enabled` ·
`get_input_settings` · `set_input_settings` · `import_move_mouse`

## Added by this feature

| Command | Args | Returns | Notes |
|---|---|---|---|
| `why_awake` | — | `AwakeReport` | Never errors. A refused read returns `readable: false`, not an `Err` — the panel must render *something* truthful. |
| `list_profiles` | — | `Vec<ProfileSummary>` | Reads the config collection, with the engine's live profile reflected as `active`. |
| `set_profile` | `id: String` | `()` | Writes the current profile back into the collection **before** loading the named one. Unknown id is a no-op, logged. |
| `create_profile` | `name: String` | `String` (new id) | Creates empty, does **not** switch to it. |
| `delete_profile` | `id: String` | `()` | Refuses to delete the last remaining profile — the engine must always hold one. |
| `is_first_run` | — | `bool` | True when no config file existed at startup. Latched at startup, not re-read. |
| `complete_first_run` | `choice: String` | `()` | `"long_job" \| "keep_screen" \| "manual"`. Creates the matching profile and clears the latch. |

## `AwakeReport` wire shape

```json
{
  "readable": true,
  "system_held": false,
  "display_held": true,
  "away_mode_held": false,
  "ours": "Off"
}
```

The `*_held` flags are Windows' aggregate **verbatim** — never adjusted for `ours`. See
[research.md](../research.md) R1/T014 for why the adjustment was cut.

## The elevated-command affordance

The panel shows `powercfg /requests` as copyable text with the plain statement that it must be
run from an elevated prompt. **The app never runs it** — running it would spawn a process that
fails (measured: exit 1 unelevated, see research R1) and would flash a console window. It is
text for the user to copy, nothing more.

## First-run choices → profiles

| `choice` | Profile name | Rule created |
|---|---|---|
| `long_job` | "Long job" | `KeepRunning` while `ProcessRunning([])` — **empty list, disabled**, so the user fills in the process name. The rule is created so the shape is obvious; it holds nothing until named. |
| `keep_screen` | "Keep a screen up" | `KeepPresenting` with no conditions, **enabled** — the user asked for exactly this. |
| `manual` | "Default" | none — an empty profile; the user drives the mode buttons. |

None of the three enables input synthesis (SC-007).
