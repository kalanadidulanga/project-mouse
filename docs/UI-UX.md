# UI / UX

## 0. The diagnosis

Move Mouse's radial launcher — a mouse illustration ringed by orbiting icons with an animated
arc — is the thing that "feels laggy sometimes". That is not a rendering bug to be optimised
away. It is structural, and it is worth naming precisely, because the same trap is easy to fall
into again with a nicer font.

**What is wrong with the radial menu:**

1. **It animates on the one path that must never be slow.** The launcher appears on tray
   interaction, when the user has already decided what they want. Any animation here is pure
   latency inserted between intent and action.
2. **It runs a continuous animation for a static value.** The green arc is a progress ring for
   "time until next action" — a number nobody watches, repainting forever in the background.
3. **Radial layouts are slower to hit than lists.** Fitts's law: circular arrangements give every
   target the same distance and none of them an edge or corner to slam into. A vertical list
   beats a ring for anything above about four items.
4. **The targets carry no labels.** A gear, a question mark, a PayPal logo, a bird. Every use is
   a small recall test.
5. **PayPal and Twitter sit in the primary interaction surface.** Donation and social links are
   legitimate for an open-source project. They are not primary actions, and putting them one
   slip away from Settings and Close costs the user something every single time.

**The corrected principle:**

> The best interface for a background utility is the one the user never opens.
> The second best is one that answers their question before they finish reading it.

---

## 1. Interaction model — three tiers

Almost all of Move Mouse's problems come from having exactly one surface for everything. Split
it by frequency:

| Tier | Surface | Frequency | Cost |
|---|---|---|---|
| **1. Glance** | Tray icon + tooltip | Constant | Free |
| **2. Act** | Native tray context menu | Several times a day | ~0 MB, instant |
| **3. Configure** | Webview settings window | Twice a week | ~130 MB, ~180 ms — and destroyed on close |

### Tier 1 — the tray icon answers the only question that matters

Four states, four distinct silhouettes. **Distinguishable at 16×16 in grayscale**, which is the
real constraint — a colour-only difference is invisible on a busy taskbar and useless to a
colourblind user.

| State | Icon | Tooltip |
|---|---|---|
| Keep presenting | Filled, bright | `Keeping display on — until 18:00` |
| Keep running | Filled | `Keeping awake — while msbuild.exe is running` |
| Off | Outline | `Not holding anything` |
| Auto-paused | Outline + dot | `Paused — a fullscreen app is running` |
| Blocked | Outline + warning | `Input blocked — an elevated window has focus` |

Note what the tooltip says: **not what the app is doing, but what is currently true of the
machine, and why.** "Active" tells the user nothing they could act on. "Keeping awake while
msbuild.exe is running" answers the question they actually have.

