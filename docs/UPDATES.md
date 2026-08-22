# Auto-update — VS Code style, at zero cost

**Yes, this is fully possible and completely free.** Tauri v2 ships an updater plugin; GitHub
Releases hosts the binaries on a CDN with no bandwidth cost for public repositories; GitHub
Actions builds and signs them with unlimited free minutes for public repos.

The one thing that is *not* free is Windows Authenticode signing — a separate concern from
update signing. See [README §Distribution reality check](../README.md#distribution-reality-check).
Updates work perfectly without it; they just carry a SmartScreen warning on first install.

---

## 1. Two different signatures — don't confuse them

This trips people up constantly, so before anything else:

| | Update signing | Code signing |
|---|---|---|
| **Tool** | `tauri signer` (minisign) | Authenticode / `signtool` |
| **Cost** | **Free** | Free via SignPath Foundation, else $150–300/yr |
| **Purpose** | Proves an update came from you — blocks a MITM pushing malware to installed users | Stops SmartScreen / "Unknown publisher" at install time |
| **Optional?** | **No.** Tauri cannot disable signature verification. | Technically yes, practically no |
| **Lose the key?** | **You can never ship an update to existing users again.** | Buy a new certificate |

Generate the update keypair once:

```bash
npm run tauri signer generate -- -w ~/.tauri/project-mouse.key
```

- **Public key** → `tauri.conf.json` → `plugins.updater.pubkey`. Safe to commit. Must be the key
  *content*, not a path.
- **Private key** → GitHub Secrets as `TAURI_SIGNING_PRIVATE_KEY`, password as
  `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.

> ⚠️ **Back the private key up somewhere offline, today.** Losing it does not mean "regenerate
> and carry on" — every already-installed copy verifies against the old public key baked into
> its binary. Losing it strands every existing user permanently, with no recovery path other
> than asking them all to manually reinstall.

> ⚠️ `.env` files **do not work** for these variables. They must be real environment variables.

---

## 2. Where to host — and the decision that outlives everything

There are two separate questions here, and conflating them is the mistake.

### The binaries → **GitHub Releases**

Free, CDN-backed, no bandwidth charge for public repos, and `tauri-action` uploads them
automatically. Nothing else competes.

### The updater endpoint → **your own domain**

The `endpoints` array in `tauri.conf.json` is **compiled into every binary you ship**. Every
copy of v1.0.0 that anyone ever installs will ask that exact URL for updates, forever.

If that URL is `https://github.com/kayd/project-mouse/releases/latest/download/latest.json` and
you later rename the repo, move to a GitHub organisation, get rate-limited, or leave GitHub
entirely — **every installed copy silently stops updating and there is no way to fix it
remotely.** The only recovery is asking users to manually reinstall.

The shared cPanel host at `kalanadidulanga.com` solves this properly. It already has a valid
Let's Encrypt certificate, which matters: **Tauri enforces TLS on updater endpoints in
production builds.**

```jsonc
"endpoints": [
  "https://kalanadidulanga.com/project-mouse/update/{{target}}/{{arch}}/{{current_version}}",
  "https://github.com/kayd/project-mouse/releases/latest/download/latest.json"
]
```

Own endpoint first, GitHub as the fallback. Tauri moves to the next URL on any non-2XX response,
so if the shared host is down, updates still work.

⚠️ **One exception to that fallback:** a `204 No Content` stops the loop immediately and is
interpreted as "no update available". That is the correct fast path for "you're up to date" —
but it means a bug that returns 204 unconditionally will silently freeze every user on their
current version, and the GitHub fallback will never be consulted. Test the 204 path explicitly.

### What the cPanel host should **not** do

**Do not host the installer binaries there.** A shared hosting account is not a CDN:

- The account is at **194,279 / 300,000 files** — 65% of the inode limit. Release artifacts
  accumulate.
- "Unlimited" bandwidth on shared hosting is governed by a fair-use clause, and using it as a
  binary distribution point is exactly what that clause exists to stop. A 6 MB installer at any
  real download volume gets throttled or the account suspended.
- Single server, no edge locations. Users far from it get slow downloads; GitHub's CDN does not
  have that problem.

Serve JSON (a few hundred bytes) from cPanel. Serve binaries from GitHub. That split costs
nothing, survives a GitHub migration, and stays inside the shared host's terms.

### What you gain by owning the endpoint

Beyond survivability — a static file could not do any of this:

- **Staged rollouts.** Serve the new version to 10% of checks first, watch the issue tracker,
  then open the gate. Invaluable for a tool that injects input into people's machines.
- **A kill switch.** If a release turns out to break something badly, stop serving it in
  seconds instead of waiting for users to notice.
- **Real adoption data.** Which versions are actually in the wild, from the check requests.
- **Per-version and per-arch responses**, using the `{{current_version}}`, `{{target}}`, and
  `{{arch}}` variables in the URL.

### The endpoint

```php
<?php
// public_html/project-mouse/update/index.php  (routed via .htaccess)
header('Content-Type: application/json');

$current = $_GET['version'] ?? '0.0.0';
$target  = $_GET['target']  ?? 'windows';
$arch    = $_GET['arch']    ?? 'x86_64';

$manifest = json_decode(file_get_contents(__DIR__ . '/latest.json'), true);

if (version_compare($current, $manifest['version'], '>=')) {
    http_response_code(204);   // up to date — Tauri stops here
    exit;
}

$key = "$target-$arch";
if (!isset($manifest['platforms'][$key])) { http_response_code(204); exit; }

echo json_encode([
    'version'   => $manifest['version'],
    'notes'     => $manifest['notes'],
    'pub_date'  => $manifest['pub_date'],
    'url'       => $manifest['platforms'][$key]['url'],        // → GitHub Releases
    'signature' => $manifest['platforms'][$key]['signature'],
]);
```

`latest.json` is refreshed by a step in the release workflow (or a cron job that pulls the
latest release from the GitHub API — cPanel has Cron Jobs, and a 15-minute poll is plenty).

⚠️ **Keep the TLS certificate alive.** If Let's Encrypt renewal fails, every update check in the
world fails silently, because Tauri refuses non-TLS endpoints in production. Monitor it.

---

## 3. `tauri.conf.json`

```jsonc
{
  "bundle": {
    "createUpdaterArtifacts": true,     // note: under `bundle`, not `plugins.updater`
    "targets": ["nsis"],
    "windows": {
      "nsis": { "installMode": "currentUser" }
    }
  },
  "plugins": {
    "updater": {
      "pubkey": "dW50cnVzdGVkIGNvbW1lbnQ6...",
      "endpoints": [
        "https://kalanadidulanga.com/project-mouse/update/{{target}}/{{arch}}/{{current_version}}",
        "https://github.com/kayd/project-mouse/releases/latest/download/latest.json"
      ],
      "windows": { "installMode": "passive" }
    }
  }
}
```

**`createUpdaterArtifacts: true`** makes the bundler emit `project-mouse-setup.exe` **and**
`project-mouse-setup.exe.sig`. The installer *is* the update payload — no zip wrapper. (The
`"v1Compatible"` value produces the old `.nsis.zip` form; it will be removed in Tauri v3.)

**`nsis.installMode: "currentUser"`** installs into `%LOCALAPPDATA%`. This is what lets updates
install **without a UAC prompt**. `perMachine` means every single update pops a consent dialog —
fatal for a background utility.

**`updater.windows.installMode: "passive"`** shows a small progress window with no interaction.
`"quiet"` is fully silent but *"the installer cannot request admin privileges by itself so it
only works in user-wide installations"* — which, with `currentUser`, is exactly our case. Start
at `passive`; move to `quiet` once it is proven.

---

## 4. The VS Code UX

VS Code downloads in the background and then shows a small "Restart to update" affordance. That
is precisely what Tauri's **split** `download()` / `install()` API is for. Do **not** use
`downloadAndInstall()` — that is the one-shot version and it gives the user no choice about when
their session is interrupted.

```
App start + every 6 h
    │
    ├─ check()  ────────────────── no update → 204, done. Cost: one request.
    │
    ├─ update found
    ├─ download(onProgress)  ────── silent, background, no UI
    │
    ├─ downloaded
    ├─ tray icon → "update ready" variant
    ├─ tray tooltip → "project-mouse — update ready (v1.2.0)"
    ├─ tray menu gains: "Restart to update v1.2.0"
    └─ settings window shows a quiet banner if it happens to be open
         │
    User clicks (whenever they feel like it)
         │
         ├─ on_before_exit → stop the scheduler, release the power request, save config
         ├─ install()   ← Windows force-exits the app here
         └─ NSIS installer runs, relaunches the app (restartAfterInstall defaults to true)
```

For a tray utility this is better than VS Code's own flow: **there is no session to lose.** The
restart is invisible — the tray icon blinks and comes back on the new version.

### The state must live in Rust

The window can be destroyed at any moment, so the pending update cannot live in React. Follow
the pattern the updater docs recommend:

```rust
struct PendingUpdate(Mutex<Option<Update>>);

#[tauri::command]
fn install_update(app: AppHandle, pending: State<'_, PendingUpdate>) -> Result<(), String> {
    let update = pending.0.lock().unwrap().take().ok_or("no update pending")?;
    update.install().map_err(|e| e.to_string())
}
```

Progress goes over a `tauri::ipc::Channel<DownloadEvent>` rather than events — channels are
ordered and are what the plugin uses internally for exactly this.

### ⚠️ Windows force-exits on install

Stated twice in the docs: *"On Windows the application is automatically exited when the install
step is executed due to a limitation of Windows installers."* There is no graceful shutdown
window. Hook it:

```rust
app.updater_builder()
   .on_before_exit(|| {
       // release the power request — otherwise the machine cannot sleep
       // until the new version starts and re-establishes it
       scheduler::shutdown();
   })
   .build()?
```

That power-request release is not hypothetical: skip it and a failed update leaves a machine
that will not sleep, with no app running to explain why. See
[WINDOWS-API gotcha 12](WINDOWS-API.md#gotcha-12--release-keep-awake-on-exit).

`restartAfterInstall` defaults to `true`, so **the `process` plugin's `relaunch()` is not
needed on Windows** — the installer brings the app back itself.

---

## 5. The release workflow

```yaml
# .github/workflows/release.yml
name: release
on:
  push:
    tags: ['v*']

jobs:
  release:
    permissions:
      contents: write
    runs-on: windows-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with: { node-version: 20, cache: npm }
      - uses: dtolnay/rust-toolchain@stable
      - uses: swatinem/rust-cache@v2
        with: { workspaces: './src-tauri -> target' }

      - run: npm ci
      - uses: tauri-apps/tauri-action@v0
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
          TAURI_SIGNING_PRIVATE_KEY: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY }}
          TAURI_SIGNING_PRIVATE_KEY_PASSWORD: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY_PASSWORD }}
        with:
          tagName: v__VERSION__
          releaseName: 'project-mouse v__VERSION__'
          releaseBody: 'See CHANGELOG.md'
          releaseDraft: true       # review before it goes live
          prerelease: false

      # push the manifest to the cPanel endpoint
      - name: Publish update manifest
        run: |
          curl -f -X POST "https://kalanadidulanga.com/project-mouse/update/publish.php" \
               -H "Authorization: Bearer ${{ secrets.MANIFEST_TOKEN }}" \
               -H "Content-Type: application/json" \
               --data-binary "@latest.json"
```

`tauri-action` builds, signs, creates the GitHub release, uploads the installer and its `.sig`,
and generates `latest.json`. `__VERSION__` is substituted from `tauri.conf.json`.

`releaseDraft: true` is worth keeping: the artifacts are built and uploaded, but nothing reaches
users until you press publish. For a tool that injects input into people's machines, a manual
gate before every release is proportionate.

**Cost: zero.** Public repositories get unlimited GitHub Actions minutes, and Release asset
bandwidth is not billed.

### `latest.json`

```json
{
  "version": "1.2.0",
  "notes": "Fixed multi-monitor absolute positioning",
  "pub_date": "2026-08-21T10:00:00Z",
  "platforms": {
    "windows-x86_64": {
      "signature": "dW50cnVzdGVkIGNvbW1lbnQ6...",
      "url": "https://github.com/kayd/project-mouse/releases/download/v1.2.0/project-mouse-setup.exe"
    }
  }
}
```

⚠️ `signature` is the **content** of the `.sig` file. A path or URL does not work.

⚠️ *"Tauri will validate the whole file before checking the version field"* — so one malformed
platform entry breaks updates for **every** platform, not just that one.

---

## 6. Check cadence

Once on startup, then every 6 hours. Not more:

- A no-update check is a single request returning 204 — but it is still a network call from a
  process that claims to be invisible.
- A user who leaves this running for weeks (the entire point of the app) would generate a lot of
  requests at a shorter interval, on shared hosting.
- Add jitter (±30 min) so a popular release does not produce a synchronised thundering herd
  against the cPanel endpoint at the top of every hour.
- Never check on a metered connection or below 20% battery — reuse the guards from
  [FEATURES §B4](FEATURES.md).

Settings should include **"Check for updates automatically"** (default on) and a manual
**"Check now"** button. Some users will run this on locked-down machines where outbound requests
are noticed, and they deserve the switch.

---

## 7. The portfolio download page

The page at `kalanadidulanga.com` is the front door; GitHub is the file store. The download
button links directly to the GitHub Releases asset:

```
https://github.com/kayd/project-mouse/releases/latest/download/project-mouse-setup.exe
```

That URL always resolves to the newest release — no page edit per version. A small script can
fill in the version number and date from the GitHub API at build time, or client-side.

**Put the SmartScreen warning on the page itself**, above the fold, with a screenshot of what
the user will see and a one-line explanation. Users who hit an unexplained "Windows protected
your PC" dialog assume malware and leave. Users who were told to expect it click through. This
single paragraph is worth more than most of the marketing copy on the page.

Also link, from the download button's immediate vicinity:

- the source repository,
- the release's SHA-256 checksum,
- the build workflow run that produced the binary.

For a tool in this category, verifiability *is* the marketing.
