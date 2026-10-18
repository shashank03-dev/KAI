# Kai on macOS (Apple Silicon)

Kai runs on Macs with Apple Silicon (M1 and later), macOS 13 or newer. This
note is how to build, install and grant access, then what is different from
Linux and Windows and why. Intel Macs are not supported: there is no Intel
build of the local model runtime, and nobody has tested Kai on one.

## Build and install

You need Rust (`curl https://sh.rustup.rs -sSf | sh`) and the Xcode Command
Line Tools (`xcode-select --install`). You do not need the full Xcode (see
"Metal shaders" below).

```sh
scripts/macos/package_dmg.sh        # builds dist/Kai.app and dist/Kai.dmg, checks both
scripts/macos/bundle.sh --install   # or: build dist/Kai.app and copy it to /Applications
open /Applications/Kai.app
```

To install from the image: open `Kai.dmg` and drag Kai to the Applications
shortcut beside it. The script checks the bundle (arm64 only, signatures,
licenses, no settings, traces, benchmark runs or key-shaped text), verifies
the image checksum, then mounts it read-only and checks it again. Set
`KAI_SIGN_IDENTITY` (a Developer ID Application identity) and
`KAI_NOTARY_PROFILE` (a `notarytool store-credentials` profile) to sign and
notarize; without them the image is ad hoc signed and not notarized, and the
script prints so. Licenses of everything inside are in
`Kai.app/Contents/Resources/licenses/`.

The same script builds `dist/kai-macos-aarch64.zip` for a release, and the
release workflow runs it on a `macos-15` runner. To run without bundling:
`cargo run --release -p kai-app` (the app) or `cargo run --release -p kai-cli`
(the `kai` command). Run that way, macOS gives the permissions to your
terminal app, not to Kai.

The app is signed ad hoc, not with an Apple developer certificate. A zip
you download with a browser is quarantined, so the first open says Apple
"cannot check it for malicious software". To open it anyway: Control-click
Kai.app › Open, or run `xattr -dr com.apple.quarantine /Applications/Kai.app`.
A copy built on the same Mac is not quarantined.

## Permissions

Onboarding shows three checks that turn green by themselves:

| Check | macOS setting | What it gives Kai |
|---|---|---|
| Screen capture | Privacy & Security › Screen & System Audio Recording | Pixels of other apps. Without it a capture still works but shows only the wallpaper and Kai's own windows, so Kai checks it with `CGPreflightScreenCaptureAccess` and does not trust the capture alone. |
| Accessibility | Privacy & Security › Accessibility | Reading other apps' buttons and fields, and pressing them. |
| Mouse and keyboard | (the same Accessibility switch) | Posting clicks and keystrokes (`CGEventPost`). |

"Turn it on" asks macOS for both Accessibility and Screen Recording. Each
shows the system prompt that opens the right pane of System Settings. macOS
shows that prompt only once per app: after a refusal, turn Kai on in System
Settings by hand. Screen Recording needs Kai to be reopened before it applies.

macOS ties these permissions to the app's code signature. An ad hoc
signature changes with every build, so after a rebuild the switches can show
"on" while macOS denies access. Turn Kai off and on again, or remove it with
"−" and add it again.

If Kai cannot click or type, open **Settings › Permissions** in Kai. This page
checks the permissions granted to the **running app**, and shows the actual
input-backend error. If Kai reports **Missing** despite an enabled macOS
switch, quit Kai, remove its old entry from the corresponding macOS Privacy &
Security list, add `/Applications/Kai.app` again, grant access, then reopen
Kai. Refresh the checks in Kai after reopening. Screen capture and input are
checked separately; both must be available for a GUI run.

Once you allow it, Kai picks up mouse and keyboard control without a
restart: the app tries again every 2 seconds while it has no control.

## What is different on a Mac

