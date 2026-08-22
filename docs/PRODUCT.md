# What this product actually is

> Read this before anything else. It changes what the software does, not just how it is described.

---

## 1. The mistake we were making

Early design here was built around one sentence: *"keep Microsoft Teams showing Available."*

That framing is wrong on three counts.

1. **It is factually not what the incumbent does.** The Move Mouse wiki — all nine pages — never mentions Teams, Skype, Lync, or presence. Not once. Its own stated most-common use case is *"keep remote sessions alive whilst working from home"*, naming Citrix Receiver, VMware Horizon, and AWS WorkSpaces. That is a **session timeout** problem, not a status-indicator problem.
2. **It is a small slice of the market and all of the reputational risk.** Presence is perhaps a quarter of demand by volume and close to all of the exposure. It is also the one segment where a competitor can beat us on features we should not build.
3. **It made us design the wrong machine.** Aiming at presence meant assuming synthetic input injection is *the* mechanism. It is one of three, it is the riskiest, and it is unnecessary for most of the people who need this software.

---

## 2. The three mechanisms

This is the central insight of the whole project. These get conflated constantly — by users, by competitors, and by our own early drafts.

| | **(A) Power inhibition** | **(B) Input synthesis** | **(C) Hardware HID** |
|---|---|---|---|
| **How** | Declares intent to the OS: `PowerCreateRequest` / `PowerSetRequest` (or the older `SetThreadExecutionState`) | Fakes an HID event: `SendInput` | A USB device enumerates as a real mouse |
| **Defeats** | Sleep, hibernate, display-off | All of (A), **plus** interactive idle timers: screen lock, chat presence, Citrix/RDP idle disconnect, in-app "are you there" timers | Everything, including on machines where you cannot install software |
| **Does not defeat** | Screen lock, session idle timers, presence, RDP disconnect | The lock screen once already locked; kernel anti-cheat | Nothing at the OS layer — but USB enumeration is still logged |
| **Policy risk** | **~Zero.** This is the sanctioned API. | **High.** This is what gets flagged as PUA and what people get fired over. | Bypasses software allowlists entirely |
| **Our stance** | **Default. Always available.** | **Opt-in, off by default, labelled honestly.** | Not our market — see §7 |

**Almost every legitimate use case in §3 is satisfied by (A) alone.** The cases that genuinely need (B) are a smaller set, and they are session and presence timers rather than power timers.

Move Mouse has **no power API integration whatsoever**. It is 100% mechanism (B) — every feature hangs off synthetic input resetting `GetLastInputInfo`. That is also the source of its own documented number-one support complaint:

> *"By far the most common complaint I get from users is 'Move Mouse is running, but my computer still went to sleep.'"*
> — Move Mouse wiki, Troubleshooting

That complaint is unfixable inside its architecture, and it is a free win for anything built the right way round.

### The two modes worth stealing

