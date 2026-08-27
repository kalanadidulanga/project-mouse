# Phase 1 Data Model: M3 leftovers

Only types that are **new** or whose **shape changes** are listed. Existing types (`Rule`,
`Condition`, `Profile`, `WakeMode`, `InputSettings`) are unchanged.

---

## `AwakeReport` — what E1 renders

Produced by `core::awake::report()`, a pure function over (machine aggregate, our own mode).
Serialized to the UI as-is.

| Field | Type | Meaning |
|---|---|---|
| `readable` | `bool` | `false` when Windows refused the read; the UI then says so instead of showing a confident "nothing is held" |
| `system_held` | `bool` | Something on this machine holds a system-required request |
| `display_held` | `bool` | Something holds a display-required request |
| `away_mode_held` | `bool` | Something holds an away-mode request |
| `ours` | `WakeMode` | What **we** are holding — known exactly, we made the request |

**Validation rule**: the `*_held` flags are Windows' aggregate reported **verbatim**. They are
never adjusted for `ours`, because whether our own `PowerSetRequest` appears in that aggregate
could not be verified — see [research.md R1, T014 result](./research.md). The two are reported
side by side and the UI lets the reader combine them; merging them would suppress a real
third-party request whenever we happened to hold the same kind.

`core/awake.rs` pins this with `our_own_request_never_alters_the_reported_aggregate`, so the
subtraction that was designed and cut cannot creep back in.

**Derivation** (the whole of the pure logic):

```
readable       = aggregate.is_some()
system_held    = aggregate & ES_SYSTEM_REQUIRED
display_held   = aggregate & ES_DISPLAY_REQUIRED
away_mode_held = aggregate & ES_AWAYMODE_REQUIRED
ours           = passed through unchanged
```

## `ProfileSummary` — the profile switcher

The engine holds exactly one `Profile`; the *collection* lives in config. The UI needs the list
without the rules.

| Field | Type | Meaning |
|---|---|---|
| `id` | `String` | Stable id, matches `Config.active_profile` |
| `name` | `String` | Display name |
| `active` | `bool` | Currently loaded into the engine |
| `rule_count` | `usize` | Shown on the switcher so an empty profile is visible as empty |

## `Snapshot.remote_session` — now populated

Already declared and `#[allow(dead_code)]`. R2 fills it from `SM_REMOTESESSION` in the sampler.
The `#[allow(dead_code)]` comes off. No new condition variant is added in this feature — the
field becomes readable in diagnostics; a `RemoteSession` condition is M6 (FEATURES B6).

## `Config` — no schema change

`schema_version` stays **2**. The collection is already `profiles: Vec<Profile>` with
`active_profile: String`; nothing on disk changes shape. What changes is that
`lib.rs::persist_current` stops overwriting the whole vector with the single profile the engine
holds (research R3) — a **write-path bug fix, not a migration**.

## State transitions

**Profile switch**: `set_profile(id)` → engine's current profile is written back into the
collection → the named profile is loaded → `active_profile = id` → persist → `state:changed`.
Writing back first is what makes switching non-destructive of unsaved rule edits.

**First run**: absent config file → UI opens on the first-run gate → one answer creates one
profile (power-only, rules **enabled**, since the user just asked for them) → persist → normal
UI. `input_enabled` is never touched; it stays `false` (SC-007).