- **Coordinates are points, not pixels.** A Retina screen captures two
  pixels per point, but clicks and the accessibility tree use points. Kai
  scales each capture to the display's size in points (`frame::to_display`)
  before anything else sees it, so the marks, the clicks and the tree all
  agree. On Linux and Windows the sizes already match and the capture is
  untouched. Without this, every click on a MacBook landed at twice its
  coordinates. For the same reason, the app reports its own windows (masked
  out of every frame) in points on a Mac, not in pixels.
- **Accessibility tree** (`a11y/macos.rs`). Kai reads the frontmost app's
  focused window and its menu bar through `AXUIElement`. It asks for all of
  an element's attributes in one call (`AXUIElementCopyMultipleAttributeValues`),
  so each element costs one round trip to the app, the way Windows uses one
  cached UI Automation query. A 0.25 s messaging timeout per call and the
  shared 600 ms read budget keep a hung app from stalling a step; a big
  tree (a browser page, Mail) stops at the budget and keeps what it read.
  Until 2026-10-18 the outer timeout fired first and threw that partial
  tree away for the grid. Secure
  text fields (`AXSecureTextField`) are password fields, and Kai never fills
  or presses them through the tree. Items of a closed menu are skipped,
  because they exist in the tree but are not on screen. A row scrolled out
  of its scroll area is marked invisible; menus and popovers are not
  clipped to their window, because they can reach past it. Electron apps
  (Slack, VS Code, Discord) build a tree only after a client sets
  `AXManualAccessibility`, so Kai sets it on the frontmost app.
- **Accessibility-first actions** (`a11y/macos_act.rs`). A press is
  `AXPress`. A fill sets `AXValue` on an empty, settable field and reads it
  back. This is the same contract as UI Automation on Windows; anything
  uncertain falls back to the mouse. A control named with an ellipsis
  ("Save As…") opens a dialog and is clicked with the mouse, and an
  `AXPress` that gets no answer in time (the app is showing a modal)
  counts as pressed, so it is never pressed twice.
- **Shortcuts use `cmd`.** Models default to `ctrl`. On a Mac the system
  prompt adds one rule saying shortcuts use `cmd` (`prompt::MAC_RULE`). It
  is added only when the eyes are on a Mac (`Perceptor::mac`), so the prompt
  on Linux and Windows stays byte for byte the same. Long text is pasted
  with cmd+v.
- **Windows and Linux habits still work.** Gemma often sends `ctrl+c` or
  `alt+tab` anyway; on a Mac those do nothing, the screen does not change,
  and the model tries again. The hands press the Mac shortcut instead
  (`input/mac_keys.rs`): `ctrl` + an editing or app letter (copy, paste,
  cut, undo, select all, save, find, tabs, windows, reload, address bar,
  quit, bold, zoom) becomes `cmd`, `ctrl+y` becomes `cmd+shift+z`, `alt+tab`
  becomes `cmd+tab`, `alt+f4` closes the window (`cmd+w`), and a
  `ctrl`-click (a right-click on a Mac) becomes a `cmd`-click. In a
  terminal (Terminal, iTerm2, Warp, kitty, Alacritty, WezTerm, Hyper,
  Ghostty) `ctrl` keeps its meaning, so `ctrl+c` still interrupts. App
  names work the same way (`launch_mac.rs`): when `open -a` does not know
  the name, "File Explorer" opens Finder, "Notepad" TextEdit, "System
  Preferences" System Settings, "Task Manager" Activity Monitor. The Mac
  prompt rule also names the Mac apps and warns that shell tools are BSD.
- **Stopping.** In the app, `ctrl+alt+Escape` (Control-Option-Escape) works
  from any app. macOS delivers global hotkeys only through the main
  thread's event loop, which the `kai` command line does not run. So in the
  terminal the stop is the top-left corner of the screen, and the command
  line says so instead of naming a shortcut that would never fire.
- **Notifications.** When Kai needs you (a code, a CAPTCHA), it shows a
  macOS notification through `osascript`. The title and text are quoted as
  AppleScript strings, so their text can never run as script (tested).
- **Kai's own browser** works on a Mac too. It finds Chrome, Chromium, Edge
  or Brave in `/Applications` or `~/Applications` (or uses `KAI_BROWSER`),
  over the same `--remote-debugging-pipe` as on Linux.