The **Blocked** state is the one nobody builds and everybody needs. Because `SendInput` fails
undetectably under UIPI ([WINDOWS-API gotcha 3](WINDOWS-API.md#gotcha-3--sendinput-fails-undetectably-under-uipi)),
the app must verify via cursor read-back and then *say so*. "It's running but nothing is
happening" is the most common support complaint about every tool in this category, and a tooltip
answers it for free.

### Tier 2 — the native menu is the actual product

```
┌────────────────────────────────────────────┐
│  ● Keeping awake                           │   ← non-interactive status line
│    while msbuild.exe is running            │
├────────────────────────────────────────────┤
│  ○  Off                                    │
│  ●  Keep running        system awake       │   ← the primary control:
│  ○  Keep presenting     + display on       │      three exclusive modes
├────────────────────────────────────────────┤
│  Keep awake until…                       ▸ │   → 1 hour / 4 hours / until 18:00
│  Keep awake while…                       ▸ │   → an app is running ▸ [pick]
├────────────────────────────────────────────┤
│  Profile: Long build                     ▸ │   → Long build ✓ / Presentation / …
├────────────────────────────────────────────┤
│  Why is my PC awake?                       │   ← opens the diagnostics panel
│  Settings…                                 │   ← the only other webview-creating item
│  About                                     │
├────────────────────────────────────────────┤
│  Quit                                      │
└────────────────────────────────────────────┘
```

**The three modes are the top-level control**, not a setting buried inside Settings. They are
what the product is, and the tray menu should read as the product.

Note what is *not* here: any mention of input synthesis. That is off by default, lives in
Settings behind an explicit choice, and once enabled appears in the status line rather than as
another top-level toggle.

This is a native Win32 menu. It costs no webview, opens in single-digit milliseconds, respects
the system theme automatically, and works with a screen reader without any effort on our part.

**Pause / Resume and profile switching are ~90% of all real interactions, and none of them
should ever open a window.** That single decision does more for perceived performance than any
amount of frontend optimisation — and it is exactly what the radial launcher gets wrong by
routing everything through an animated surface.

> Note what this does for the memory budget: a user who only ever pauses and switches profiles
> never creates a webview at all. The 8 MB figure is not a best case for such a user — it is
> their entire experience of the app.

### Tier 3 — the settings window

Created on demand, destroyed on close. Small, fixed size, not resizable, not maximisable.

---

## 2. Settings window layout

**640 × 480, fixed.** Not resizable. A settings window that can be dragged to 1920px wide is a
window whose layout you now have to defend at every width, for no benefit — nobody wants a
fullscreen mouse jiggler.

```
┌─────────────────────────────────────────────────────────────┐
│  project-mouse                                        ─  ✕  │
├───────────────┬─────────────────────────────────────────────┤
│               │                                             │
│  ● Status     │   ┌───────────────────────────────────┐    │
│    Rules      │   │                                   │    │
│    Profiles   │   │            Active                 │    │
│    Activity   │   │      next action in 42s           │    │
│    Settings   │   │                                   │    │
│               │   │        [  ●━━━  Pause  ]          │    │
│               │   │                                   │    │
│               │   └───────────────────────────────────┘    │
│               │                                             │
│               │   Profile      Office              ▾       │
│               │                                             │
│               │   Today        14 actions · 0 blocked      │
│               │   Memory       6.8 MB                      │
│               │                                             │
│  Office    ▾  │                                             │
└───────────────┴─────────────────────────────────────────────┘
```

Left rail, five items, always visible. No hamburger, no nested navigation, no tabs inside tabs.
Five destinations do not need a disclosure mechanism.

**The Status page is one sentence and one switch.** A user opening this window has one of two
questions: *is it working?* and *make it stop / start*. Both are answered above the fold with no
scrolling and no reading. Everything else on the page is secondary text.

Showing **Memory** on the status page is deliberate. The README makes a public promise about it;
the app should be willing to be checked against that promise, in the same place the user is
already looking.

---

## 3. The rule builder — plain language, not a node graph

This is the hardest screen and the one most likely to go wrong. The temptation is a visual
node-graph editor. Resist it: it is weeks of work, it is hard to make accessible, and it makes
simple rules *harder* to express than a sentence would.

Rules read as English sentences with the variables as inline dropdowns:

```
┌─────────────────────────────────────────────────────────────┐
│  Rules · Office                                    + New    │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  ●  Long build                                    ⋯   [ ●] │
│     Keep  ▸ running                                         │
│     While  ▸ msbuild.exe is running  ▸ on AC power          │
│                                                             │
│  ●  Monitoring wallboard                          ⋯   [ ●] │
│     Keep  ▸ presenting                                      │
│     While  ▸ weekdays 08:00–18:00                           │
│                                                             │
│  ○  Remote session                                ⋯   [● ] │
│     Keep  ▸ running                                         │
│     While  ▸ CitrixReceiver.exe is running                  │
│     Then  ▸ jiggle every 4–7 min   ⚠ synthesizes input      │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

Each `▸ chip` is a dropdown. Adding a condition appends a chip. The whole rule stays readable as
a sentence at every stage of editing, which means a user can verify their intent by reading it
back — the thing node graphs are worst at.

**`Keep` and `While` are the common case; `Then` is the exception.** Most rules hold a power
state under conditions and dispatch nothing at all. A rule that synthesizes input carries a
visible marker on the row itself — not a warning dialog, just an honest label, so that scanning
the list tells you which rules touch input and which do not.

**Every rule ships disabled-by-default when created from a template**, and the toggle is on the
right where the eye lands last. Automation that starts running the instant you finish typing is
how people end up with a cursor they did not ask for.

---

## 4. Motion budget

The complaint was lag. Here is the rule that prevents it coming back:

| Element | Allowed |
|---|---|
| Window open | None. Paint the final frame. |
| Tab / page switch | None, or ≤80 ms opacity |
| Toggle, checkbox | ≤120 ms |
| Dropdown, popover | ≤150 ms, opacity + 4px translate |
| Progress (update download) | Continuous — it represents real work |
| **Anything else** | **None** |

Hard rules:

- **Never animate `width`, `height`, `top`, or `left`.** `transform` and `opacity` only — they
  are the only two properties the compositor can handle without a layout pass.
- **No looping animation, ever.** No pulsing dots, no spinning gears, no progress ring counting
  down to the next action. A looping animation in a settings window is a repaint every frame for
  a value that changes once a minute. This is the specific thing that makes Move Mouse feel
  heavy.
- **Respect `prefers-reduced-motion`** — drop to zero across the board.
- The "next action in 42s" counter updates **once per second, as text**. No ring, no sweep.

The perceived-speed win is not in making animations faster. It is in not having them.

---

## 5. Visual language

**Follow Windows. Do not invent a look.**

A background utility that looks like a Windows utility is trusted. One with a custom gradient
theme and a mascot looks like something that was bundled with a driver download — which is a
serious problem for an app that is already fighting an AV-reputation battle.

```css
--font: "Segoe UI Variable Text", "Segoe UI", system-ui, sans-serif;
/* Segoe UI Variable ships with Windows 11 and needs no download.
   Segoe UI covers Windows 10. No webfont, no layout shift, no licence question. */

--radius: 4px;          /* Fluent's control radius. Not 16px. */
--space:  4px;          /* everything is a multiple */
```

- **Follow the system light/dark preference.** No in-app theme switcher in v1 — it is a setting
  nobody changes and a code path everybody has to maintain.
- **Accent colour: the user's Windows accent**, read from the system. Free personalisation, zero
  design decisions, and it makes the app look like it belongs.
- **One accent colour only.** Green for "active" and red for "error" are the only semantic
  colours. Everything else is greyscale.
- **No custom title bar in v1.** `decorations: true`. A custom title bar means reimplementing
  drag, snap layouts, and the Windows 11 maximise flyout — real work, zero user benefit, and it
  is the classic way an app starts feeling non-native.
- **No Mica or Acrylic in v1.** They require `transparent: true`, Mica is Windows 11-only, and
  the Tauri schema documents *"bad performance when resizing/dragging"* for Acrylic and Blur on
  several builds. A crisp opaque surface at 8 MB beats a translucent one that stutters. See
  [TAURI-V2 §5](TAURI-V2.md#5-native-look-on-windows).

### Density

Comfortable, not compact. This window opens twice a week — it is not a trading terminal. 32px
row heights, 16px gutters, generous whitespace. The content is thin; let it breathe rather than
inventing filler to justify a dense grid.

---

## 6. Where the donate and social links go

They belong in the project, not in the hot path. **About page only** — reached from the tray
menu or the left rail, never adjacent to Settings or Close.

An open-source project should absolutely ask for support. It should not put a PayPal button one
pixel-slip away from the control the user actually reached for, several times a day, for years.

---

## 7. Accessibility

Cheap to do now, expensive to retrofit, and this app has a real accessibility audience —
"prevent the screen locking while I read" is an assistive use case.

- **Full keyboard navigation.** Tab order follows visual order; every control reachable; visible
  focus rings that are not `outline: none`.
- **Screen reader labels** on every icon-only control. There should be very few of these.
- **Contrast ≥ 4.5:1** for text, ≥ 3:1 for UI boundaries.
- **Never colour alone.** The tray icon states differ in *shape*; status differs in *text*.
- **`prefers-reduced-motion`** honoured.
- **Windows High Contrast mode** — use system colour keywords rather than hardcoded hex where
  the platform provides them.

---

## 8. First run

The most important screen in the app, and the one most projects skip.

```
┌─────────────────────────────────────────────────────────────┐
│                                                             │
│              project-mouse is running                       │
│                                                             │
│         Look for the icon in your system tray.              │
│                    [tray icon, arrow]                       │
│                                                             │
│    What are you trying to do?                              │
│                                                             │
│      ◉  Finish a long job     Builds, renders, transfers    │
│                               Screen may sleep — work won't │
│      ○  Keep a screen up      Dashboards, presentations     │
│      ○  Let me set it up      Start from an empty profile   │
│                                                             │
│                                        [ Start ]            │
└─────────────────────────────────────────────────────────────┘
```

One question, three answers, each creating a working profile. The user is running within ten
seconds and never has to meet the rule builder unless they want to.

**Neither default enables input synthesis.** The first run of this application does not
synthesize a single event, and it should not need to explain that — it should simply be true,
and discoverable later by anyone who goes looking.

Then it closes — and **destroys itself**, dropping straight to the 8 MB steady state. The first
thing the user experiences is the thing the whole architecture is for.

---

## 9. The design test

Before adding anything to the UI, three questions:

1. **Does this need a window?** If it can live in the tray menu, it must.
2. **Does this move?** If yes, why — and does it represent real work in progress?
3. **Would this look out of place next to Task Manager?** If yes, it is wrong.
