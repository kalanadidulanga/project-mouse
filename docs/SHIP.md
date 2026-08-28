# Shipping — M5 release checklist

What CI does automatically, and the steps a human must do (secrets, signing, hardware). Consolidates
ROADMAP M5 + UPDATES.md into one runnable list. Items marked **[human]** cannot be automated.

## Automated in CI (`.github/workflows/ci.yml`)

- `npm ci` + `npm run build` (frontend), `cargo fmt --check`, `cargo clippy`, `cargo test`.
- **Honesty gate:** grep fails the build if `undetectable` / `human-like` / `natural motion` appears
  in code or docs (PRODUCT §5, Test 3).
- **Boundary lint:** grep fails if `cfg(windows)` appears outside `platform/` (constitution IV).
- **Size gate:** release exe must be ≤ 8 MB.

## The two signatures — do not confuse them (UPDATES.md §1)

| | Update signing (minisign) | Code signing (Authenticode) |
|---|---|---|
| Stops | a MITM pushing a fake update | SmartScreen "unknown publisher" |
| Cost | free | free via SignPath Foundation |
| Lose the key | **every installed user is stranded forever** | buy a new cert |

## Release steps

1. **✅ DONE — update keypair generated + updater wired.** `src-tauri/pm-updater.key` (private,
   gitignored) + `pm-updater.key.pub`; the public key is already in `tauri.conf.json`
   `plugins.updater.pubkey`, `bundle.createUpdaterArtifacts: true`, and `tauri-plugin-updater` is
   wired in `lib.rs` (auto-check every 6 h that only hints; tray **Check for updates…** installs;
   `on_before_exit` releases the power request before Windows force-exits — UPDATES.md §4).
   - ⚠️ **[human] Back up `pm-updater.key` + its password offline, today.** Losing it means no
     installed user can ever be updated again. The password chosen at generation is a placeholder —
     regenerate with your own before public release if you like (`npm run tauri signer generate`).
   - **[human] Set ONE GitHub repo secret:** `TAURI_SIGNING_PRIVATE_KEY` = the **contents** of
     `pm-updater.key`. The key is password-less (`--ci`), and `release.yml` hardcodes an empty
     password — so no password secret is needed (avoids the password-mismatch class of failure).
   - ✅ endpoint set to `github.com/kalanadidulanga/project-mouse` in `tauri.conf.json`.
2. **[human] Publish the repo to GitHub** (GitHub Desktop → *Publish repository*, **public** — free
   Actions + free Releases CDN). Then tag `vX.Y.Z` → `release.yml` builds, signs, drafts a release
   with the installer + `latest.json`. Review the draft → publish. Installed apps update from there.
3. **[human, once] Authenticode / SmartScreen** — apply to [SignPath Foundation](https://signpath.org/)
   (free for OSS). The repo already meets its conditions: OSI license (MIT), MFA, reproducible build,
   published signing policy. Reputation accrues per file-hash over time — EV no longer buys a bypass.
4. **[human] Tag `vX.Y.Z`** → `release.yml` builds, signs the update artifacts, drafts the GitHub
   release, and pushes `latest.json` to the cPanel endpoint. Review the draft, then publish.
5. **[human] Submit** the signed installer to the
   [Microsoft Defender false-positive portal](https://www.microsoft.com/en-us/wdsi/filesubmission)
   **before** release; publish a winget manifest; publish the release SHA-256.
6. **[human] Download page** — show the SmartScreen warning screenshot + a one-line explanation, link
   the source, the SHA-256, and the build run (verifiability is the marketing).

## Release-checklist tests that CI cannot run

- **[human, hardware] S0 Modern Standby (SC-009, the flagship):** an S0 laptop, **Keep running**,
  lid closed, 8 h → still reachable and a running job completed. Compare with `SetThreadExecutionState`
  (which fails). The M0 spike (`spike-m0/`, `M0_AUTO=1`) validated the API path; this proves it on
  real hardware.
- **[human] Budget benchmark (SC-007):** idle 10 min → CPU ≤ 0.05 %, **private working set** ≤ 8 MB
  (measured private WS + `EmptyWorkingSet` trim — M0 measured 0.9 MB after trim). Needs an interactive
  desktop session, so it lives here rather than in CI.
- **[human] `powercfg /requests`** (elevated) shows our reason string while holding, and is clean
  after Quit and after `taskkill /F` (M0 T7/T8 verified the identical power path).

## Deferred refinements (not blockers)

Shipped since this list was written (v0.2.0): first-run wizard · E1 "why is my PC awake?" ·
profiles · visible movement (C2) · variation (C5) · Move Mouse importer.

Still deferred:

- **E1 cannot name the holder**, and that is not a gap to close — `GetPowerRequestList` refuses an
  unelevated caller and `powercfg /requests` will not run without an elevated prompt (measured;
  `specs/003-settings-ui/research.md` R1). Closing it would mean taking admin, which is a promise
  we do not break.
- **Click and scroll (C4).** Needs the foreground-application gate FEATURES requires before it is
  safe to ship — a synthetic click lands on whatever is under the cursor.
- **The name change** from `project-mouse` before first public release (PRODUCT §9). It names the
  mechanism that is deliberately *not* the default, and the binary name is what ends up in
  blocklists, winget manifests, and `powercfg /requests` output.
- macOS / Linux (M6).