- **Local models.** The pinned llama.cpp build `b11429` has a macOS arm64
  build with Metal (`llama-b11429-bin-macos-arm64.tar.gz`, 11,971,406 bytes,
  SHA-256 `740288ec…ee82459`, hashed here). The GPU shares the Mac's memory.
  By default macOS lets it use about two thirds of RAM (three quarters above
  36 GB), and the model store counts that once: a model runs on the GPU or
  on the CPU, never "split" across two memories that are really one. On a
  16 GB MacBook Air, Gemma 4 E4B fits on the GPU.
- **Settings** live in `~/.config/kai`, as on Linux, so the app and the
  `kai` command share them. Keys go to the macOS Keychain.

## Metal shaders

GPUI normally compiles its Metal shaders at build time with `xcrun metal`.
That compiler ships only with the full Xcode, not with the Command Line
Tools most Macs have, so the build failed there. Kai turns on GPUI's
`runtime_shaders` on macOS (`crates/kai-app/Cargo.toml`), so the shaders are
compiled once when the app starts. That costs a few milliseconds at launch
and needs no Xcode.

## What was rejected

- **Grid-only on macOS** (no accessibility tree). It would ship sooner, but
  the grid is the weakest grounding Kai has: the model guesses cells
  instead of naming buttons. The AX reader is about 250 lines and mirrors
  the Windows one.
- **ScreenCaptureKit directly.** xcap already captures through Core
  Graphics. The only Mac-specific problem was the point/pixel scale, which
  is fixed above. A second capture path would be more code without a
  measured gain.
- **A Developer ID signature and notarization.** These need a paid Apple
  developer account, which is the owner's call. The ad hoc signature and
  the quarantine note above are enough for a first release.

## How it was checked, and what is not yet checked

On a real Mac: `.github/workflows/macos.yml` runs on GitHub's `macos-15`
runner (Apple Silicon, macOS 15, a 1024x768 virtual display at scale 1). It
runs clippy, the whole test suite, `scripts/macos/bundle.sh` (build, ad hoc
sign, smoke test), the app's own screenshots of every screen
(`KAI_CAPTURE`), `scripts/macos/live_check.sh`, and the bench's recorded
replies on the real desktop. The first runs (2026-10-15) found and fixed:

- The accessibility read asked the system-wide element for
  `AXFocusedApplication`, which failed although Kai was trusted, so every
  read fell back to the grid. It now opens the frontmost app by pid.
- The bench's web server script used `setsid` (absent on macOS), the bench
  saw no display or browser on a Mac, and two recorded replies used
  GNU-only flags.
- After the browser was killed, a call could wait for its timeout because
  Chrome's helpers kept the pipe open; calls now also watch the process.
- Home showed "Ctrl K" for a shortcut that is ⌘K on a Mac.
- The accessibility fill declined on TextEdit's document: text views report
  no `AXEnabled`, and the actor required one.

Measured there (run 5, all green): the tree read of a TextEdit window took
10 ms (14 nodes); Kai clicked the document and typed through the mouse and
keyboard, and filled an empty document through `AXValue` in 6.7 ms, each
read back from the tree; the app drew all 60 screens in both themes;
replay passed 18 of 19 tasks. The one failure is `dark-mode`, whose
recorded reply is the GNOME command.

From Linux, `cargo clippy --target aarch64-apple-darwin` keeps the Mac code
checked between Mac runs (see CLAUDE.md), and unit tests that run on every
OS cover the Mac logic: role mapping and naming, scroll clipping, Retina
scaling, AppleScript quoting, the Metal runtime choice and its hash,
unified-memory ranking, Homebrew on `PATH`, and the Mac prompt rule.

Not measured yet: a live model on a Mac, and a Retina display (the runner's
display is scale 1; the scaling is unit-tested). Before claiming success
numbers on macOS, run a live
`kai bench --backend gemini --model gemma-4-26b-a4b-it` A/B on a Mac.
