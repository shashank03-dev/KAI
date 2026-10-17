# Kai stack audit: macOS (Apple Silicon) and Linux, as of 2026-10-17

Scope: read-only audit of `/home/user/KAI` at HEAD `bb46617` (2026-10-17, "Fix macOS Settings crash, rebuild account connections, ship a DMG"), 56 commits in total. Sources are repository paths (relative to the repo root) and commit hashes, plus crates.io and web checks done on 2026-10-17. Line counts were measured with `wc -l` on `*.rs` files.

## 1. What the docs say that matters for choosing a stack

### Takeaway
Kai's own docs are unusually candid. The harness (kai-core) and the accessibility-first perception are well built and well tested. The cross-model and cross-OS claims are mostly unmeasured, and **no live model has ever run a task on a Mac**. The repo's latest self-assessment is 6/10 overall and 4/10 for "plug-and-play reliability with diverse open models". That is a qualitative estimate, not a measurement.

### Cited Findings
- **Platform history.** Linux and Windows were the original targets. At first, macOS was "not a release target and not claimed to be flawless". macOS support was added on 2026-10-15 at the owner's request. — [DECISIONS.md §Fixed decisions](DECISIONS.md); [DECISIONS.md §macOS on Apple Silicon (2026-10-15)](DECISIONS.md); commit `8607cc3`.
- **MACOS.md (2026-10-15/17).**
  - Requirements: Apple Silicon only, macOS 13+. Intel is unsupported because there is no Intel build of the local runtime.
  - Accessibility: an AXUIElement reader (one `AXUIElementCopyMultipleAttributeValues` call per element, 0.25 s messaging timeout, 600 ms read budget). Presses use `AXPress`. Fills set `AXValue` on an empty field. `AXManualAccessibility` is set for Electron apps. Secure fields are never filled.
  - Retina: captures are scaled from pixels to points (`frame::to_display`). Before this fix every click landed at 2x its coordinates.
  - GPUI `runtime_shaders`, so the full Xcode is not needed.
  - Signing: ad hoc only, not notarized. Permissions reset after each rebuild because the ad hoc signature changes.
  - — [docs/MACOS.md](docs/MACOS.md)
- **What MACOS.md rejected.**
  - ScreenCaptureKit: "xcap already captures through Core Graphics … a second capture path would be more code without a measured gain".
  - Developer ID signing and notarization: needs a paid account and was left as the owner's call.
  - Grid-only perception.
  - — [docs/MACOS.md §What was rejected](docs/MACOS.md)
- **Mac measurements (GitHub `macos-15` runner, 1024x768 at scale 1, run 5).**
  - AX tree read of TextEdit: 10 ms (14 nodes).
  - `AXValue` fill: 6.7 ms.
  - All 60 app screens rendered.
  - Replay: 18/19 tasks.
  - Not measured: a live model on a Mac, and a Retina display.
  - — [docs/MACOS.md §How it was checked](docs/MACOS.md)
- **MAC_GUI_TESTING.md.** Status line: "prepared, not executed". Every acceptance item is unchecked: sign-in, Retina accuracy, pause and stop, the shell refusal, and the GPT-5.6 Luna Calculator task. — [docs/MAC_GUI_TESTING.md](docs/MAC_GUI_TESTING.md)
- **ARCHITECTURE_REVIEW_2026_10_17.md.**
  - Layers: kai-core owns transports, validation, policy, execution and recording. kai-os owns perception, input, launchers, CDP and the keychain. kai-app owns the GUI.
  - On evidence: "There is no defensible percentage improvement in accuracy or speed yet". The build evidence is `cargo check`, fmt and bundle only. "Compilation and packaging are build checks, not evidence of working sign-in or desktop reliability".
  - Remaining warnings: vendored Cocoa deprecation warnings.
  - — [docs/ARCHITECTURE_REVIEW_2026_10_17.md](docs/ARCHITECTURE_REVIEW_2026_10_17.md)
- **DRIVER_MATRIX.md (2026-10-12, Linux Xvfb with AT-SPI 2.52).**
  - GTK 3.24: press, toggle and fill all work through accessibility.
  - Qt 5.15: publishes no AT-SPI tree, so Kai falls back to the grid. Recorded as a "known gap".
  - Firefox ESR 140 page buttons expose no AT-SPI action, so Kai uses the mouse. Gecko fills are declined on purpose.
  - Not covered: Electron, GTK4, WebKitGTK and Tauri.
  - — [docs/DRIVER_MATRIX.md](docs/DRIVER_MATRIX.md)