The Python library [wakepy](https://wakepy.readthedocs.io/stable/) has the cleanest conceptual model in the entire category, and it is better product design than anything currently shipping on Windows:

| Mode | Keeps | Allows | For |
|---|---|---|---|
| **Keep running** | System awake, work continuing | Screen may dim, blank, and lock | Builds, renders, training runs, transfers, backups |
| **Keep presenting** | System awake **and** display lit and unlocked | Nothing | Dashboards, wallboards, presentations, kiosks, reading |

Two independent, independently-persisted states. This single distinction resolves entire categories of bug report in competing tools, where users complain in *both* directions — "it won't let my screen sleep" and "my screen slept when I told it not to" — because those tools have one toggle where there should be two.

---

## 3. Who actually needs this

Every entry below is from a primary source — an issue tracker, a vendor doc, or a practitioner forum thread. Sources at the end.

### Development, CI, and — new — AI agents

- **Self-hosted CI runners and build agents on desktop hardware.** The machine sleeps overnight and the agent drops offline. The OS default fails for a precise reason worth internalising: **Windows idle detection is driven by user input, not CPU load.** A machine can sit at 100% CPU and still sleep.
- **AI coding agents running unattended.** This framing did not exist three years ago and is now the fastest-growing one. [Insomnia](https://stanley-projects.github.io/Insomnia/) is positioned entirely on it — hooks into Claude Code and Cursor so the machine is "awake only while something needs it." PowerToys' own Modern Standby issue lists "Remote-access sessions (RDP, SSH, AI agents) that must stay reachable" as a driver.
- **Process-lifetime binding is the specific unserved primitive.** PowerToys [#27980](https://github.com/microsoft/PowerToys/issues/27980) asks for `--process-name` rather than `--pid`, because the user's sync tool respawns itself under a new PID. [#44512](https://github.com/microsoft/powertoys/issues/44512) asks for the same with a UI. **Both still open.**

### Data, ML, and large transfers

- Overnight training runs — exactly *keep running* semantics: keep computing, let the screen go dark.
- Large transfers and backups. Windows sleeps mid-copy because a network transfer does not itself assert a power request.
- **Remote astrophotography**, which nobody would have predicted: a telescope PC captures via NINA while a processing PC receives images over LAN. The processing PC slept mid-transfer and killed the session. Note their actual requirement — awake *at night only*, asleep during the day. They ended up scripting `powercfg.exe` into sequencer templates because no tool did it.

### Media production — and the shape of the whole category

The [Adobe Media Encoder bug report](https://community.adobe.com/t5/adobe-media-encoder-beta-bugs/computer-sleeps-mid-render/idi-p/12970399) contains the canonical diagnosis: *"AME should alert the OS that it's busy and prevent sleep until the last job in the queue is finished."*

**That is the entire category in one sentence.** Third-party tools exist because first-party applications forget to call the power API. We are the bolt-on for every app that should have asserted a wake lock and didn't.

The inverse also exists and matters: OBS blocks sleep *whenever it is open*, not only while recording, and there are multiple forum threads asking for the opposite behaviour. Both directions of the complaint live in the same forum.

### Sysadmin, NOC, and remote sessions

- **Dashboards and wallboards.** PowerToys [#42720](https://github.com/microsoft/PowerToys/issues/42720) states it best: *"I have dashboards and automations on my system that I actively monitor during office hours… I don't require monitoring off-hours and would rather set & forget a schedule."* Note the energy-consciousness — users do not want *always on*, they want *conditionally on*.
- **Remote sessions — the single largest legitimate cluster.** Citrix, VMware Horizon, AWS WorkSpaces, RDP. Critically, **these are session timers, so mechanism (A) does not solve them.** The mRemoteNG maintainers closed [#405](https://github.com/mRemoteNG/mRemoteNG/issues/405) — "simulate keyboard or mouse move to avoid RDP session idle timeouts" — as *Cannot Fix / Vendor Upstream*, leaving a hole standalone tools fill.
- **Long unattended maintenance.** A repair technician running virus scans in Safe Mode on customer machines, where power settings cannot be changed and re-authenticating every few minutes is untenable.

### Labs and instrument control

The [NI LabVIEW forum thread](https://forums.ni.com/t5/LabVIEW/Preventing-PCs-from-sleeping-or-hibernating/td-p/3266364) is the best primary source in this whole report, for two reasons:

1. *"The DAQ crashes and it's a pain to recover from"* — sleep does not merely pause acquisition, it **corrupts the hardware session**.
2. A participant describes a **20-minute medical test running unattended while monitoring a patient**, where an unexpected lock is a safety issue.
3. *"My company enforces a 'Power Saver' profile with no user override rights"* — the policy-locked machine, in its purest form.

And the thread's own conclusion is instructive: someone suggested "just move the mouse," and the consensus **rejected it** in favour of wrapping the Windows Power Requests API properly. Practitioners prefer (A) when (A) suffices. So should we.

### Kiosks, signage, retail, manufacturing terminals

Harder than it looks, which is itself the opportunity: to keep a kiosk up you must independently disable display-off, sleep, password-on-wake, **and** the `Interactive logon: Machine inactivity limit` security policy. That is four settings across three policy trees. Miss one and the kiosk shows a login prompt to the public.

A single tool that asserts the correct state and *reports what it is doing* is worth more than four correctly-set policies nobody can audit.

### Presentations and classrooms

The least-stigmatised use case, cited by every tool including Microsoft's own docs. But note the **counter-requirement**, which is a feature in itself: Move Mouse [#97](https://github.com/sw3103/movemouse/issues/97) asks the tool to **suspend itself during presentation mode or screen sharing**, because the audience can see the cursor twitching.

So the same user, in the same hour, needs both "keep the screen on" and "absolutely do not synthesize input." One toggle cannot express that. Two mechanisms can.

### Accessibility — the strongest framing available

Genuinely under-served, and nobody in this category markets to it.

WCAG 2.2.1 (Timing Adjustable, Level A) requires that users can turn off, adjust, or extend time limits. The same principle applies to OS session timeouts. Institutional policy already carves out the door: the [University of Northern Iowa screen-lock policy](https://uniservicehub.atlassian.net/wiki/spaces/SH/pages/207659145/Screen+Lock+Out+Times+-+15+Minute+Maximum) mandates a 15-minute maximum citing NIST 800-53, PCI DSS, HIPAA and CIS Benchmarks — **with exceptions "only for documented accessibility needs."**

Who this covers: people who read slowly; people using screen magnifiers or screen readers, who spend long periods on one screen generating no input events; people with motor impairments for whom re-authenticating every fifteen minutes is a real cost; people with cognitive disabilities for whom an unexpected lock loses their place.

This is an open lane, and it is the framing that makes an IT department nod instead of reaching for the blocklist.

### Gaming

Windows idle detection often does not count gamepad input, so the screen blanks mid-session. Legitimate, and **solved entirely by mechanism (A)**. See §5 for the hard line on injecting input into games.

### What we cannot serve

**Digital forensics and law enforcement.** When a running, full-disk-encrypted machine is seized, a screensaver lock means the key is gone. A hardware jiggler plugged in at seizure keeps it unlocked. This is documented and unambiguous — and it is **hardware-only**, because you cannot install software on a target machine. Not our market, and worth knowing so we do not design for it by accident.

**Anything in a browser tab.** The [W3C Screen Wake Lock API](https://github.com/w3c/screen-wake-lock/blob/gh-pages/explainer.md) now ships in all browsers. Web dashboards, recipe sites, and presentation apps no longer need us. Our surface is the OS layer and everything the browser cannot reach.

---

## 4. Presence, honestly

We should be able to state our position on this without flinching, because users and IT departments will both ask.

**The sympathetic case is real and well-documented.** Teams presence is widely considered broken rather than merely inconvenient. Microsoft [documents](https://learn.microsoft.com/en-us/microsoftteams/presence-admins) that presence flips to Away after a few minutes of inactivity — and critically that **"the ability of a Teams admin to customize these settings isn't currently supported."** Neither the user nor their IT department can change it. Users describe reading a long document, being on a phone call, or thinking, and having the system report they have left. That is a false negative in a presence system, and correcting it feels honest to the people doing it.

**The indefensible case is also real.** In May 2024 Wells Fargo [fired more than a dozen wealth-management employees](https://www.bankingdive.com/news/wells-fires-employees-faking-productivity-finra/719033/); FINRA disclosures record them as *"discharged after review of allegations involving simulation of keyboard activity creating impression of active work."* At least one had over seven years' tenure.

**And detection is a commercial product.** [ActivTrak ships jiggler detection](https://support.activtrak.com/hc/en-us/articles/4406765537563-Detect-Mouse-Jigglers-and-Other-Activity-Mimicking-Tools) with three signals:

1. A *"continuously growing, crowd-sourced database of known mouse-jiggling software"*, matched against running processes — **100% confidence**
2. Uniform, machine-regular input timing — 80% confidence
3. Over 45 minutes continuously active on one screen without a tab switch — 70% confidence

**Signal 1 means that if this project succeeds, its binary name and process signature end up in a commercial blocklist.** That is not a reason to abandon the project; it is a reason to make sure the thing in the blocklist is a tool whose default behaviour is a sanctioned power API call.

Signal 2 has a sharper implication, covered in §5.

**Our position:** we serve the timer, not the deception. The software will keep a machine awake and can, when a user explicitly enables it, reset a session idle timer. It will not claim to be undetectable, it will not market itself on beating monitoring software, and the README will say plainly that it cannot guarantee any chat client's status.

---

## 5. The line

Three tests, each with documented support. A feature that fails any of them does not ship.

### Test 1 — Is the synthesized input semantically meaningless?

A cursor jiggle, an F15 keypress, a virtual no-op — input designed to be absorbed by an idle timer and by nothing else — is categorically different from a click at (x, y) that activates a control.

The purest expression of this is arkane-systems' **"Zen mode"**: the pointer is jiggled *virtually*, resetting the idle timer without the visible cursor moving at all. Move Mouse has the same thing under the name **Stealth** (Direction = None). That is the honest primitive, and it should be our default when input synthesis is enabled at all — not an advanced option.

Zen-style virtual jiggle also happens to solve three separate complaint categories at once: multi-monitor coordinate bugs, the cursor being visible during screen shares, and "the fast version makes my mouse unusable."

### Test 2 — Does it target another program's UI?

Once you are clicking coordinates inside someone else's application, you are a macro tool — RPA territory — and you inherit that threat model. Move Mouse crossed this line with **Activate Application**, **Run Command**, and **PowerShell Script** actions.

We will offer a scripting escape hatch (see FEATURES), but sandboxed, and it is not the headline.

### Test 3 — Does it evade a detector, or merely reset a timer?

**This is the one that costs us a feature we had already called a flagship.**

ActivTrak's detection signal 2 is literally *"uniform, machine-regular input timing."* Which means any feature marketed as **"randomised so it looks human"** is, by construction, an anti-detection feature. Our earlier plan for Bézier-curve "human-like motion" with overshoot and micro-jitter, sold as making automated movement indistinguishable from a person's, fails this test as stated.

The honest resolution is not to delete randomisation — it has genuinely benign purposes:

- A cursor that always lands on the same pixel will eventually land somewhere destructive.
- Motion that is smooth rather than teleporting is less startling to look at.
- A fixed interval synchronises badly with other periodic events.

It is to **change what it is for and how it is described.** Randomisation ships as "vary the movement so it is less intrusive," it is not the headline feature, and no documentation, marketing copy, or UI string ever describes it as looking human or avoiding detection. If we cannot describe a feature honestly in the UI, we do not ship it.

### And an absolute one

**Never inject input into a process with kernel-level anti-cheat.** Games are supported through mechanism (A) only. Competitive titles ban for far less, and a ban attributable to our software is a reputational event we would not recover from.

---

## 6. Competitors

| Tool | Platform | Status | Footprint | Mech. | Distinguishing feature |
|---|---|---|---|---|---|
| **[Move Mouse](https://github.com/sw3103/movemouse)** | Windows | Active, 784★ | Store / portable exe | (B) only | Most powerful: 9 composable actions incl. PowerShell and Run Command, Quartz cron schedules, blackout windows, auto-pause/resume. **No power API at all.** |
| **[PowerToys Awake](https://learn.microsoft.com/en-us/windows/powertoys/awake)** | Windows | Active (Microsoft) | Part of PowerToys | (A) only | First-party legitimacy. Four modes, CLI with `--pid`. **Fails at the lock screen; open Modern Standby bug.** |
| **[Mouse Jiggler](https://github.com/arkane-systems/mousejiggler)** | Windows | Active, 1.4k★ | 24 MB + .NET 10, or 134 MB | (B) | **Zen mode** — virtual jiggle. Winget and Chocolatey. |
| **[Caffeine](https://www.zhornsoftware.co.uk/caffeine/)** | Windows | Active | **306 KB** | (B) | F15 every 59 s. Configurable key — which matters, because F15 breaks in PuTTY, PowerPoint, and Google Docs. |
| **[Don't Sleep](https://www.softwareok.com/?seite=Microsoft%2FDontSleep)** | Windows | Active | **267 KB portable** | (A) | Blocks shutdown/restart/logoff too. **CPU-load and network-traffic triggers** — the only Windows tool with conditional activation. |
| **[Amphetamine](https://apps.apple.com/us/app/amphetamine/id937984704)** | macOS | Active, 4.8★ | — | (A) + optional | **The category leader by a mile.** ~14 trigger conditions: app running, USB/Bluetooth device, Wi-Fi SSID, IP address, VPN, mounted drive, CPU %, power adapter. |
| **[Insomnia](https://stanley-projects.github.io/Insomnia/)** | Windows | Active, new | No runtime deps | (A) | Positioned on AI coding agents. Local-only, no telemetry. |
| **[wakepy](https://wakepy.readthedocs.io/stable/)** | Cross-platform | Active | Python lib | (A) | The correct-API reference implementation and **the best conceptual model in the category.** |
| **[keep-presence](https://github.com/carrot69/keep-presence)** | Win/Mac/Linux | Semi-active, 329★ | Python | (B) | Idle-aware by design. **X11 only — no Wayland.** |

### The three gaps

**1. Windows has no Amphetamine.** Amphetamine's trigger system is years ahead of anything on Windows. Move Mouse has schedules and blackouts; Don't Sleep has CPU and network triggers. **Nobody has "stay awake while this process runs / while this USB device is connected / while CPU is above 40% / until this time."** This is the clearest single opportunity in the matrix, and it is exactly the axis users keep asking for and not getting.

**2. Modern Standby is broken everywhere.** Every major tool in this category has an open S0 bug:

- PowerToys [#48965](https://github.com/microsoft/powertoys/issues/48965) — `SetThreadExecutionState` only resets idle timers; on Modern Standby systems with the display off it fails to prevent connected standby. **Fix is `PowerSetRequest` with `PowerRequestExecutionRequired`.** Open.
- Mouse Jiggler #130 — *"Jiggle timer dies on Modern Standby (S0) display-off and never recovers."* Open.
- Move Mouse #109 — a user running it 24/7 reports actions stopping and the machine sleeping ~15 minutes later, repeatedly.

As S3 sleep disappears from new laptops, this is the whole market breaking at once. **If we ship one thing correctly, ship this.**

**3. Footprint is a competitive weapon, not vanity.** Mouse Jiggler ships either a 24 MB binary requiring the .NET 10 Desktop runtime, or a 134 MB self-contained portable. Don't Sleep is 267 KB. Caffeine is 306 KB.

On locked-down machines — precisely the machines that need this most — *"no installer, no runtime, single file, runs from a USB stick, works without admin"* is decisive. This retroactively justifies the entire lightweight architecture: it is not an aesthetic preference, it is the thing that makes the software usable by the people with the worst version of the problem.

**Also open ground: Linux/Wayland.** keep-presence is X11-only "due to underlying library limitations," and the only Wayland option in the topic is a 13-star bash script. Consistent with [CROSS-PLATFORM.md](CROSS-PLATFORM.md), which explains why.

---

## 7. Hardware, and what it tells us

USB jigglers sell steadily. People buy them over software for five reasons, in descending order of legitimacy:

1. **They cannot install software.** Locked-down corporate, government, clinical, and lab machines block non-allowlisted executables. A USB device operates outside OS software policy entirely.
2. **They do not own the machine.** Forensics; IT accessing a departed employee's workstation; a technician on a customer's machine in Safe Mode.
3. Nothing to uninstall, nothing in the process list.
4. One dongle works on every machine with no per-machine setup.
5. Detection avoidance — the purely mechanical class (a device that physically nudges a real mouse) never enumerates as USB at all. This is the disreputable driver, and it is why "Undetectable" appears in the product titles on Amazon.

**Software can never win 1 or 2. Those users are not our market — accept it and stop designing for them.**

But note two things. First, hardware is not undetectable either, and the honest vendors say so: CRU's own documentation states that its jiggler *"appears as a USB mouse to the host computer… and its presence and use will be logged by some operating systems."*

Second — **hardware is dumb, and that is where software wins.** A dongle cannot bind to a process's lifetime, honour a schedule, detect that you are screen-sharing and stand down, distinguish "keep the CPU running" from "keep the display lit," pause on battery, release cleanly on exit, or explain itself in a log. Every "only when I need it" complaint in the issue trackers is a request for *intelligence*, and intelligence is precisely what hardware cannot have.

**Do not compete with hardware on stealth. Compete on conditionality.** The hardware market's existence proves the demand; its limitations define our product.

---

## 8. Positioning

### The line

> **A task-bound wake lock.** It keeps the machine awake for exactly as long as your work actually needs, and not one second longer.

That framing makes both a developer and a CISO nod, which is the test.

### Vocabulary

**Use:** *keep awake* · *prevent sleep* · *wake lock* · *inhibit* · *release* · **"without modifying your power settings"** — the single best phrase in the category, because it tells an administrator the tool is non-destructive and self-reverting · *"during long-running tasks, presentations, or downloads"*.

Borrowing standards-body language — `systemd-inhibit`, `caffeinate`, W3C Screen Wake Lock — is free credibility.

**Never use:** *undetectable* — the tell, and the word in the disreputable products' titles · *simulate user activity* — Move Mouse's own tagline and the weakest thing about an otherwise serious tool, because it describes the mechanism and implies the deception · *keep your status green* · *stay active* · *bypass monitoring*.

### What earns an IT department's trust, in priority order

1. **Default to mechanism (A).** Then we can say truthfully: *"By default this tool synthesizes no input and cannot defeat a screen lock or a presence indicator."*
2. **Never touch persistent system state.** *"Does not modify your power plan; releases everything on exit"* is the sentence administrators want to read.
3. **Be auditable.** Log what state was requested, when, why, and for which process. Use `PowerSetRequest` with a descriptive reason string so the request appears in `powercfg /requests` attributed to us by name.
4. **Ship enterprise plumbing:** winget, a documented SHA, a signed binary, and a mode an administrator can lock — for example, power inhibition permitted, input synthesis disabled organisation-wide.
5. **Lead with accessibility and unattended workloads**, not with remote work.
6. **State the negative space in the README.** *"This tool will not keep your chat status green"* is a feature claim to an enterprise buyer.

### A feature nobody has built

PowerToys [#44501](https://github.com/microsoft/powertoys/issues/44501) — *"Awake? I'd prefer Asleep!"* — asks for the inverse: **why is my PC awake?** `powercfg /requests` answers this and no normal user knows it exists.

A panel that lists every process currently holding a power request, in plain language, with the offender named — is an obvious, unbuilt, entirely benign feature that would make this tool worth installing even for someone who never turns the main function on. It also demonstrates, in the product itself, exactly the transparency we are asking administrators to trust us on.

---

## 9. The name

`project-mouse` is a working title, and it should probably not survive.

Two lessons from the category:

- **"Mouse" and "jiggler" are safe but permanently cheap.** They name the mechanism, and the mechanism we are naming is the one we have just decided is *not* the default.
- **A name can be killed by a store policy.** Apple invoked Guideline 1.4.3 against **Amphetamine** over its name and pill icon, and only reversed after a public campaign — with 400,000+ downloads already shipped. Avoid drug metaphors and anything a content policy can catch.

Names in the safe, serious register: *Awake* (taken by PowerToys), *Caffeine* (taken), *Insomnia* (taken), *Don't Sleep* (taken). The register to aim for is a short, plain word about **wakefulness or attention**, not about mice.

**Open decision.** Worth settling before the first public release, because the binary name is what ends up in blocklists, winget manifests, and `powercfg /requests` output — and it is expensive to change afterwards.

---

## Sources

Move Mouse: [repo](https://github.com/sw3103/movemouse) · [wiki](https://github.com/sw3103/movemouse/wiki) · [Scenarios](https://github.com/sw3103/movemouse/wiki/Scenarios) · [Troubleshooting](https://github.com/sw3103/movemouse/wiki/Troubleshooting) · [#97](https://github.com/sw3103/movemouse/issues/97)
PowerToys: [Awake docs](https://learn.microsoft.com/en-us/windows/powertoys/awake) · [#27980](https://github.com/microsoft/PowerToys/issues/27980) · [#42720](https://github.com/microsoft/PowerToys/issues/42720) · [#44501](https://github.com/microsoft/powertoys/issues/44501) · [#44512](https://github.com/microsoft/powertoys/issues/44512) · [#48965](https://github.com/microsoft/powertoys/issues/48965)
Other tools: [Mouse Jiggler](https://github.com/arkane-systems/mousejiggler) · [Caffeine](https://www.zhornsoftware.co.uk/caffeine/) · [Don't Sleep](https://www.softwareok.com/?seite=Microsoft%2FDontSleep) · [Amphetamine](https://apps.apple.com/us/app/amphetamine/id937984704) · [Insomnia](https://stanley-projects.github.io/Insomnia/) · [wakepy](https://wakepy.readthedocs.io/stable/) · [keep-presence](https://github.com/carrot69/keep-presence)
Practitioners: [NI LabVIEW](https://forums.ni.com/t5/LabVIEW/Preventing-PCs-from-sleeping-or-hibernating/td-p/3266364) · [Adobe Media Encoder](https://community.adobe.com/t5/adobe-media-encoder-beta-bugs/computer-sleeps-mid-render/idi-p/12970399) · [OBS](https://obsproject.com/forum/threads/only-prevent-pc-from-sleeping-when-obs-is-actively-being-used.174023/) · [mRemoteNG #405](https://github.com/mRemoteNG/mRemoteNG/issues/405) · [Cloudy Nights](https://www.cloudynights.com/forums/topic/883937-keep-computer-from-going-to-sleep-during-file-transfers/) · [Microsoft Q&A kiosk GPO](https://learn.microsoft.com/en-us/answers/questions/189791/gpo-question-preventing-kiosks-from-sleeping-or-di)
Presence and detection: [Teams presence admin docs](https://learn.microsoft.com/en-us/microsoftteams/presence-admins) · [Banking Dive: Wells Fargo](https://www.bankingdive.com/news/wells-fires-employees-faking-productivity-finra/719033/) · [ActivTrak detection](https://support.activtrak.com/hc/en-us/articles/4406765537563-Detect-Mouse-Jigglers-and-Other-Activity-Mimicking-Tools) · [CurrentWare](https://www.currentware.com/blog/mouse-jiggler-detection/)
Policy and standards: [UNI screen-lock policy](https://uniservicehub.atlassian.net/wiki/spaces/SH/pages/207659145/Screen+Lock+Out+Times+-+15+Minute+Maximum) · [W3C Screen Wake Lock](https://github.com/w3c/screen-wake-lock/blob/gh-pages/explainer.md) · [AppleInsider on Amphetamine's name](https://appleinsider.com/articles/21/01/02/apple-threatens-to-pull-amphetamine-macos-app-over-branding)
