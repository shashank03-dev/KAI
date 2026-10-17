# OS-level APIs and pitfalls for a desktop computer-use agent (macOS first, Linux second), as of 2026-10-17

Research done 2026-10-17 with web search and fetch. Most sources are practitioner reports (GitHub issues and PRs, Apple Developer Forums, project docs) rather than Apple or freedesktop specifications. Each finding is dated where the source gives a date. Anything recalled from background knowledge and not re-verified this session is labelled as such under Inferences or Gaps.

## macOS screen capture: ScreenCaptureKit vs CGWindowListCreateImage, Sequoia prompts, macOS 26 changes, latency, Rust crates

### Takeaway
`CGWindowListCreateImage` and `CGDisplayCreateImage` are unavailable when building against the macOS 15 SDK. ScreenCaptureKit (macOS 12.3+) is the only forward path. Since Sequoia (2024), any ScreenCaptureKit capture made outside Apple's `SCContentSharingPicker`, `SCScreenshotManager` included, raises a "bypass the system private window picker" alert that offers "Allow for one month". No documented way exists for a general-purpose agent to suppress it: the persistent-content-capture entitlement is reserved for screen-sharing products, the picker cannot do full-screen screenshots through `SCScreenshotManager`, and MDM profiles are for managed fleets only. On macOS 26 (Tahoe), unbundled executables and child processes have trouble getting or inheriting Screen Recording, so capture should run inside a signed `.app` bundle.