- **OPEN_MODEL_COMPUTER_USE_SCORECARD_2026_10_16.md.**
  - Scores: actions 8/10, grounding 7/10, open-model interop 4/10, safety 8/10, cross-app/OS reach 5/10, speed 6/10, external evaluation evidence 4/10.
  - Model compatibility: "Catalog entries do not imply correct model-native chat templates or action parsers". Fara1.5 and UI-TARS need native adapters.
  - — [docs/OPEN_MODEL_COMPUTER_USE_SCORECARD_2026_10_16.md](docs/OPEN_MODEL_COMPUTER_USE_SCORECARD_2026_10_16.md)
- **OPEN_MODEL_COMPUTER_USE_MILESTONES.md (2026-10-17).** Unchecked items:
  - a live baseline on the Mac;
  - authentic Fara1.5, UI-TARS and Holo3.1 checkpoint verification;
  - real coordinate calibration.
  - — [docs/OPEN_MODEL_COMPUTER_USE_MILESTONES.md](docs/OPEN_MODEL_COMPUTER_USE_MILESTONES.md)
- **CALIBRATION.md (2026-10-16).**
  - The fixture and scorer exist. "Model calibration and valid paired A/B measurements still outstanding".
  - The one Gemma A/B attempt was invalid.
  - The Holo3.1 4B download is incomplete.
  - Gemma keeps using `marks` (Set-of-Marks) by default.
  - — [docs/CALIBRATION.md](docs/CALIBRATION.md)
- **COMPUTER_TOOLS.md and CUA_NATIVE_PROTOCOL.md (2026-10-15..17).**
  - A shared, versioned action vocabulary (`--computer-tools-v1`, off by default).
  - `--oai-contract <frame>`: open models answer in OpenAI's computer-tool shape, with per-model coordinate frames `marks`, `pixels`, `norm1000` and `smart-resize`.
  - An experimental Holo3.1 native function-call transport. All of this is fixture-tested only.
  - — [docs/COMPUTER_TOOLS.md](docs/COMPUTER_TOOLS.md); [docs/CUA_NATIVE_PROTOCOL.md](docs/CUA_NATIVE_PROTOCOL.md)
- **SHIP_PLAN.md (2026-10-13).** Positions Kai's loop as OpenAI's loop made smaller for small models: marks instead of pixels, a `then` batch of 3 or fewer, a run memo, tree-only steps. It cites ComponentBench: 83.1% with tree observations vs 48.9% coordinate-only, same model. — [docs/SHIP_PLAN.md](docs/SHIP_PLAN.md)
- **RESEARCH_2026_10_VENDORS.md.**
  - Both vendors ship two surfaces: computer plus browser with accessibility refs.
  - "Background operation is now table stakes" (Codex drives macOS apps in the background).
  - Holo4-35B-A3B reports 80.8% on OSWorld (vendor-run).
  - Kai's browser lane keeps a 12-action schema.
  - — [docs/RESEARCH_2026_10_VENDORS.md §0](docs/RESEARCH_2026_10_VENDORS.md)
- **RESEARCH_2026_10_FOUNDATIONS.md.**
  - "The model is no longer the bottleneck; the harness is."
  - StructAgent's verifier-gated progress moved Qwen3.5-27B from 31.6% to 62.2% on OSWorld-Verified.
  - The "Computer-Use 2.0" pattern: window-scoped background sessions and an action ladder (accessibility action, then pixel event, then raise the window).
  - — [docs/RESEARCH_2026_10_FOUNDATIONS.md §0, §4](docs/RESEARCH_2026_10_FOUNDATIONS.md)
- **BACKGROUND.md (2026-10-12).**
  - On X11, no pointer path both works in GTK/Qt and leaves the cursor alone (`XSendEvent` is ignored, XTEST moves the cursor).
  - On Wayland, clients cannot inject input at all except through the compositor or a portal.
  - Built: window-scoped frames only (`KAI_WINDOW_SCOPE=1`, off).
  - — [docs/BACKGROUND.md](docs/BACKGROUND.md); [DECISIONS.md §Item 3 and 4](DECISIONS.md)
