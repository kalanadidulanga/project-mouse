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

1. **[human, once] Generate the update keypair** and back the private key up offline in two places:
   ```
   npm run tauri signer generate -- -w %USERPROFILE%\.tauri\project-mouse.key
   ```
   - Public key → `tauri.conf.json` `plugins.updater.pubkey` (commit it).
   - Private key → repo secret `TAURI_SIGNING_PRIVATE_KEY`; password → `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.
   - ⚠️ Losing this key means no existing user can ever be updated again. Back it up before shipping.
2. **[human, once] Wire the updater** in `tauri.conf.json`: `bundle.createUpdaterArtifacts: true`,
   `plugins.updater.endpoints` (own domain first, GitHub fallback), `windows.installMode: "passive"`.
   Enable `tauri-plugin-updater` (split `download()`/`install()`, `on_before_exit` releases the power
   request — UPDATES.md §4). Deferred until the key exists, since the pubkey must be baked in.
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

- First-run wizard (M3); the full "why is my PC awake?" list via `CallNtPowerInformation` (M3 E1);
  visible movement / click / randomisation (M4 C2/C4/C5); the name change from `project-mouse`
  before first public release (PRODUCT §9); Move Mouse importer + macOS/Linux (M6).