### Cited Findings
- **Deprecated, then removed from the SDK.** `CGWindowListCreateImage` was deprecated starting in macOS 14, with ScreenCaptureKit as the replacement — [getsentry/sentry-unreal#1126](https://github.com/getsentry/sentry-unreal/issues/1126).
  - Building against the macOS 15 SDK makes it unavailable, so code fails to compile. Seen in a JUCE build on macOS 15.5 — [google/eclipsa-audio-plugin#10](https://github.com/google/eclipsa-audio-plugin/issues/10). Also seen in cliclick, which MacPorts fixed by capping the deployment target at 14 — [MacPorts #71136](https://trac.macports.org/ticket/71136).
  - `CGDisplayCreateImage` hit the same wall with the macOS 15 SDK — [MacPorts #70756](https://trac.macports.org/ticket/70756).
- ScreenCaptureKit is available on macOS 12.3 and later — [Apple docs](https://developer.apple.com/tutorials/data/documentation/screencapturekit.md). `SCScreenshotManager` was introduced at WWDC23 (macOS 14) — [WWDC23 "What's new in ScreenCaptureKit"](https://developer.apple.com/videos/play/wwdc2023/10136/).
- **Sequoia re-prompt cadence (2024).** In the Sequoia betas the prompt went from weekly to monthly, offering "Allow For One Month". The extra prompts after a reboot or logout were dropped (beta 6, Aug 2024) — [TidBITS 2024-08-19](https://tidbits.com/2024/08/19/apple-reduces-excessive-sequoia-permission-requests-shifts-to-monthly); [9to5Mac 2024-08-14](https://9to5mac.com/2024/08/14/macos-sequoia-screen-recording-prompt-monthly/). The prompt reportedly applies to ScreenCaptureKit apps too — [TechRadar](https://www.techradar.com/computing/mac-os/on-macos-sequoia-youll-only-have-to-deal-with-screen-recording-permission-pop-ups-once-a-month-rather-than-once-a-week).
- **Apple DTS on what triggers the alert (Apple Developer Forums, Sep–Dec 2024)** — [Forum thread 765103](https://developer.apple.com/forums/thread/765103):
  - Quinn (Apple DTS) reproduced the alert using only `SCShareableContent.current` plus `SCScreenshotManager.captureImage`, on macOS 14.6.1 and 15.0.
  - The first call shows the normal "record this computer's screen" prompt. A later call that returns real data shows "…is requesting to bypass the system private window picker and directly access your screen and audio", with the buttons "Allow for one month" and "Open System Settings".
  - Quinn named only two ways to avoid it: the `com.apple.developer.persistent-content-capture` entitlement ("intended specifically for screen sharing products"), and `SCContentSharingPicker`.
  - The developer filed FB15360079. Removing `CGWindowListCreateImage` did not stop the alert.
- **The persistent-content-capture entitlement is undocumented and unobtainable for most apps** (2024 reporting) — [MacTrast](https://www.mactrast.com/2024/08/macos-sequoia-beta-requires-users-to-update-screen-recording-app-permissions-monthly/); [Macworld](https://www.macworld.com/article/2429205/macos-sequoia-requires-regular-permission-checks-when-using-certain-apps.html).
- **MDM profiles.** Apple provides an MDM-deployed profile that suppresses the alerts. Self-installing configuration profiles for the ScreenCapture TCC service is prohibited — [tinyapps.org 2024-09](https://tinyapps.org/blog/202409180700_disable_sequoia_nag.html).
- **What the picker can and cannot do.** Tested on macOS 26.5; PR merged 2026-09-24 — [openscreen PR #737](https://github.com/getopenscreen/openscreen/pull/737):
  - Captures started from `SCContentSharingPicker` gave full-resolution video for display or window picks, with no Screen Recording grant and no bypass alert.
  - `SCScreenshotManager` on a picker filter failed with error -3801 and raised the grant prompt.
  - System audio came through as all zeros without the grant.
  - A pick does not persist across launches.
  - Without `excludedWindowIDs`, the app's own windows appear in a display pick.
  - The PR puts the minimum at macOS 15.2, because of `includedDisplays` and `includedWindows`.
- **macOS 26.1 hides unbundled executables (2025–26).** A background Unix executable calling `CGRequestScreenCaptureAccess()` stopped appearing in the Screen Recording pane in 26.1. It worked in 26.0.1 — [Apple Forums 807323](https://developer.apple.com/forums/thread/807323).
- **Tahoe responsible-process attribution (2026).** On Tahoe, TCC attributes capture to the responsible process. A child process such as node, launched by a LaunchAgent or wrapper, may not inherit the parent's Screen Recording grant. This is a single-project bug report — [openclaw#14138](https://github.com/openclaw/openclaw/issues/14138).
- **Cua's Tahoe capture issue (opened 2026-01-21, still open).**
  - Under macOS 26.2 inside a Lume VM, captures showed only the wallpaper. ScreenCaptureKit returned -3801 even though TCC showed Python as allowed.
  - The bundling and Info.plist theory was tested (py2app with `NSScreenCaptureUsageDescription`) and did not fix it.
  - The author concluded it was a Virtualization.framework bug: windows had no backing store.
  - A commenter reported on 2026-03-17 that a display-scoped `SCContentFilter`, filtered down to the app, was more reliable than per-window filters on Tahoe.
  - Source: [trycua/cua#870](https://github.com/trycua/cua/issues/870).
- **Cua driver probes capture separately.** Cua's driver notes that "macOS Tahoe can show a separate dialog" for the ScreenCaptureKit probe. So `permissions status` does not run a capture probe, while `permissions grant` requires a live capture probe to succeed — [Cua docs: macOS permissions](https://cua.ai/docs/reference/cua-driver/macos-permissions).
- **Settings pane renamed on Tahoe.** The pane is now "Screen & System Audio Recording", and the Sequoia re-prompt carried over to Tahoe. This comes from a third-party guide, not verified against Apple — [Screenify 2026-08-13](https://www.screenify.studio/blog/2026-08-13-macos-screen-recording-permission-fix).
- **Rust crates:**
  - `screencapturekit` (doom-fish/screencapturekit-rs) offers safe Rust bindings. docs.rs shows `screencapturekit = "5"`, while a COVERAGE file at 8.0.1 exists, so it is actively versioned — [docs.rs](https://docs.rs/screencapturekit); [COVERAGE 8.0.1](https://docs.rs/crate/screencapturekit/8.0.1/source/COVERAGE.md); [crates.io](https://crates.io/crates/screencapturekit).
  - The crate page says apps need an Info.plist purpose string, and the binary must be enabled in Screen Recording (in development that is usually the terminal or IDE) — [crates.io](https://crates.io/crates/screencapturekit).
  - `xcap` (nashaofu) is cross-platform (Linux X11 and Wayland, macOS, Windows); its last push was 2026-03-21 per an aggregator — [gittrend](https://gittrend.io/repo/nashaofu/xcap).

### Inferences
- **An agent cannot capture without prompts.** It needs unattended full-screen screenshots, so it cannot use the picker path; `SCScreenshotManager` on a picker filter fails. It must take the Screen Recording grant and accept the "Allow for one month" re-prompt. The UX should expect a monthly re-consent and detect it: probe a capture and treat error -3801 as revoked or not granted.
- **For streaming, use a long-lived `SCStream`** and grab the latest frame instead of calling `SCScreenshotManager` each step. This is also likely lower latency, but I found no measured numbers this session (see Gaps).
- **Put capture in a signed `.app` bundle** with `NSScreenCaptureUsageDescription`, launched through LaunchServices so it is the responsible process. Do not use a bare CLI binary or a child of a LaunchAgent. This follows from the 26.1 forum report, openclaw#14138 and the Cua driver's LaunchServices grant flow.
- **Background knowledge, not re-verified this session:**
  - `objc2-screen-capture-kit` is the auto-generated framework crate in the objc2 project. It gives raw but complete coverage, versioned with objc2 0.3 / 0.6.x in 2025.
  - xcap's macOS backend historically used CoreGraphics (`CGWindowListCreateImage`) and was migrating to ScreenCaptureKit.
  - Check both before adopting either crate.

### Gaps
- I found no measured latency comparison for `SCScreenshotManager`, `SCStream` frame grab and `CGWindowListCreateImage` on Apple Silicon. A Kai-side benchmark is needed.
- I found no Apple release note listing macOS 26 TCC or ScreenCaptureKit changes. The Tahoe findings above come from forums and issues.
- Whether xcap 0.x currently uses ScreenCaptureKit on macOS, and its latest crates.io release date, were not confirmed.
- No source confirmed whether the re-prompt period on macOS 26 is still 30 days.

## macOS accessibility, input injection, TCC, and code-signing identity

### Takeaway
Read UI structure with `AXUIElement`. Electron and Chromium apps expose a full tree only after `AXManualAccessibility` (or `AXEnhancedUserInterface`) is set on the app element. Inject input with `CGEventPost`, which needs the Accessibility grant. TCC grants are keyed to the code signature: ad-hoc builds are pinned to the cdhash, so every rebuild silently loses the grant. Sign every build with a stable certificate (Developer ID, Apple Development, or even self-signed), keep the bundle ID and install path fixed, and relaunch after granting. Notarization is needed only for Gatekeeper on downloads, not for keeping permissions.

### Cited Findings
- **Electron's accessibility switch.** Electron's docs say third-party assistive technology on macOS can turn on accessibility inside an Electron app by setting `AXManualAccessibility` on the app's process element with `AXUIElementSetAttributeValue` — [Electron accessibility docs](https://docs.w3cub.com/electron/tutorial/accessibility); [Electron PR #10305](https://ayakael.net/mirrors/electron/commit/e6733b4b23ccad59bdb199d08ac3048d6bdb5279).
  - `app.setAccessibilitySupportEnabled` is off by default, and "rendering accessibility tree can significantly affect the performance of your app".
  - Chromium's handling of the attribute is in code review [1413423003](https://codereview.chromium.org/1413423003).
- **Ad-hoc signing loses the grant on every rebuild (2026 practitioner report).** For an ad-hoc signed app, macOS ties the Accessibility grant to the exact cdhash, not the designated requirement. Every rebuild silently revokes it, while the app still shows as checked in Privacy & Security.
  - After switching to a real signing certificate, two clean builds gave identical designated requirements, and six rebuild-and-reinstall cycles kept the grant.
  - A self-signed certificate keeps permissions but not Gatekeeper approval, so users still need "Open Anyway".
  - Source: [shannoncoat commit 061ae098da](https://git.bdeshi.space/bdeshi/shannoncoat/src/tag/0.0.8).
- **Signing guidance from another project (2026).** A real signing identity is needed for stable permissions, ad-hoc signing is opt-in and fragile, and the bundle ID and install path should stay fixed — [aura signing.md (2026.3.2)](https://cdn.jsdelivr.net/npm/@api-pedia/aura@2026.3.2/docs/platforms/mac/signing.md).
- **Grants follow the host, not the library.** TCC grants go to the app or executable that runs the code, not to a library or package — [swift-computer-use](https://awesome.ecosyste.ms/projects/github.com%2Fgregorym%2Fswift-computer-use).
  - A host that embeds the Cua Swift libraries needs Accessibility for any `AXUIElement` work and Screen Recording for screenshots, granted against its own bundle identifier — [Cua Swift integration](https://cua.ai/docs/cua-driver/guide/getting-started/swift-integration).
- **Permission answers are cached per process.** A helper started before the grant keeps reporting "missing" until it restarts — [pi-computer-use troubleshooting](https://cdn.jsdelivr.net/npm/@injaneity/pi-computer-use@0.5.1/docs/troubleshooting.md). Cua likewise says the responsible app must be quit and relaunched after a grant changes — [Cua docs](https://cua.ai/docs/reference/cua-driver/macos-permissions).
- **Input Monitoring.** Input Monitoring, Screen Capture and Accessibility are separate TCC services; Input Monitoring is for listening to events, Accessibility is for posting and controlling — [HackTricks macOS TCC page](https://hacktricks.wiki/en/macos-hardening/macos-security-and-privilege-escalation/macos-security-protections/macos-input-monitoring-screen-capture-accessibility.html).

### Inferences
- **Background knowledge, not re-verified this session:**
  - Posting events needs Accessibility; check it with `AXIsProcessTrustedWithOptions`, and use `CGPreflightPostEventAccess` / `CGRequestPostEventAccess` on macOS 10.15+.
  - `AXUIElementSetMessagingTimeout` defaults to about 6 s. A hung target app stalls tree walks, so set a short per-element timeout (around 0.5–1 s) and walk the tree with `AXUIElementCopyMultipleAttributeValues` to cut IPC round trips.
  - The App Sandbox is incompatible with posting events into other apps and controlling them via AX, so a computer-use app must ship outside the Mac App Store sandbox.
  - Hardened runtime plus notarization is compatible, provided the needed entitlements are present: e.g. `com.apple.security.automation.apple-events` plus `NSAppleEventsUsageDescription` for Automation.
- **Practical identity recipe for Kai:**
  - Ship one signed `Kai.app` that owns all TCC grants, with a fixed bundle ID and installed to `/Applications`.
  - Sign it with Developer ID for releases, and with a stable local certificate in development: Apple Development, or a self-signed code-signing cert in the login keychain.
  - Never ad-hoc sign a build that testers grant permissions to.
  - Run capture and input in-process, or in helpers launched by Kai.app, so Kai.app is the responsible process.

### Gaps
- No Apple primary source was found this session for the AX messaging-timeout default, or for the exact TCC designated-requirement matching rules. Apple's "TN3127: Inside Code Signing: Requirements" and DTS forum posts are the places to confirm them.
- `AXEnhancedUserInterface` side effects were not sourced this session. It is known anecdotally to break window animations and positioning in some apps.

## How existing agents handle macOS permissions

### Takeaway
Mature agents put permissions on one signed app identity, launched through LaunchServices, and give the user explicit check and grant tooling. Cua's driver is the clearest public example.

### Cited Findings
- **Cua's `cua-driver`.**
  - `permissions status` reports the Accessibility and Screen Recording grants without prompting, through a running daemon.
  - `permissions grant` launches the installed `CuaDriver.app` through LaunchServices so prompts are attributed to the app, then requires a live capture probe to pass.
  - It never sends a prompt-capable request over the daemon socket.
  - `doctor` and `diagnose` report permission attribution.
  - Source: [Cua docs: macOS permissions](https://cua.ai/docs/reference/cua-driver/macos-permissions).
- **cmux** bundles a helper with its own TCC identity, so granting Screen Recording to the helper does not require restarting the main app — [cmux-cua skill](https://skills.cat/skills/manaflow-ai/cmux/cmux-cua).
- **Hermes (Nous Research)** tells users to grant Accessibility and Screen Recording to the terminal, or to the Hermes app — [Hermes computer-use docs](https://hermes-agent.nousresearch.com/docs/user-guide/features/computer-use).
- **Semantouch** is a Swift helper that pairs ScreenCaptureKit window capture with the AX API, with an app denylist and human confirmation for consequential actions — [watzon/semantouch](https://github.com/watzon/semantouch).
- **Cua on Tahoe VMs.** Cua hit Tahoe capture failures inside VMs, and its Python server injects input with pynput — [trycua/cua#870](https://github.com/trycua/cua/issues/870).

### Inferences
- The pattern to copy is one signed app identity, launched by LaunchServices, plus a non-prompting status check, an explicit grant flow with a live probe, and a relaunch after the grant.
- Terminal-attributed grants, the Hermes and dev-mode approach, are fragile on Tahoe given the 26.1 unbundled-executable change.

### Gaps
- I did not find primary sources this session for how UI-TARS Desktop, Open Interpreter, the Claude and ChatGPT desktop apps, Raycast or Rewind handle signing and TCC.

## Linux: AT-SPI2, X11, Wayland portals (ScreenCast, RemoteDesktop, libei), uinput, consent and restore tokens

### Takeaway
**X11:** XTest plus AT-SPI2 still works fully, with no consent dialogs.

**Wayland:** the only sanctioned path is xdg-desktop-portal.
- Screen capture uses `ScreenCast` delivered over PipeWire.
- Input uses `RemoteDesktop`, preferably via `ConnectToEIS` (libei/liboeffis).
- Both need a user consent dialog. Restore tokens (`persist_mode`) are meant to avoid repeat prompts, but 2026 reports show they are buggy on both GNOME and KDE.

**Alternatives and accessibility gaps:**
- uinput (ydotool) bypasses the compositor but needs root or a udev rule, and is keycode- and layout-dependent.
- AT-SPI2 works on Wayland for tree and text, but global screen coordinates are unreliable or unavailable. Map element positions through the compositor or through screenshots instead.

### Cited Findings
- **KDE Plasma 6.1 (mid-2024)** added the Input Capture portal and the `ConnectToEIS` method on the RemoteDesktop portal — [Phoronix](https://www.phoronix.com/news/KDE-Plasma-6.1-Beta); [GamingOnLinux 2024-05](https://www.gamingonlinux.com/2024/05/kde-plasma-6-1-beta-released-with-wayland-explicit-sync-support/). KWin's libeis is reached through the portal; it exposes no public socket — [KWin input.cpp](https://invent.kde.org/plasma/kwin/-/blob/master/src/input.cpp).
- **libei** reached 1.6.0 — [libei API docs](https://libinput.pages.freedesktop.org/libei/api/). `liboeffis` is the helper library that contacts the RemoteDesktop portal and obtains an EIS socket — [Fedora liboeffis](https://packages.fedoraproject.org/pkgs/libei/liboeffis).
  - The portal session's `connect_to_eis` returns an fd for `ei_setup_backend_fd()` — [PyGObject Xdp docs](https://api.pygobject.gnome.org/Xdp-1.0/class-InputCaptureSession.html).
  - gnome-remote-desktop 46.0 bumped its minimum libei — [Debian changelog](https://metadata.ftp-master.debian.org/changelogs/main/g/gnome-remote-desktop/testing_changelog).
- **GNOME 50** added virtual-monitor and remote-desktop improvements (HiDPI, monitor mode emulation) — [linux.org relaying Phoronix](https://www.linux.org/threads/phoronix-gnome-50-lands-virtual-monitor-remote-desktop-improvements-including-hidpi.62062/latest).
- **Restore tokens:**
  - GNOME implemented persistent RemoteDesktop sessions by serializing the streams and device types under a token — [xdg-desktop-portal-gnome MR 88](https://gitlab.gnome.org/GNOME/xdg-desktop-portal-gnome/-/merge_requests/88).
  - Plasma 6.5 (Sep 2025) added a settings page for managing saved grants — [pinggy blog](https://pinggy.io/blog/remote_desktop_into_wayland_without_a_vps/).
- **Restore-token bugs (2026):**
  - KDE issued a restore token even when the user unchecked "restore". Fixed in Plasma 6.6.6, commit dated 2026-05-22 — [KDE bug 517454](https://bugs.kde.org/show_bug.cgi?id=517454).
  - On Plasma 6.7.2 (Jul 2026), a non-UUID token was rejected after a service restart, so local approval was needed again — [tigervnc-users](https://groups.google.com/g/tigervnc-users/c/cdv-YPjQvkU).
  - A KDE Connect developer found persistence worked only within the same session; the token was invalid after a reboot — [kdeconnect MR 639](https://invent.kde.org/network/kdeconnect-kde/-/merge_requests/639).
  - On GNOME, the dialog appeared repeatedly when a client created and destroyed RemoteDesktop/ScreenCast sessions over and over, ignoring the restore token. The same did not happen on KDE — [ConnectWise report](https://screenconnect.product.connectwise.com/communities/1/topics/5169-bug-linux-client-repeatedly-opens-gnome-remote-desktop-permission-dialogs-and-ignores-restore-token).
- **uinput and ydotool:**
  - ydotool writes to `/dev/uinput`, so it needs root or a udev rule (e.g. `KERNEL=="uinput", GROUP="input", MODE="0660"` plus input-group membership and a re-login). It has no focus awareness and no window management — [wdotool README](https://github.com/cushycush/wdotool); [password-store list](https://lists.zx2c4.com/pipermail/password-store/2021-June/004455.html).
  - Since ydotool 1.0, `key` takes keycodes only; `type` is layout-dependent — [Debian ydotool README](https://sources.debian.org/src/ydotool/1.0.4-3/README.md).
  - wtype uses Wayland protocols and handles Unicode with a temporary keymap, but it relies on the virtual-keyboard protocol, which GNOME does not support — [password-store list](https://lists.zx2c4.com/pipermail/password-store/2021-June/004455.html). The GNOME caveat is background knowledge.
  - wdotool uses the RemoteDesktop portal instead of uinput — [wdotool](https://github.com/cushycush/wdotool).
- **AT-SPI on Wayland:**
  - Global desktop coordinates are not supported — [at-spi2-core#14](https://gitlab.gnome.org/GNOME/at-spi2-core/issues/14).
  - GTK 4.10+ returned (0,0) for every widget's extents — [GTK#6343](https://gitlab.gnome.org/GNOME/gtk/-/issues/6343).
  - GTK3 popovers are placed at 0,0 on Wayland — [GTK#5149](https://gitlab.gnome.org/GNOME/gtk/-/issues/5149).
  - Orca requested desktop coordinates, got window-relative ones, and clicked the wrong place — [at-spi2-core#14](https://gitlab.gnome.org/GNOME/at-spi2-core/issues/14).
  - Accerciser proposes asking the compositor instead — [accerciser MR 33](https://gitlab.gnome.org/GNOME/accerciser/-/merge_requests/33).
- **Newton** is a Wayland-native accessibility architecture by Matt Campbell, funded by the Sovereign Tech Fund. It was unmerged as of its 2024 update, built on GNOME 46.2 — [GNOME a11y blog](https://blogs.gnome.org/a11y/); [OSSNA 2024 slides](https://mwcampbell.us/ossna2024-slides.pdf).

### Inferences
- **Recommended Linux stack:**
  - X11: XTest for input, XGetImage/XShm for capture, AT-SPI2 for the tree.
  - Wayland: the portal ScreenCast stream (PipeWire) for capture, RemoteDesktop with `ConnectToEIS` (via liboeffis/libei) for input, and AT-SPI2 for names, roles, text and actions.
- **Prefer AT-SPI actions over coordinates on Wayland.** Use `Action.do_action` and `EditableText` where possible, and fall back to screenshot-relative coordinates from Set-of-Marks rather than AT-SPI extents.
- **Store the restore token and handle its loss.** Persist it securely, reuse one long-lived session (never create and destroy sessions per step), and fall back cleanly to a fresh consent dialog when the token is rejected.
- **Offer uinput only as an explicit opt-in.** It is a privileged, power-user fallback, for example on wlroots compositors without the RemoteDesktop portal.

### Gaps
- No 2026 primary source on GNOME 48–50 specifics for `ConnectToEIS` or InputCapture.
- No 2026 source on whether Newton merged.
- The wlroots/Sway RemoteDesktop portal status was not checked. xdg-desktop-portal-wlr historically had ScreenCast only, no RemoteDesktop; that is background knowledge and unverified.
- I found no current information on the `atspi` Rust crate (odilia-app/atspi) release state.

## Which language ecosystem has the best maintained bindings

### Takeaway
Swift is first-class on macOS: ScreenCaptureKit, AX, CGEvent and the picker all have native APIs, which is why Cua, Semantouch and swift-computer-use are Swift. Rust is viable on both OSes:
- macOS: `screencapturekit` (actively versioned to 8.x in 2026) or the generated `objc2-*` crates, with AX and CGEvent via `core-graphics` / `objc2-application-services`.
- Linux: `ashpd` for portals and `atspi` (zbus) for AT-SPI.

Python (pyobjc, pyatspi, pynput) is the fastest to prototype but suffers most from TCC attribution, because the grant goes to "Python" or the terminal. Node (nut.js) lags on Wayland.

### Cited Findings
- **Rust:**
  - `screencapturekit` crate: "safe, idiomatic Rust bindings for Apple's ScreenCaptureKit", with an 8.0.1 coverage document — [docs.rs](https://docs.rs/screencapturekit); [COVERAGE 8.0.1](https://docs.rs/crate/screencapturekit/8.0.1/source/COVERAGE.md).
  - xcap: cross-platform Rust capture, including Wayland, last pushed 2026-03-21 — [gittrend](https://gittrend.io/repo/nashaofu/xcap).
- **Swift agent helpers** (Cua driver, swift-computer-use, Semantouch) — [Cua Swift integration](https://cua.ai/docs/cua-driver/guide/getting-started/swift-integration); [semantouch](https://github.com/watzon/semantouch).
- **Python attribution failure.** Python capture via pynput and Quartz led to a TCC entry for "Python" and -3801 errors on Tahoe — [trycua/cua#870](https://github.com/trycua/cua/issues/870).
- **Python portal bindings.** PyGObject Xdp (libportal) bindings expose `connect_to_eis` — [PyGObject Xdp](https://api.pygobject.gnome.org/Xdp-1.0/class-InputCaptureSession.html).

### Inferences
- **For Kai (Rust):**
  - macOS: keep everything in-process in the signed Kai.app, using `screencapturekit` or `objc2-screen-capture-kit` for capture, and `objc2-application-services` / `accessibility-sys` plus `core-graphics` CGEvent for AX and input. A small Swift helper is justified only if a picker-based or SwiftUI-only API is needed; it would then need its own TCC identity, as cmux does.
  - Linux: `ashpd` (ScreenCast, RemoteDesktop with `ConnectToEIS`), the `reis` crate for the EI protocol, PipeWire via `pipewire-rs`, and `atspi` (zbus).
  - The crate names `ashpd` and `reis` are background knowledge, not verified this session.

### Gaps
- Release dates and maintenance status of `ashpd`, `reis`, `atspi`, `enigo`, `rdev` and `nut.js` in 2026 were not verified this session. Check crates.io and npm before committing to any of them.
