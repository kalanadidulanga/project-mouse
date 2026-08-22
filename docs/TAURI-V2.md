# Tauri v2 — the parts that matter here

Verified against the v2 docs, the config JSON schema, `docs.rs/tauri`, and the Tauri source
where the guide pages are silent — which turns out to be often. **Several of the most important
things in this document are not on the documentation site at all.**

---

## 0. Four things that contradict the obvious plan

### 1. `destroy()` freeing the webview is **undocumented**

The docs say only: `destroy()` — *"Similar to `close` but does not emit any events and force
close the window instead."* There is **no statement anywhere in Tauri's documentation about
memory or resource release on window destruction.** Each WebView2 window runs its own browser
and GPU processes, and whether destroying the last webview tears down the shared WebView2
environment is not documented either.

The entire ≤8 MB idle budget rests on this. **Prototype and measure it first** — before the
rule engine, before the UI. See [ARCHITECTURE §3](ARCHITECTURE.md#3-window-lifecycle--where-the-memory-budget-lives).

### 2. `tokio` cannot be removed

`crates/tauri/Cargo.toml` has an unconditional
`tokio = { version = "1", features = ["rt", "rt-multi-thread", "sync", "fs", "io-util"] }`.
No feature flag drops it.

What *is* controllable: Tauri stores the runtime in a `static OnceLock` and builds it lazily on
the first `spawn`/`block_on`. **Non-`async` commands run on the main thread and never touch it.**
So: make every command in this app synchronous, and no worker threads exist at idle. The updater
is the one async path — it materialises a *multi-threaded* runtime for the process lifetime.
Run update checks on a dedicated `std::thread` with a `current_thread` runtime instead.

### 3. `titleBarStyle` is macOS-only

The schema description is literally *"The style of the macOS title bar"*. On Windows the only
lever is `decorations: false` plus `data-tauri-drag-region`.

### 4. `set_activation_policy` does not exist on Windows

Both `App::` and `AppHandle::set_activation_policy` are `#[cfg(target_os = "macos")]`. On
Windows nothing is needed to stay out of the taskbar when zero windows exist; per-window there
is `"skipTaskbar": true`.

---

## 1. Starting with no window

`AppConfig.windows` **defaults to `[]`** — the window in every scaffolded template is a template
artifact, not a framework requirement.

Better than deleting it: keep the window declared with `"create": false`, so its geometry and
effects stay in config rather than being duplicated in Rust.

```jsonc
{ "app": { "windows": [ { "label": "main", "create": false, /* … */ } ] } }
```

```rust
// materialise it on demand, reusing the declared config
tauri::WebviewWindowBuilder::from_config(app.handle(), &app.config().app.windows[0])?.build()?;
```

---

## 2. Staying alive with zero windows

**This is the single most important API in the project and it appears only in the
plugin-authoring guide**, not on the tray page, the window page, or the config reference.

```rust
tauri::Builder::default()
  .setup(|app| { /* build tray */ Ok(()) })
  .build(tauri::generate_context!())?
  .run(|_app, event| {
      if let tauri::RunEvent::ExitRequested { api, code: None, .. } = event {
          api.prevent_exit();
      }
  });
```

### ⚠️ The `code: None` match is not optional

`RunEvent::ExitRequested { code: Option<i32>, api }`:

| `code` | Meaning | What we want |
|---|---|---|
| `None` | User closed the last window | **Prevent** — stay in the tray |
| `Some(0)` | Our own `app.exit(0)` from the tray Quit item | Allow |
| `Some(i32::MAX)` | `RESTART_EXIT_CODE` — updater restart | Allow |

Match unconditionally and **the app becomes unquittable.** The tray's own Quit menu item stops
working and the user has to reach for Task Manager — for an app that already looks like malware.

Tauri handles the third case itself: `prevent_exit()` is a documented no-op during a restart
(`if self.code != Some(RESTART_EXIT_CODE)`), so the updater is never blocked by this handler.

---

## 3. Tray icon

**`tray-icon` is not a default Cargo feature.** A build without it has no tray and no window —
i.e. it silently does nothing.

```toml
tauri = { version = "2", default-features = false, features = [
  "wry", "compression", "common-controls-v6", "tray-icon", "image-ico",
] }
```

(Windows default features include `x11` and `dbus`, which gate Linux runtime features — dead
weight here.)

```rust
use tauri::{menu::{Menu, MenuItem}, tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent}};

let tray = TrayIconBuilder::with_id("main")
    .icon(active_icon.clone())
    .tooltip("project-mouse — active · next in 42s")
    .menu(&menu)
    .show_menu_on_left_click(false)      // left = open window, right = menu
    .on_menu_event(|app, ev| match ev.id.as_ref() {
        "quit" => app.exit(0),           // Some(0) → allowed through
        _ => {}
    })
    .on_tray_icon_event(|tray, ev| {
        if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = ev {
            /* create the window here */
        }
    })
    .build(app)?;
app.manage(tray);                        // or retrieve later via app.tray_by_id("main")
```

Runtime state changes: `set_icon`, `set_tooltip`, `set_menu`, `set_visible`,
`set_show_menu_on_left_click`. Note **`set_title` is unsupported on Windows** and
`icon_as_template` is macOS-only.

⚠️ Prefer building the tray in Rust over `tauri.conf.json`'s `trayIcon.iconPath`. The schema
warns that the config path *"stores the image in raw pixels to the final binary… it's going to
bloat your final executable."*

⚠️ **The official system-tray example does the wrong thing for us.** It calls
`window.unminimize(); window.show(); window.set_focus();` — which assumes a resident hidden
window. Following it verbatim gives a permanently resident WebView2 tree and blows the budget.
We need the inverse: create on click, destroy on close.

---

## 4. Window lifecycle

| Call | Behaviour |
|---|---|
| `hide()` | Window and its WebView2 processes stay resident. **Never use this.** |
| `close()` | Emits `CloseRequested` first; interceptable. |
| `destroy()` | Force close, no events. **This is ours.** |

The X button already fires `CloseRequested` → window closes → last window gone →
`RunEvent::ExitRequested { code: None }` → we prevent exit. So the default path is already
correct; the explicit handler exists only to make sure nobody "fixes" it into a `hide()`:

```rust
.on_window_event(|window, event| {
    if let tauri::WindowEvent::CloseRequested { .. } = event {
        let _ = window.destroy();
    }
})
```

**`backgroundThrottling`** (`BackgroundThrottlingPolicy`) defaults to `suspend` — *"a web view
that's not in a window fully suspends tasks"*. Leave it unset. Never set `disabled`; that burns
CPU in a backgrounded webview.

**`noRedirectionBitmap`** (Windows-only) sets `WS_EX_NOREDIRECTIONBITMAP` and *"can avoid the
white flash that may appear before the webview content is rendered when using a transparent
window"* — relevant if we enable Mica, which requires transparency.

---

## 5. Native look on Windows

⚠️ **`windowEffects` requires `transparent: true`.**

| Effect | Support |
|---|---|
| `mica`, `micaDark`, `micaLight`, `tabbed*` | **Windows 11 only** |
| `acrylic` | Windows 10 v1903+ / 11 — *"bad performance when resizing/dragging"* on 10 v1903+ and 11 build 22000 |
| `blur` | Windows 7/10/11 22H1 — *"bad performance when resizing/dragging"* on 11 build 22621 |

So there is no single value that looks right on both Windows 10 and 11. Branch at runtime on
`tauri_plugin_os::version()`, or — given the perf warnings on resize and the fact that our
window is small and non-resizable — **just paint a flat solid background and skip effects
entirely.** A crisp opaque surface at 8 MB beats a translucent one that stutters. Revisit only
if the design genuinely calls for it.

`shadow` (default `true`) on Windows: *"`false` has no effect on decorated window… `true` will
make undecorated window have a 1px white border, and on Windows 11, rounded corners."*

---

## 6. Plugins — the minimal set

| Plugin | Verdict |
|---|---|
| `tauri-plugin-updater` | **Yes.** Core requirement. |
| `tauri-plugin-single-instance` | **Yes.** Must be registered *first* to work. |
| `tauri-plugin-autostart` | **Yes.** `init(MacosLauncher::LaunchAgent, Some(vec!["--minimized"]))` — the args vec is how the startup flag is passed. |
| `tauri-plugin-global-shortcut` | Optional (M2). Ideal for a windowless toggle. |
| `tauri-plugin-process` | **No.** Only needed if the *frontend* calls `relaunch()`. Rust has `app.exit()` / `app.restart()` already. |
| `tauri-plugin-store` | **No.** Its writes are async → instantiates tokio. A `serde_json` write to `app.path().app_config_dir()` is lighter and we already need atomic-write logic. |
| `tauri-plugin-log` | **No.** We use `tracing` directly. |
| `tauri-plugin-os` | Only if we branch on Windows version for effects — and §5 argues we shouldn't. |
| `tauri-plugin-window-state` | **No.** It saves state on app close, and this app never closes. |
| `tauri-plugin-notification` | M2, for error toasts. |

⚠️ **The single-instance doc example panics in this app.** It does
`app.get_webview_window("main").expect("no main window").set_focus()` — and there is usually no
main window. Use the callback to create the window, or just flash the tray.

---

## 7. Bundle configuration

```jsonc
{
  "build": { "removeUnusedCommands": true },
  "app": { "withGlobalTauri": false },
  "bundle": {
    "active": true,
    "targets": ["nsis"],
    "createUpdaterArtifacts": true,
    "windows": {
      "nsis": { "installMode": "currentUser", "compression": "lzma", "languages": ["English"] },
      "webviewInstallMode": { "type": "downloadBootstrapper", "silent": true }
    }
  }
}
```

**`targets: ["nsis"]`, not `"all"`.** MSI can only be built on Windows (WiX) and additionally
requires the VBSCRIPT optional Windows feature — the mysterious `failed to run light.exe`.

**`installMode: "currentUser"`** (the default) installs to `%LOCALAPPDATA%` with metadata under
`HKCU`. This is the **only mode where updates install without a UAC prompt**, and a tray utility
has no business asking for admin.

**`webviewInstallMode`** size impact:

| Type | Adds | |
|---|---|---|
| `downloadBootstrapper` | **0 MB** | Default. Use this. |
| `embedBootstrapper` | ~1.8 MB | |
| `offlineInstaller` | ~127 MB | |
| `fixedRuntime` | ~180 MB | |
| `skip` | 0 MB | ⚠️ App simply will not work without the runtime |

WebView2 ships with Windows 10 (April 2018+) and Windows 11, so the bootstrapper almost never
runs. Note the schema also says *"for the updater bundle `DownloadBootstrapper` is used"*
regardless of this setting.

**`withGlobalTauri: false`** (the default) — `true` injects the entire JS API onto
`window.__TAURI__`.

---

## 8. Size

Tauri's official release profile:

```toml
[profile.release]
codegen-units = 1
lto = true
opt-level = "s"   # "z" is smaller still — try both and measure
panic = "abort"
strip = true
```

**`removeUnusedCommands: true`** strips commands never allowed in the capability files. The
docs' own tip: *"To maximize the benefit of this, only include commands that you use in the ACL
instead of using `defaults`."* Requires `tauri@2.4` / `tauri-build@2.1` / `tauri-cli@2.4`, and
does not account for ACLs added dynamically at runtime.

---

## 9. Capabilities (ACL)

**Our own commands need no capability entry.** *"By default, all commands that you registered in
your app (using `tauri::Builder::invoke_handler`) are allowed to be used by all the windows and
webviews of the app."*

⚠️ **Security boundaries key on the window *label*, not the title.** A capability listing
`"windows": ["main"]` matches only a window whose label is `main`. Get it wrong and every
`invoke` fails at runtime — but only after the user opens the window, so it will not show up in
a smoke test.

⚠️ **Do not write `"core:default"`.** It pulls in nine permission sets and defeats
`removeUnusedCommands`. Enumerate:

```jsonc
// src-tauri/capabilities/main.json
{
  "identifier": "main-capability",
  "platforms": ["windows"],
  "windows": ["main"],
  "permissions": [
    "core:event:allow-listen",
    "core:event:allow-unlisten",
    "core:window:allow-close",
    "core:window:allow-start-dragging",
    "core:resources:allow-close"
  ]
}
```

`core:resources:allow-close` is needed because the JS `Update` object is a `Resource` with an
`rid`. If update checks are funnelled through our own `fetch_update` / `install_update` commands
(see [UPDATES.md](UPDATES.md)), `updater:default` is not needed at all — the frontend never
touches the plugin directly.

Restrict our own commands too, via `build.rs`, which also feeds `removeUnusedCommands`:

```rust
tauri_build::try_build(
    tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "get_state", "set_profile", "upsert_rule", "delete_rule",
            "set_rule_enabled", "trigger_rule_now", "pause_all", "resume_all",
            "get_logs", "get_diagnostics", "fetch_update", "install_update",
        ]),
    ),
).unwrap();
```

---

## 10. IPC

**Every command in this app is synchronous and non-`async`** — see §0.2. That is not a
stylistic preference; it is what keeps tokio dormant.

- *"Command names must be unique"* across the whole app, even across modules.
- Commands in `lib.rs` **cannot** be `pub`; commands in a separate module **must** be.
- `invoke_handler` can only be called once — the last call wins.
- Events *"directly evaluate JavaScript code so it might not be suitable to sending a large
  amount of data"*. Channels are ordered and fast — use `tauri::ipc::Channel<T>` for
  update-download progress.

At our payload sizes (a few hundred bytes of state) none of this is a bottleneck.