- **AUDIT_2026_10_15.md platform matrix.**
  - Release cases still needed: on macOS, Retina, permission revocation and a signed bundle; on Linux, GTK/Qt/Electron; on Wayland, portal deny/grant and native vs XWayland.
  - A11: capture is primary-display only, so multi-monitor operation is limited, and a negative-coordinate monitor can trigger the corner stop.
  - A12: by default "success" is the model's claim.
  - A13: permission badges can turn green without the capability being exercised.
  - A14: Wayland native input is not implemented.
  - — [docs/AUDIT_2026_10_15.md](docs/AUDIT_2026_10_15.md)
- **UI_UX_AUDIT_2026_10_16.md.**
  - Opening Model Settings read the keychain synchronously in render: 864 samples of `SecKeychainFindGenericPassword` in a 1 s macOS sample, a real UI-thread stall (a separate read was sampled at 16.7 s).
  - Navigation lacked AX roles and names.
  - Nested ScrollArea (GPUI/Ely) swallowed wheel events.
  - — [docs/UI_UX_AUDIT_2026_10_16.md](docs/UI_UX_AUDIT_2026_10_16.md); [DECISIONS.md §Transcript follow, keychain off the UI thread](DECISIONS.md)
- **LOCAL_MODELS.md.** Runtime is llama.cpp `llama-server` pinned to b11429 (2026-10-05), with GGUF and mmproj files served as OpenAI-compatible on 127.0.0.1. The macOS arm64 Metal build is hash-pinned. — [docs/LOCAL_MODELS.md](docs/LOCAL_MODELS.md); `crates/kai-os/src/local/runtime.rs:10`
- **ROADMAP.md (2026-10-10).** A phased plan of honest measurement, then a data flywheel, then SFT, then online RL, then speed distillation, toward "Gemma better than GPT-6 Astra". It is aspirational and none of it is measured. — [docs/ROADMAP.md](docs/ROADMAP.md)

### Inferences
- The docs consistently separate "built" from "measured". On macOS almost everything is "built, unit- or CI-tested, not live-measured". A stack decision should treat Kai-on-Mac as an unvalidated port of a Linux-first harness.
- The research docs point toward accessibility-first grounding, background/window-scoped operation and verifier-gated progress. Those are harness properties, mostly in kai-core and kai-os, not UI-framework properties.

### Gaps
- I skimmed DESIGN.md, MEMORY.md, DATASET.md, HUMAN_CHECKS.md, SAFETY.md, PROCEDURES.md, MCP.md and EXECUTOR.md only through DECISIONS and README references. They concern features, not the stack.
- The 4,518-line CODE_REVIEW_2026_10_16.md was not read in full.

## 2. Dependencies, maturity and crate sizes

### Takeaway
The OS layer sits on mainstream, actively maintained crates: xcap, enigo, atspi/zbus, objc2, uiautomation. The UI sits on a **fragile, third-party GPUI snapshot plus a vendored component library whose upstream disappeared**. kai-core, the OS-free harness, is by far the largest crate (~31k source lines plus ~9.7k test lines). kai-app is ~14k lines.

### Cited Findings

**Crate sizes (src / tests, lines of Rust, measured 2026-10-17)**

| Crate | src | tests | Files |
|---|---|---|---|
| kai-core | 31,374 | 9,689 | 161 |
| kai-os | 13,439 | 2,372 | 61 |
| kai-app | 14,075 | 0 | 58 |
| kai-cli | 6,677 | 0 | 26 |

- The vendored Ely component tree is 251,197 lines of Rust, counted with `find vendor -name '*.rs' | xargs cat | wc -l` over the whole `vendor/` folder (Ely is its only entry; examples included).

**Largest modules (lines)**
- kai-core: `engine/` 5,622; `client/` 3,431; `memory/` 3,261; `models/` 1,600; `oai_contract/` 1,033; `cua/` 961.
- kai-os: `web/` (own CDP browser lane) 4,341; `a11y/` 2,340; `local/` (llama.cpp runtime) 1,859.
- macOS accessibility is small: `a11y/macos.rs` 291, `macos_act.rs` 107, `macos_roles.rs` 114.

