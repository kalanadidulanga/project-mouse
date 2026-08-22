# Cross-platform: the boundary, and what each OS can actually do

**Short answer: yes, but ~70% of it. And on Wayland, part of it is impossible by design.**

Tauri is cross-platform, so the shell, tray, packaging, updater, and the entire React UI come
free on macOS and Linux. The framework is not the problem.

The problem is that **the core value of this application is OS-specific input injection**, and
that is precisely the part no framework can abstract for you.

---

## 1. What ports and what does not

**One thing improves the odds considerably**, and it comes out of the mechanism split in
[PRODUCT.md §2](PRODUCT.md#2-the-three-mechanisms): the *default* engine — power inhibition —
ports cleanly everywhere. `IOPMAssertionCreateWithName` on macOS, the
`org.freedesktop.login1` and `org.freedesktop.ScreenSaver` D-Bus interfaces on Linux, and both
work identically under X11 and Wayland. [wakepy](https://wakepy.readthedocs.io/stable/) already
demonstrates the whole mapping in one library.

**It is only the opt-in input engine that hits the Wayland wall.** So a cross-platform build is
not all-or-nothing: it can ship with full power-management support and input synthesis marked
unavailable — which, given that power inhibition is the default and covers most use cases, is a
genuinely useful product rather than a broken one.

| Portable (~70%) | Per-OS (~30%) |
|---|---|
| Rule engine, evaluator, executor | Input injection |
| Scheduler and timing loop | Idle detection |
| Profiles, config, migrations | Keep-awake / sleep inhibition |
| Motion path generation, humanisation | Process enumeration |
| Logging, ring buffer, diagnostics | Foreground app identification |
| React UI, rule builder, log viewer | Fullscreen / presentation detection |
| IPC command surface | Autostart registration |
| Tray menu structure (Tauri) | Battery / CPU sampling |

The per-OS 30% is small in line count and large in difficulty. It is also the part that is
*impossible to retrofit a boundary around later* — by then it will be scattered through the
engine.

---

## 2. The boundary: define it now, implement it later

Cost of adding this on day one: an afternoon. Cost of adding it after the Windows
implementation is written: a rewrite of `core/`.

```rust
// platform/mod.rs

pub trait InputInjector: Send + Sync {
    fn move_relative(&self, dx: i32, dy: i32) -> Result<()>;
    fn move_absolute(&self, x: i32, y: i32) -> Result<()>;
    fn cursor_pos(&self) -> Result<(i32, i32)>;
    fn click(&self, button: Button, count: u8) -> Result<()>;
    fn scroll(&self, delta: i32) -> Result<()>;
    fn key(&self, key: Key) -> Result<()>;
}

pub trait IdleMonitor: Send + Sync {
    /// Milliseconds since the last input the OS observed — including our own.
    fn system_idle_ms(&self) -> u64;
}

pub trait PowerGuard: Send + Sync {
    fn set_keep_awake(&self, display: bool, system: bool) -> Result<()>;
    fn release(&self) -> Result<()>;
    fn battery(&self) -> Option<BatteryState>;
}

pub trait ProcessMonitor: Send + Sync {
    fn running_processes(&self) -> Result<Vec<ProcessInfo>>;
}

pub trait ForegroundMonitor: Send + Sync {
    fn foreground_app(&self) -> Result<Option<ProcessInfo>>;
    /// Fullscreen game / presentation / busy — whatever the OS can tell us.
    fn presentation_state(&self) -> PresentationState;
}

pub trait SessionMonitor: Send + Sync {
    fn is_locked(&self) -> bool;
    fn is_remote_session(&self) -> bool;
}

pub trait AutoStart: Send + Sync {
    fn is_enabled(&self) -> Result<bool>;
    fn set_enabled(&self, on: bool) -> Result<()>;
}

/// Everything the engine is allowed to know about the OS.
pub struct Platform {
    pub input:      Box<dyn InputInjector>,
    pub idle:       Box<dyn IdleMonitor>,
    pub power:      Box<dyn PowerGuard>,
    pub processes:  Box<dyn ProcessMonitor>,
    pub foreground: Box<dyn ForegroundMonitor>,
    pub session:    Box<dyn SessionMonitor>,
    pub autostart:  Box<dyn AutoStart>,
    pub caps:       Capabilities,
}
```

### `Capabilities` is not optional

Feature parity across these platforms is genuinely impossible, so the UI must be able to ask
rather than assume:

```rust
pub struct Capabilities {
    pub can_inject_input: bool,          // false on Wayland without a portal grant
    pub can_detect_idle: bool,
    pub can_prevent_display_sleep: bool,
    pub can_prevent_system_sleep: bool,
    pub can_enumerate_processes: bool,
    pub can_detect_foreground_app: bool, // false on Wayland
    pub can_detect_fullscreen: bool,
    pub can_autostart: bool,
    pub requires_permission_grant: bool, // macOS Accessibility, Wayland portal
}
```

The rule builder greys out conditions the platform cannot evaluate, with a tooltip explaining
why. A rule referencing an unsupported condition must fail loudly at load time, not silently
never fire — silent non-firing is the worst possible failure mode for an automation tool.

**A `MockPlatform` implementation falls out of this for free**, which is what makes the entire
rule engine unit-testable without touching an OS API. That alone justifies the boundary even
if the project stays Windows-only forever.

---

## 3. macOS — viable, with a permission wall

| Need | API |
|---|---|
| Input injection | `CGEventCreateMouseEvent` / `CGEventCreateKeyboardEvent` + `CGEventPost` (CoreGraphics) |
| Idle time | `CGEventSourceSecondsSinceLastEventType(kCGEventSourceStateHIDSystemState, kCGAnyInputEventType)` |
| Keep awake | `IOPMAssertionCreateWithName` — `kIOPMAssertionTypeNoDisplaySleep` / `PreventUserIdleSystemSleep` |
| Processes | `libproc` / the `sysinfo` crate |
| Foreground app | `NSWorkspace.frontmostApplication` |
| Fullscreen | `CGWindowListCopyWindowInfo`, or `NSApplicationPresentationOptions` |
| Autostart | `SMAppService` (13+), LaunchAgent plist below that |
| Lock state | `CGSessionCopyCurrentDictionary` → `kCGSSessionOnConsoleKey` |

**The blockers are not technical, they are distribution:**

1. **Accessibility permission.** `CGEventPost` does nothing until the user grants the app
   Accessibility access in System Settings → Privacy & Security. There is no programmatic
   grant. The onboarding flow must walk the user there and detect the grant afterwards
   (`AXIsProcessTrustedWithOptions`).
2. **Notarization is mandatory.** $99/yr Apple Developer membership, plus notarization on every
   release, or Gatekeeper blocks the app outright on first launch. There is no equivalent of
   "click through the SmartScreen warning".
3. **Permission resets.** The Accessibility grant is tied to the app's code signature. Every
   unsigned rebuild loses it, which makes local development annoying and makes user-side
   updates occasionally reset the grant.
4. **Idle semantics differ.** macOS treats display sleep and system idle differently from
   Windows, and a synthetic `CGEventPost` resets the HID idle timer — so the self-injection
   feedback loop from [ARCHITECTURE §6](ARCHITECTURE.md#6-the-self-injection-feedback-loop)
   exists here too, in the same form.

**Verdict: realistic.** Budget one to two weeks including the permission onboarding flow, and
$99/yr forever.

---

## 4. Linux — X11 is fine, Wayland is the wall

### X11 ✅

| Need | API |
|---|---|
| Input injection | XTEST — `XTestFakeMotionEvent`, `XTestFakeButtonEvent`, `XTestFakeKeyEvent` |
| Idle time | `XScreenSaverQueryInfo` → `XScreenSaverInfo.idle` |
| Keep awake | `org.freedesktop.ScreenSaver.Inhibit` (D-Bus), or `XResetScreenSaver` |
| Processes | `/proc` via the `sysinfo` crate |
| Foreground window | `_NET_ACTIVE_WINDOW` root property → `_NET_WM_PID` |
| Fullscreen | `_NET_WM_STATE_FULLSCREEN` |
| Autostart | `~/.config/autostart/project-mouse.desktop` |

Straightforward. Everything the Windows implementation does has a direct X11 equivalent.

### Wayland ❌ / ⚠️

**There is no portable way to inject synthetic input on Wayland. This is a security design
decision, not a missing feature.** The compositor owns the input pipeline and does not expose
it to arbitrary clients — precisely so that an application cannot do what this application
exists to do.

The available paths, none of them universal:

| Approach | Reality |
|---|---|
| **`libei` / `libeis` + XDG `RemoteDesktop` portal** | The sanctioned future. Requires a portal permission dialog per session; compositor support is uneven and still maturing. This is the path to bet on, but not one that works everywhere today. |
| **`ydotool` / `/dev/uinput`** | Works on any compositor because it injects at the kernel evdev layer. Requires the user to be in the `uinput` group or run a root daemon. Asking users to grant an input-injection tool device-level access is a hard sell, and rightly so. |
| **`zwlr_virtual_pointer_v1`** (wlroots) **+ `zwp_virtual_keyboard_v1`** (wayland-protocols-misc, *not* wlroots) | Works on sway, Hyprland, river. Neither is implemented by GNOME (Mutter) or KDE (KWin) — i.e. not on the majority of desktop Linux installs. |
| **`org_kde_kwin_fake_input`** | KDE's own compositor-specific injection protocol — this is how KDE Connect moves the cursor. KWin therefore *does* have a route, just not a portable one. |
| **XWayland** | Injects only into XWayland clients. Native Wayland windows never see the events. Not a solution. |

Idle detection is comparatively easy: `ext-idle-notify-v1`, or the
`org.freedesktop.ScreenSaver` / `org.gnome.Mutter.IdleMonitor` D-Bus interfaces.

Foreground-window identification is another casualty — there is no portable Wayland protocol
for "which app is focused", by the same design principle.

### The other Linux tax: the tray

`StatusNotifierItem` via `libayatana-appindicator`. Works on KDE, XFCE, Cinnamon. **GNOME
requires a third-party shell extension** — there is no tray on stock GNOME. For a tray-first
application, that is not a cosmetic issue; it is a "the app has no UI entry point" issue.

### Memory note

The Linux webview is WebKitGTK, whose footprint is worse than WebView2's and less predictable.
The lazy create/destroy discipline from [ARCHITECTURE §3](ARCHITECTURE.md#3-window-lifecycle--where-the-memory-budget-lives)
matters more here, not less.

**Verdict for input synthesis: X11 yes, Wayland experimental at best.** Wayland is the default
on current Fedora, Ubuntu, and both major desktop environments — so "Linux support" that only
means X11 is shrinking, not growing.

**Verdict for power inhibition: fine everywhere.** `org.freedesktop.login1.Manager.Inhibit`
(with `idle` / `sleep` / `shutdown` locks) and `org.freedesktop.ScreenSaver.Inhibit` work under
both X11 and Wayland, on GNOME and KDE alike. On Linux, the default mode of this application
would simply work.

That inverts the usual conclusion, and is worth stating clearly: the Linux port is not blocked.
Only half of it is — and it is the half that is off by default.

**The gap is real, too.** keep-presence, the most-starred cross-platform entry, is X11-only
"due to underlying library limitations," and the only Wayland option on the topic page is a
thirteen-star bash script. This tier of the category is genuinely unserved.

Note the *shape* of the Wayland problem: it is not that injection is impossible everywhere, it
is that **every available route is compositor-specific**. Supporting it properly means four
backends (libei/portal, KWin fake-input, wlroots virtual-pointer, uinput) with runtime detection
between them — which is why this is scoped as a research project rather than a port.

---

## 5. Support tiers — what to actually claim

Do not put "cross-platform" on the README until the Wayland question is settled. Claim this
instead:

| Tier | Platform | Meaning |
|---|---|---|
| **Tier 1** | Windows 10 1809+ / 11 | Every feature. CI-tested. Signed releases. |
| **Tier 2** | macOS 13+ | Feature parity minus Windows-specific niceties. Requires Accessibility grant. Notarized. |
| **Tier 3** | Linux / X11 | Feature parity minus session-lock triggers. Community-supported. |
| **Experimental** | Linux / Wayland | Idle detection and keep-awake work. Input injection requires `libei` portal support or a `uinput` grant. Feature-detected at runtime; unsupported rules are disabled with an explanation. |

An honest tier table is worth more than an optimistic badge. Users on Wayland who install this
expecting it to work and find nothing happens will file bugs that cannot be fixed, and leave
reviews saying it is broken.

---

## 6. Recommendation

1. **v1: Windows only.** Ship something excellent on one platform.
2. **Define the `Platform` traits from commit one.** It costs an afternoon, it makes the engine
   testable immediately, and it is the difference between "add macOS" and "rewrite the core".
3. **Keep `core/` free of `#[cfg(windows)]`.** If a conditional compilation attribute appears
   outside `platform/`, the boundary has leaked — treat it as a CI lint failure.
4. **macOS second**, once the Windows version has users. The work is well understood; the cost
   is the permission flow and the Apple tax.
5. **Linux last, and scoped honestly.** X11 as Tier 3. Revisit Wayland when `libei` has landed
   in Mutter and KWin — at that point it becomes a normal port instead of a research project.