**UI framework**
- `gpui = { package = "gpui-pre", version = "=0.3.8" }` and `gpui-pre-platform =0.3.8`, pinned exactly to match Ely's pin. — [Cargo.toml](Cargo.toml); [DECISIONS.md §Fixed decisions](DECISIONS.md)
- `gpui-pre` is a third-party snapshot, not an official Zed release.
  - crates.io describes it as "Zed's GPU-accelerated UI framework (gpui-pre snapshot of zed@93d7456)". Its sole owner is user `huacnlee` (Jason Lee).
  - First published 2026-09-03, with weekly releases: 0.3.4 (09-07) through 0.3.9 (2026-10-12). Kai is already one version behind.
  - The official `gpui` crate's last release is 0.2.2, from 2025-10-22.
  - `gpui-component` (longbridge) is at 0.7.3, 2026-10-12.
  - — [crates.io API: gpui-pre](https://crates.io/api/v1/crates/gpui-pre), [owners](https://crates.io/api/v1/crates/gpui-pre/owners), [gpui](https://crates.io/api/v1/crates/gpui), [gpui-component](https://crates.io/api/v1/crates/gpui-component)
- **Ely GPUI Components.**
  - Vendored at rev `b0dbfd4` because upstream "stopped being cloneable" on 2026-10-12, which broke every fresh build.
  - It is excluded from workspace lint and test.
  - The vendor policy is "avoid local edits", but DECISIONS records an edit to the vendored splash: `SplashScreen::icon_image`, "vendored ely".
  - — [vendor/README.md](vendor/README.md); [DECISIONS.md §Ely vendored](DECISIONS.md); [DECISIONS.md §cobalt orbit icon](DECISIONS.md)

**Capture**
- `xcap` 0.9.8 (crates.io latest 0.9.8, updated 2026-08-01, about 2.4M downloads, Apache-2.0).
- On macOS, xcap 0.9.8 captures with **`CGWindowListCreateImage`** (`src/macos/capture.rs:15`, read from the crate tarball).
- That API was deprecated in macOS 14 and is "unavailable … obsoleted in macOS 15.0" when compiling against the macOS 15 SDK. Apple's replacement is ScreenCaptureKit.
- On Linux, xcap ships `wayland_capture.rs`, a portal/PipeWire path. Its README marks Wayland capture as "available, but not fully supported in some special scenarios".
- — [crates.io xcap](https://crates.io/api/v1/crates/xcap); [xcap README](https://github.com/nashaofu/xcap); [eclipsa-audio-plugin #10](https://github.com/google/eclipsa-audio-plugin/issues/10); [sentry-unreal #1126](https://github.com/getsentry/sentry-unreal/issues/1126)

**Input and accessibility libraries**
- `enigo` 0.6.1, last release 2025-08-28, about 3.6M downloads. Uses `CGEventPost` on macOS and XTEST on X11. — [crates.io enigo](https://crates.io/api/v1/crates/enigo); [docs/MACOS.md](docs/MACOS.md); [docs/BACKGROUND.md](docs/BACKGROUND.md)
- `atspi` 0.30.0 (2026-05-06, about 15M downloads, odilia-app) with `zbus` 5.19.0. The lockfile also carries atspi 0.29 and zbus 4.4 as transitive duplicates. — [crates.io atspi](https://crates.io/api/v1/crates/atspi); Cargo.lock
- macOS: `objc2-application-services`, `objc2-core-foundation`, `objc2-core-graphics` and `objc2-app-kit` 0.3.2 (AX tree, CGWindow).
- Windows: `uiautomation` 0.25.1 and `windows` 0.62. — [crates/kai-os/Cargo.toml](crates/kai-os/Cargo.toml)

**Other kai-os dependencies**
- `global-hotkey` 0.8 (kill switch), `arboard` 3.6.1, `keyring` 3.6.3 (apple-native, linux-native-sync-persistent), `rquickjs` 0.14 (browser-script worker), `reqwest` 0.12 with rustls and native roots. — [Cargo.toml](Cargo.toml)
- **Browser (CDP):** there is no third-party CDP crate. Kai implements CDP itself over `--remote-debugging-pipe` in `kai-os/src/web/` (4,341 lines). It is unavailable on Windows. — [docs/MACOS.md](docs/MACOS.md); [docs/AUDIT_2026_10_15.md A14](docs/AUDIT_2026_10_15.md)
- **Local LLM runtime:** an external `llama-server` binary (llama.cpp b11429), downloaded and SHA-pinned, not linked as a library. — [docs/LOCAL_MODELS.md](docs/LOCAL_MODELS.md)
- **Evaluated and rejected:** `rustautogui` 2.5.
  - It panics without an X display and captures X11 only.
  - It has 27 `println!` calls that would corrupt the MCP stdout.
  - It is FFT-heavy and not `Send`.
  - — [DECISIONS.md §rustautogui evaluated, not added](DECISIONS.md)
- The lockfile has 958 packages. — Cargo.lock

### Inferences
- The highest-risk dependency for the Mac and Linux desktop app is the GPUI path:
  - a single-maintainer weekly snapshot crate, pinned with `=`;
  - a vendored, unmaintained component library of about 250k lines;
  - a recorded crash caused by an Ely `Select` assertion (section 4).
- xcap's macOS path rests on an API Apple has obsoleted in the macOS 15 SDK. Kai's runtime tests on macos-15 passed (section 4), so it still works at runtime for now. But this is a known deprecation risk, and MACOS.md's "ScreenCaptureKit rejected" rationale predates any assessment of it.
- Every OS-layer crate is mainstream. Replacing the UI would not force replacing kai-os.

### Gaps
- Not checked: whether `objc2-core-graphics` still exposes `CGWindowListCreateImage` when linked against newer SDKs, or the runtime behaviour on macOS 26. No Mac was available.
- The internal maturity of the vendored Ely tree was not checked: test coverage and how many of its ~250k lines kai-app actually uses.

## 3. Benchmarks and perception approach

### Takeaway
All live-model numbers come from a **Linux container** (Xvfb, 4 vCPU, no GPU) on 2026-10-10..14, mostly Gemma 4 26B-A4B through the Gemini API, with one or two runs per side and Kai's own 14-30 local tasks. There are **no OSWorld scores, no Mac live numbers and no GPU local-model numbers**. Perception is Set-of-Marks drawn from the accessibility tree, with a numbered-grid fallback, tree-only steps, and (experimental) unmarked pixel frames for models trained on coordinates.

### Cited Findings

**Head-to-head, 2026-10-10** (14 tasks, one run each, Gemini or Vercel AI Gateway free tiers)

| Model | Passed | Steps | Model p50 |
|---|---|---|---|
| Gemma 4 26B-A4B | 14/14 | 75 | 2.85 s |
| Gemma 4 31B | 11/14 | — | 27.2 s |
| GPT-5 mini | 12/14 | 212 | — |
| GPT-4.1 mini | 8/14 | — | — |

- Gemma 26B on the full 30-task set: 23/30.
- — [README.md §Head-to-head](README.md); `bench/results/2026-10-10-*.json`

**Live re-run, 2026-10-12** (Gemma 26B, 14 tasks, two runs per side)

| Config | Passed | Steps per run | Model calls | Input tokens |
|---|---|---|---|---|
| Old harness | 28/28 | 120 | 143 | 304k |
| New harness | 27/28 | 90 | 94 | 194k |

- Of the experimental switches (`--outcomes`, `--macro-history`, `--recover`, window scope, best-of-3), none earned a default.
- — [bench/results/2026-10-12-live.md](bench/results/2026-10-12-live.md)

**Browser lane vs desktop, 2026-10-13** (4 web tasks, two runs per side)

| | Passed | Steps | Prompt tokens | Wall time |
|---|---|---|---|---|
| Lane (CDP, headless Chromium 141) | 8/8 | 38 | 85k | 178 s |
| Desktop (Firefox, AT-SPI) | 7/8 | 70 | 162k | 409 s |

- — [bench/results/2026-10-13-browser-ab.md](bench/results/2026-10-13-browser-ab.md)

**Local model on CPU, 2026-10-13**
- Holo3.1 4B Q4 on 4 vCPU without a GPU: 8/14, then 6/14 (noise, one run each).
- About 60 s per model call, 6–11 minutes per web task. Rated "too slow to be pleasant".
- — [bench/results/2026-10-13-local.md](bench/results/2026-10-13-local.md)

**Real-world tasks, 2026-10-12/13** (independently checked)
- Several false-done results: t1, t2, and a Team `du` run.
- Mousepad Find and Replace failed in 6 attempts.
- The safety probes held.
- — [bench/results/2026-10-13-real-world.md](bench/results/2026-10-13-real-world.md)

**Memo and task-checks A/B, 2026-10-14**
- Raw results only, each round 14/18. — commits `4372bc1`, `17c81a4`, `d5a6418`, `140c35b`, `f131cb9`; `bench/results/2026-10-13-live/`

**Executor A/B, 2026-10-10**
- Gemma alone 13/14 vs Gemma + jevos 11/14. No benefit. — [bench/results/2026-10-10-executor-ab.md](bench/results/2026-10-10-executor-ab.md)

**Harness overhead (Linux container)**
- Per step, model excluded: 65 ms p50 / 93 ms p95.
- Observe: about 200 ms (capture 19–28 ms in parallel with the AT-SPI tree read at 136–198 ms, then encode 41–52 ms).
- The tree read has a hard 600 ms budget, then falls back to the grid.
- — [README.md](README.md); [DECISIONS.md §Phase 2b](DECISIONS.md)

**Replay on non-Linux runners**
- Windows (real runner): 10/19. Seven failures are Linux-shell recorded replies. — [DECISIONS.md §Windows for real](DECISIONS.md)
- macOS (macos-15 runner): 18/19. — [docs/MACOS.md](docs/MACOS.md)

**Perception approach**
- Accessibility tree, numbered Set-of-Marks overlay, and an integer `target`.
- A numbered grid when the tree is missing or too slow.
- Tree-only steps when text suffices.
- Screenshots downscaled to Gemma's 280-token budget (1070x602).
- Kai's own windows are masked: `WDA_EXCLUDEFROMCAPTURE` on Windows; painted over on X11 and macOS.
- `--oai-contract` adds `pixels`, `norm1000` and `smart-resize` frames on an unmarked capture of up to 1600 px, plus an optional `--grounder` model.
- — [README.md](README.md); [docs/HARNESS.md](docs/HARNESS.md); [DECISIONS.md §Phase 2b](DECISIONS.md); [docs/COMPUTER_TOOLS.md](docs/COMPUTER_TOOLS.md)

### Inferences
- The most robust measured gains came from harness fixes and from the CDP browser lane, not from model swaps. The browser lane roughly halved steps and tokens.
- The perception design (a11y marks) is validated on Linux GTK and the Gemma combination only. On macOS the AX path was exercised only in TextEdit by a scripted check, not by a model.

### Gaps
- No OSWorld or other external benchmark exists.
- No measured coordinate calibration for any pixel-native model.
- No GPU local-model numbers.
- No live run on macOS or on Wayland.

## 4. Mac-specific bugs, crashes and workarounds (git history)

### Takeaway
Each real-Mac run found bugs that Linux cross-checks could not. The one recorded app crash is a GPUI/Ely component assertion. The recurring macOS frictions are permissions tied to the ad hoc signature, Retina scaling, the main-thread requirement for hotkeys, Metal shader compilation, and keychain calls blocking the UI thread.

### Cited Findings

**Introduction of macOS support**
- `8607cc3` (2026-10-15) introduced it:
  - AX reader and actor;
  - Retina point scaling;
  - permission checks (`CGPreflightScreenCaptureAccess`);
  - GPUI `runtime_shaders` (no full Xcode);
  - stop from the CLI is the screen corner, because global hotkeys need the app's main-thread event loop;
  - osascript notifications;
  - Homebrew on PATH for Finder launches.
  - — commit `8607cc3`; [docs/MACOS.md](docs/MACOS.md)

**First real-Mac run fixes**
- `e249482`: the system-wide `AXFocusedApplication` query failed even though Kai was trusted. Every read silently fell back to the grid. It is now read by the frontmost app's pid.
- Also in `e249482`: `setsid` is missing on macOS, and two recorded replies used GNU-only flags.
- `24dcd4a`: shortcuts moved to cmd, and a Quit menu was added. The bench saw no display or browser on a Mac.
- `d67e523`: Macs were labelled "linux" in traces and in the CUA client.
- `b5b45b1`: the UI showed "Ctrl K" instead of ⌘K.
- `6a965ae`: TextEdit's NSTextView reports no `AXEnabled`, so the fill declined.
- `fe92995` and `1bc97a6`: a killed Chrome kept the pipe open, so calls hung until timeout. A busy Mac also shut Chrome down slowly (test race).
- — git log

**Crash**
- `bb46617` (2026-10-17): the app **aborted** whenever Settings opened with Codex or Claude Code selected and no catalog fetched.
- Cause: Ely's `Select` asserts on zero choices ("select Name(\"fast\") has no choices").
- Fixed by disabling empty pickers, and connections were rebuilt as a state machine.
- Still unfixed: four `window not found` log lines when the splash window closes.
- — commit `bb46617`; [DECISIONS.md §Mac crash](DECISIONS.md)

**UI-thread stall**
- The keychain was read synchronously during GPUI render. One read was sampled at 16.7 s on macOS. All reads moved to `kai-app/src/keys.rs` on the agent runtime, with a source guard test.
- — [DECISIONS.md §Transcript follow, keychain off the UI thread](DECISIONS.md); [docs/UI_UX_AUDIT_2026_10_16.md U06](docs/UI_UX_AUDIT_2026_10_16.md)

**Linux GPUI issue**
- `kai-app` built `gpui_platform` without the `x11` and `wayland` features. The earlier Linux builds "never showed a window at all".
- — [DECISIONS.md §Screenshots — captured](DECISIONS.md)

**Permissions**
- macOS ties permissions to the code signature, and the ad hoc signature changes with every build.
- After a rebuild the switches can show "on" while access is denied. Users must toggle Kai off and on, or remove and re-add it.
- The Screen Recording prompt shows once per app, and Kai must be reopened after granting it.
- — [docs/MACOS.md §Permissions](docs/MACOS.md)

**CI posture**
- The macOS check is "manual only, like the Windows check". The main CI is paused.
- — commit `6b1c292`; CLAUDE.md rule 5

### Inferences
- About 40% of Mac bugs were in kai-os/kai-core OS glue (AX, process and pipe handling, shell portability). The UI-side defects were the crash, the hotkey and menu conventions, and the keychain-in-render stall.
- The crash class (component assertions on data states) and render-thread blocking would follow any GPUI/Ely UI, because the component library is unmaintained upstream.

### Gaps
- Not checked: crash reports from real user Macs, behaviour on macOS 26, multi-display or Retina live runs, Gatekeeper behaviour for the DMG.

## 5. OS-independent, reusable parts and coupling of kai-app

### Takeaway
kai-core (~31k lines, no OS or UI dependencies) and kai-os (~13k lines, no UI dependency) are reusable under any UI framework, and kai-cli already drives both without a GUI. kai-app is widely but shallowly coupled to kai-core: 163 references, dominated by `settings`, plus 40 references into kai-os. Nearly every kai-app file also uses GPUI/Ely, so the UI layer itself would be rewritten, not ported.

### Cited Findings

**kai-core is OS- and UI-free**
- Its dependencies are only anyhow, serde, reqwest, tokio, regex, url, psl, async-trait and similar. No GPUI and no OS crates. — [crates/kai-core/Cargo.toml](crates/kai-core/Cargo.toml)
- OS access goes only through the traits in `ports.rs`: `Perceptor`, `Probe`, `Actuator`, `Steering`, `Verifier`, `Approver`, `Interrupt`, `EventSink`. — `crates/kai-core/src/ports.rs` (lines 49–296); CLAUDE.md

**kai-os depends on kai-core but not on any UI crate**
- It has 25 `cfg(target_os = "macos")` gates. — [crates/kai-os/Cargo.toml](crates/kai-os/Cargo.toml); grep

**Coupling of kai-app (grep of `crates/kai-app/src`, 2026-10-17)**
- **163 `kai_core::` references** in 44 of 57 files. Top modules:

  | Module | References |
  |---|---|
  | `settings` | 41 |
  | `human` | 10 |
  | `skill` / `Skill` | 9 / 7 |
  | `Outcome` | 9 |
  | `trace` / `Trace` | 8 / 5 |
  | `orchestrator` | 8 |
  | `engine` | 6 |
  | `risk`, `models`, `connections`, `Decision` | 5 each |
  | `catalog`, `Action` | 4 each |

- **40 `kai_os::` references.** Top modules:

  | Module | References |
  |---|---|
  | `secrets` | 12 |
  | `local` | 10 |
  | `permissions` | 6 |
  | `kill` | 3 |
  | `frame` | 2 |
  | `web`, `notify`, `exclude`, `DesktopActuator`, `DesktopProbe`, `KillSwitch` | 1 each |

- **43 of 57 files use `gpui::` or Ely.**
- Engine bridge: the largest kai-app files are `agent.rs` (560 lines, the async bridge to the engine), `state.rs` (471) and `runs.rs` (471).
- — grep counts; [docs/ARCHITECTURE_REVIEW_2026_10_17.md](docs/ARCHITECTURE_REVIEW_2026_10_17.md) ("kai-app owns the native GUI and asynchronous bridge to the engine")

**Other front ends**
- `kai mcp` exposes `kai_observe`, `kai_act`, `kai_probe` and `kai_run_task`, so external agents (Claude Code, Codex) can use Kai's eyes and hands without the GUI. — [docs/OPEN_MODEL_COMPUTER_USE_SCORECARD_2026_10_16.md](docs/OPEN_MODEL_COMPUTER_USE_SCORECARD_2026_10_16.md); [docs/MCP.md](docs/MCP.md)

### Inferences
- **Keep regardless of UI:** kai-core (engine loop, schema, risk/safety, prompts, model clients, catalog, memory, procedures, ledger, CUA/OAI-contract adapters, MCP) and kai-os (capture, AX/AT-SPI/UIA, marks, input, kill switch, keychain, CDP lane, llama.cpp runtime).
- **A UI replacement would touch:** about 14k lines of GPUI/Ely code. Its integration surface is mostly `kai_core::settings`, plus trace/outcome/skill types, the orchestrator and engine, and kai-os secrets, permissions and local runtime calls. That surface could be wrapped in a thin façade or an IPC API (for example a Tauri or Swift front end over kai-core) without touching the harness.
- `settings` (41 references) is the tightest seam. A UI swap would want a stable settings and run-event API first.

### Gaps
- No precise list of the public functions kai-app calls; the counts are path-prefix references, not unique call sites.
- Not checked: whether `agent.rs` uses GPUI types in its engine bridge, which would affect how cleanly it lifts out.

## 6. What is missing for macOS and Linux

### Takeaway
The macOS gaps are distribution (Developer ID and notarization), a capture path on a deprecated API (no ScreenCaptureKit), no background or own-cursor operation, no live-model validation, and unverified Retina and multi-monitor behaviour. Linux is X11-centric: there is no native Wayland input or global hotkey, Qt5 trees are missing, capture is primary-display only, and nothing has run outside an Xvfb container.

### Cited Findings

**macOS**
- No Developer ID or notarization (ad hoc, quarantine workaround). — [docs/MACOS.md](docs/MACOS.md)
- ScreenCaptureKit was not adopted, and xcap uses `CGWindowListCreateImage` (obsoleted in the macOS 15 SDK). — [docs/MACOS.md](docs/MACOS.md); xcap 0.9.8 source; [eclipsa #10](https://github.com/google/eclipsa-audio-plugin/issues/10)
- The global stop hotkey works only in the app; the CLI uses the screen corner instead. — [docs/MACOS.md](docs/MACOS.md)
- No live model run, no Retina run, and the GUI testing handoff is not executed. — [docs/MACOS.md](docs/MACOS.md); [docs/MAC_GUI_TESTING.md](docs/MAC_GUI_TESTING.md)
- No background or own-cursor driving like Codex's macOS mode. Only window-scoped frames are built, and they are off by default. — [docs/RESEARCH_2026_10_VENDORS.md](docs/RESEARCH_2026_10_VENDORS.md); [docs/BACKGROUND.md](docs/BACKGROUND.md)
- Intel Macs are unsupported. — [docs/MACOS.md](docs/MACOS.md)

**Linux**
- Wayland: capture works through the portal (one prompt per session), but synthetic input reaches only XWayland apps. The stop shortcut fires only while an XWayland window has focus. Onboarding says "Limited" and recommends Xorg. — [DECISIONS.md §Phase 2b](DECISIONS.md); `crates/kai-os/src/permissions.rs:28-48`; `crates/kai-os/src/kill.rs:67`
- No libei or RemoteDesktop-portal input. — [docs/AUDIT_2026_10_15.md A14](docs/AUDIT_2026_10_15.md)
- Qt 5 apps publish no AT-SPI tree in testing. GTK4, Electron and Tauri rows are untested. — [docs/DRIVER_MATRIX.md](docs/DRIVER_MATRIX.md)

**Both platforms**
- Capture is primary-display only, and permission badges are not exercised by a real operation. — [docs/AUDIT_2026_10_15.md A11, A13](docs/AUDIT_2026_10_15.md)
- Default success is the model's own claim; the DONE gate and task checks are off by default. — [docs/AUDIT_2026_10_15.md A12](docs/AUDIT_2026_10_15.md)
- CI is paused, and the Mac and Windows workflows are manual. — CLAUDE.md; commit `6b1c292`

### Inferences
- For a macOS-first stack these are the gaps that matter most, in this order:
  1. a ScreenCaptureKit-based capture, which is also the basis for per-window and background capture;
  2. notarized signing, which also stabilizes TCC permissions across builds;
  3. a live-model acceptance run on a Retina Mac.
- For Linux, the decisive gap is Wayland input. Any replacement OS layer would need libei or the RemoteDesktop portal; xcap, enigo and global-hotkey do not solve it.

### Gaps
- Not checked: whether enigo 0.6 or another Rust crate offers libei or RemoteDesktop-portal input today.
- I did not research ScreenCaptureKit Rust bindings (for example `screencapturekit` crates) or their maturity in this pass.
