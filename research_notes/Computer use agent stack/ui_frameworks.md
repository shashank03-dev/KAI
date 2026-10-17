# Desktop UI frameworks for a computer-use agent app (macOS Apple Silicon + Linux), as of 2026-10-17

Scope: Tauri 2, Electron, SwiftUI/AppKit (+ Rust core), GPUI (Zed), Qt / cxx-qt, Flutter, Slint, Dioxus, egui / iced. Windows out of scope.
Method: web search + fetch (2026-10-17), plus live crates.io API queries on 2026-10-17 for exact version numbers and dates (most reliable data here). Research budget was ~18 tool calls, so several frameworks (Flutter, iced, egui details) are thin; see Gaps.

Context from this repo (Kai): `kai-app` already uses GPUI via the crates.io snapshot `gpui-pre`/`gpui-pre-platform` pinned at `=0.3.8` plus a vendored Ely GPUI component library (`/home/user/KAI/Cargo.toml` lines 44-49; `DECISIONS.md` line 27-31). `DECISIONS.md` records two GPUI traps already hit: building `gpui_platform` without `x11`/`wayland` features gives a headless-only Linux platform, and a macOS build needs `runtime_shaders` (DECISIONS.md ~line 229, ~line 917).

## Maturity and stability on macOS and Linux in 2026 (incl. GPUI outside Zed)

### Takeaway
Tauri 2 (2.12.x, weekly-ish patch releases) and Electron are the mature, stable options; Slint is a stable 1.x; cxx-qt and Dioxus are still pre-1.0 with frequent breaking releases. GPUI has no stable standalone release: the official `gpui` crate on crates.io is frozen at 0.2.2 (Oct 2025), and outside users consume weekly Zed snapshots (`gpui-pre`) or community forks, with gpui-component (Longbridge) moving fast (0.7.3, Oct 2026).

### Cited Findings
- crates.io versions/dates queried 2026-10-17:
  - `tauri`: stable 2.12.4 (2026-10-14), 2.12.3 (10-10), 2.12.2 (10-09), 2.12.1 (09-30); a **3.0.0-alpha.5** was published 2026-10-14 (alpha.4 on 10-01) — [crates.io tauri](https://crates.io/crates/tauri). No official Tauri 3 announcement was found by search — [Tauri blog search result](https://v2.tauri.app/blog/authors/amr-bashir).
  - Tauri 2.11.0 was 2026-04-30, 2.11.x patches through 2.11.5 (2026-07-01); 2.11.1-era security fix: ACL checks now apply to IPC requests from remote origins even without an AppManifest — [Tauri release notes](https://v2.tauri.app/release/tauri/).
  - `gpui` (official): max version **0.2.2, published 2025-10-22**; earlier 0.2.0 2025-10-09; nothing newer in 2026 — [crates.io gpui](https://crates.io/crates/gpui).
  - `gpui-pre` / `gpui-pre-platform`: "Zed's GPU-accelerated UI framework (gpui-pre snapshot of zed@93d7456)", repository zed-industries/zed, weekly releases 0.3.5 (2026-09-14) … 0.3.9 (2026-10-12), ~176k downloads — [crates.io gpui-pre](https://crates.io/crates/gpui-pre).
  - `gpui-component`: 0.7.3 (2026-10-12), 0.7.0 (2026-09-28), 0.6.6 (2026-09-21) — very fast cadence — [crates.io gpui-component](https://crates.io/crates/gpui-component).
  - Community forks: `gpui-ce` (community edition, 0.3.3 Dec 2025, 0.2.2 Aug 2026) — [crates.io gpui-ce](https://crates.io/crates/gpui-ce); `gpui-box` "independent distribution derived from Zed's GPUI… not an official Zed project… pre-1.0 and may make breaking changes", requires Rust 1.97+ — [docs.rs gpui-box](https://docs.rs/crate/gpui-box/0.1.1).
  - `slint`: 1.18.1 (2026-09-21) — [crates.io slint](https://crates.io/crates/slint); Slint promises a stable 1.x API "without breaking your code" and supports OSes supported by their vendors — [Slint desktop docs](https://docs.slint.dev/latest/docs/slint/guide/platforms/desktop/).
  - `cxx-qt`: 0.10.0 (2026-08-24), 0.9.0 (2026-06-23), 0.8.0 (2025-12-18) — three minor (breaking, pre-1.0) releases in ~8 months — [crates.io cxx-qt](https://crates.io/crates/cxx-qt); historically "API changes frequently" — [cxx-qt mirror README](https://gitee.com/freecamle/cxx-qt); 0.6 had many breaking changes — [KDAB cxx-qt releases](https://github.com/KDAB/cxx-qt/releases).
  - `dioxus`: 0.7.10 stable (2026-07-30), 0.8.0-alpha.3 (2026-10-16) — [crates.io dioxus](https://crates.io/crates/dioxus); 0.7 released 2025-09-08 with hot-patching and a native renderer — [Dioxus blog](https://dioxuslabs.com/blog/).
  - `iced`: 0.14.0 (2025-12-07), previous 0.13 (Sept 2024) — roughly yearly breaking releases — [crates.io iced](https://crates.io/crates/iced).
  - `egui`: 0.37.0 (2026-10-15) — [crates.io egui](https://crates.io/crates/egui).
- Zed's April 27, 2026 post calls GPUI "a young framework" that ships no component library (hence gpui-component), says "a growing number of projects are building with the gpui/gpui-component combination" (links an awesome-gpui list), and that Longbridge chose gpui to rewrite its desktop client — [Zed blog: Community Champion Jason Lee](https://zed.dev/blog/community-champion-jason-lee).
- gpui-component README: powers Longbridge Pro, 60+ components; install snippet pulls gpui from Git — [longbridge/gpui-component](https://github.com/longbridge/gpui-component).
- Zed itself hit 1.0 (GPUI-based) — [Linuxiac](https://linuxiac.com/zed-code-editor-hits-1-0-with-gpu-accelerated-ui/). Zed preview 1.18.0 notes list GPUI fixes including an X11 window-close panic and a Wayland fullscreen freeze — [Zed preview 1.18.0](https://zed.dev/releases/preview/1.18.0).
- Electron: 38.2+ supports Wayland out of the box on Linux following Chromium's switch — [Electron blog: Tech Talk Wayland](https://electronjs.org/blog/tech-talk-wayland). Electron 39 (Chromium 142, ASAR integrity stable), 40 (Chromium 144, Node 24), 41 (Chromium 146; macOS ASAR integrity digest) are out — [Electron blog](https://www.electronjs.org/blog).

### Inferences
- GPUI's stability risk is structural: there is no semver'd API, every weekly `gpui-pre` snapshot is a slice of Zed's main branch, and component libraries (gpui-component, Ely) pin specific snapshots, so upgrading GPUI means upgrading the component library in lockstep (Kai already pins both, per DECISIONS.md). Expect API churn per upgrade, but the renderer itself is battle-tested by Zed on macOS and Linux.
- The presence of 3.0.0 alphas suggests a Tauri 3 is in progress; Tauri 2 remains the production line as of Oct 2026.

### Gaps
- Flutter desktop 2026 status (release cadence, macOS/Linux maturity) was not researched within budget.
- No independent third-party write-ups on GPUI-outside-Zed pain points were found; the evidence is crate metadata plus Zed's own post.
- Electron exact 2026 release dates not confirmed (8-week cadence is common knowledge but not verified here).
- Qt 6 itself (6.9/6.10 release status) not checked.

## Agent-specific features (overlay, click-through, capture exclusion, hotkeys, tray, notifications, multi-window, a11y)

### Takeaway
Electron and Tauri expose the agent-relevant window knobs directly (always-on-top, content protection = macOS `sharingType = none`, ignore-cursor-events, transparency); native AppKit gives all of them by definition. GPUI/Slint/iced/egui need platform code (objc2 / raw window handle) for capture exclusion and click-through. Crucially, `sharingType = none` is not a reliable exclusion against ScreenCaptureKit, so an agent should also filter its own windows in its capture pipeline (SCContentFilter excluding own app/windows).

### Cited Findings
- Tauri v2 JS API: `setAlwaysOnTop(alwaysOnTop)` and `setContentProtected(protected_)` ("Prevents the window contents from being captured by other apps"); both require explicit permissions (`core:window:allow-set-always-on-top`, `core:window:allow-set-content-protected`) not in `core:window:default`. The API also lists `setIgnoreCursorEvents`, `setVisibleOnAllWorkspaces`, `setSkipTaskbar`, `setFocusable` (details not read) — [Tauri window API](https://v2.tauri.app/reference/javascript/api/namespacewindow/).
- Electron `setContentProtection` on macOS sets NSWindow `sharingType` to `NSWindowSharingNone`; on Windows uses `WDA_EXCLUDEFROMCAPTURE` — [Electron issue #19880](https://github.com/electron/electron/issues/19880); [Electron docs commit](https://ayakael.net/mirrors/electron/commit/44bd560068d545a51c8300a0255983c90a0dc3e7).
- Cap (a Tauri screen recorder) excludes windows from recordings on macOS for any window — [Cap PR #1179](https://github.com/CapSoftware/Cap/pull/1179).
- `sharingType = none` is unreliable with modern capture: Apple forum thread on ScreenCaptureKit initially omitting a sharing-none app then showing it after options changed — [Apple Developer Forums 808016](https://developer.apple.com/forums/thread/808016); Feb 2025 forum post: excluded from screenshots but not from screen recording — [Apple Developer Forums](https://origin-devforums.apple.com/forums/profile/learnyst); OBS display capture still recorded such a window — [OBS forum](https://obsproject.com/forum/threads/obs-and-dlp-on-macos.72643/latest); behaviour changed across macOS releases — [gitconnected blog](https://levelup.gitconnected.com/how-i-made-a-desktop-app-invisible-to-screen-sharing-electron-os-level-tricks-5734513c1e67).

### Inferences
- For an agent that screenshots the desktop, the right design is "exclude own windows at capture time" (ScreenCaptureKit `SCContentFilter` with `excludingApplications`/`excludingWindows` on macOS; on Linux X11 hide/unmap or crop; Wayland portals offer no per-window exclusion), and treat `sharingType` only as a hint. This is framework-independent, so it does not favour Electron/Tauri much.
- Click-through overlays: Electron (`setIgnoreMouseEvents`) and Tauri (`setIgnoreCursorEvents`) expose it; with GPUI, Slint, iced, egui you would set `NSWindow.ignoresMouseEvents` via objc2 on macOS and X11 input shape / Wayland empty input region on Linux yourself. On Wayland, always-on-top and global positioning are compositor-controlled (layer-shell only on wlroots/KDE), regardless of framework.
- Global hotkeys and tray: Tauri has official plugins (global-shortcut, tray in core); Electron has `globalShortcut`/`Tray`; Rust-native toolkits typically use the `global-hotkey`/`tray-icon` crates (from the Tauri org). Not verified in this session — see Gaps.

### Gaps
- Did not verify Tauri's per-platform caveats (e.g., `setIgnoreCursorEvents`, transparency requiring `macOSPrivateApi` on macOS — App Store incompatible) — page truncated.
- GPUI's built-in support for window kinds (PopUp/floating), transparency and click-through not verified from source in this session (check `gpui-pre` `WindowKind`, `WindowBackgroundAppearance`).
- Accessibility of own UI: not researched. Known to be a weak point of GPU-drawn toolkits (GPUI, iced, egui partially via AccessKit; Slint has accessibility support) but unverified here.
- Global hotkeys on Wayland (needs xdg-desktop-portal GlobalShortcuts) not verified.

## Integration with a Rust core engine (in-process / sidecar / FFI) and Python

### Takeaway
Rust-native toolkits (GPUI, Slint, Dioxus, iced, egui, Tauri backend) run the Rust core in-process with no FFI; Tauri adds a JS/web frontend over typed IPC commands; Electron needs a sidecar process or N-API (napi-rs); SwiftUI/AppKit needs UniFFI or a C ABI; Qt via cxx-qt bridges Rust QObjects to QML. Python is best kept as a sidecar process in every option.

### Cited Findings
- Tauri apps call Rust via `#[tauri::command]` (2.11 added a `rename` attribute) and IPC is ACL-gated — [Tauri release notes](https://v2.tauri.app/release/tauri/).
- Screenpipe's desktop app is "Tauri (Rust backend) + Next.js (React frontend)" around its Rust capture engine — [Screenpipe architecture docs](https://docs.screenpi.pe/architecture).
- cxx-qt: Rust crates for bidirectional Rust/C++ bindings with Qt, usable from CMake or Cargo — [KDAB](https://www.kdab.com/authors/leon-matthes/).
- UI-TARS Desktop is a pnpm/Turborepo monorepo; Electron app captures screenshots, sends to the model, and executes actions via mouse/keyboard events — [pyshine](https://pyshine.com/UI-TARS-Desktop-ByteDance-Multimodal-AI-Agent/); [themenonlab](https://themenonlab.blog/blog/ui-tars-desktop-open-source-gui-agent).

### Inferences
- For Kai (Rust core, `kai-core` OS-free behind ports), in-process GPUI keeps one language, one binary, no IPC schema; a Tauri port would mean re-implementing the UI in TS and serialising engine events over IPC; SwiftUI would mean a UniFFI layer plus a separate Linux UI (no SwiftUI on Linux), i.e., two UIs.

### Gaps
- No measured IPC overhead numbers for Tauri vs in-process were found.

## Bundling, signing, notarization, auto-update, packaging

### Takeaway
Electron (electron-builder / Forge) and Tauri (tauri-bundler + updater plugin) have turnkey DMG/notarization/AppImage/deb/rpm + auto-update. Rust-native toolkits (GPUI, Slint, iced, egui) need `cargo-bundle`/`cargo-packager` or hand-rolled scripts plus a separate updater. Electron 41 adds macOS ASAR integrity digests.

### Cited Findings
- Electron 39 made ASAR integrity stable; Electron 41 lets macOS apps embed an ASAR integrity digest — [Electron blog](https://www.electronjs.org/blog).
- Tauri 2.11 added file-association support for mobile; Windows version strings fixed (packaging metadata work continues) — [Tauri release notes](https://v2.tauri.app/release/tauri/).
- UI-TARS Desktop ships as downloadable Electron builds from GitHub releases — [themenonlab](https://themenonlab.blog/blog/ui-tars-desktop-open-source-gui-agent).

### Inferences
- Zed (GPUI) ships signed/notarized DMGs and Linux tarballs with its own updater, proving it is doable but custom work; Kai would own that pipeline.

### Gaps
- Did not verify Tauri bundler's current Flatpak support, `cargo-packager` status, or Slint/GPUI packaging tooling. Did not check Flutter's `flutter build macos/linux` packaging.

## Bundle size, memory, startup; Linux WebKitGTK and Wayland

### Takeaway
Electron bundles Chromium (large: Notion's Electron Mac app is 518 MB per Gruber), Tauri uses the system webview (small, but on Linux that is WebKitGTK, with well-documented NVIDIA/DMABUF rendering bugs and slow WebGL paths). Native GPU toolkits (GPUI, Slint, iced, egui) avoid webviews entirely. Electron now runs native Wayland by default (38.2+).

### Cited Findings
- Tauri's own docs: WebKitGTK and GPU drivers (most often NVIDIA) cause blank/white windows, resize flicker and crashes, linked to the DMABUF renderer; workarounds `WEBKIT_DISABLE_DMABUF_RENDERER=1`, `WEBKIT_DISABLE_COMPOSITING_MODE=1` (last resort), `__NV_DISABLE_EXPLICIT_SYNC=1`; WebGL can silently drop to a slow path — [Tauri: Linux Graphics Issues](https://v2.tauri.app/develop/debug/linux-graphics/) (tracking issue tauri-apps/tauri#9394).
- A Tauri app (GeoLibre) reports jank from WebKitGTK doing raster tiles synchronously on the UI thread (anecdotal) — [GeoLibre debugging guide](https://instagit.com/opengeos/GeoLibre/how-to-debug-webkitgtk-performance-linux/).
- Electron 38.2+ supports Wayland out of the box — [Electron blog](https://electronjs.org/blog/tech-talk-wayland).
- Notion's Electron Mac app is 518 MB; Apple cited Notion at WWDC 2026 as migrating its interface to SwiftUI — [Daring Fireball, 2026-07-03](https://daringfireball.net/2026/07/claudes_criminally_bad_mac_app_is_an_inside_job).
- Claude's Mac app (Electron) "can use more memory and battery than a native app" (reviewer opinion) — [eesel.ai](https://www.eesel.ai/blog/claude-ai-mac-apps).
- Zed's Linux port uses Vulkan; Linux fragmentation slowed GPUI's port (2024) — [Phoronix](https://phoronix.com/news/Zed-Code-Editor-Linux-Progress).

### Inferences
- For an agent that must stay responsive while capturing screens and running models, a webview on Linux (WebKitGTK) is the biggest technical risk among the options; a GPU-native Rust UI or Electron (bundled Chromium) avoid it.

### Gaps
- No controlled 2025-2026 benchmark comparing bundle size/RAM/startup across all frameworks was found; avoid quoting generic "Tauri 3 MB vs Electron 150 MB" numbers without a dated source.

## Which real AI desktop apps use each, and why

### Takeaway
Electron dominates AI chat/agent clients (Claude desktop, UI-TARS Desktop); OpenAI's ChatGPT Mac app is native Swift; Rust/Tauri is chosen by local-first, capture-heavy apps (Screenpipe, Jan's rebuild, Cap); GPUI powers Zed and Longbridge Pro.

### Cited Findings
- Claude desktop: Electron since Oct 2024 and still in 2026; Anthropic's Boris Cherny (HN) gave reasons: engineers with Electron background, shared code keeps web/desktop consistent, "Claude is good at building with Electron", and said this may change. Felix Rieseberg (Electron co-creator) leads Claude Cowork / Claude Code Desktop — [Daring Fireball, 2026-07-03](https://daringfireball.net/2026/07/claudes_criminally_bad_mac_app_is_an_inside_job).
- ChatGPT for Mac is a native Swift app, giving performance/integration but delaying Windows — [webpronews](https://www.webpronews.com/why-anthropic-chose-electron-for-claudes-desktop-app-and-what-it-reveals-about-the-future-of-ai-interfaces/); [Allen Pike, 2025](https://allenpike.com/2025/why-is-chatgpt-so-good-claude).
- UI-TARS Desktop (ByteDance): Electron (third-party scan lists Electron v34.1.1), Apache-2.0, macOS/Linux/Windows — [desktopinsights](https://desktopinsights.com/apps/ui-tars-desktop); [pyshine](https://pyshine.com/UI-TARS-Desktop-ByteDance-Multimodal-AI-Agent/).
- Screenpipe: Tauri + Next.js — [Screenpipe docs](https://docs.screenpi.pe/architecture); licence reportedly moved from MIT to source-available (conflicts with docs still saying MIT) — [usecarly](https://www.usecarly.com/blog/tags/screen-recording-search/).
- Jan: formerly Electron, main branch rebuilt on Tauri (v0.8.5) — [pyshine (secondary)](https://pyshine.com/jan/).
- Cap (screen recorder): Tauri, with a (closed) PR exploring a move to Electron — [Cap PR #1179](https://github.com/CapSoftware/Cap/pull/1179); [Cap PR #2119](https://download.plaud.ai/CapSoftware/Cap/pull/2119/).
- Zed: GPUI — [Linuxiac](https://linuxiac.com/zed-code-editor-hits-1-0-with-gpu-accelerated-ui/); Longbridge Pro: GPUI + gpui-component — [Zed blog](https://zed.dev/blog/community-champion-jason-lee).

### Inferences
- The pattern: teams with web codebases choose Electron for speed and parity; teams with a Rust engine choose Tauri (or GPUI) to stay close to it; Mac-first polish teams go native Swift.

### Gaps
- Not verified this session: Cursor (VS Code fork, Electron), LM Studio, Msty, Raycast (AppKit/Swift; cross-platform port), Cua (Swift `lume` + Python). These are commonly reported but I found no dated primary source within budget; confirm before citing.

## Summary comparison (synthesis, dated 2026-10-17)

| Framework | Latest (date) | 1.0 / stable? | Rust core | Linux render | Agent window knobs | Packaging |
|---|---|---|---|---|---|---|
| Tauri 2 | 2.12.4 (2026-10-14); 3.0 alphas | yes | in-process backend + web UI over IPC | WebKitGTK (NVIDIA/DMABUF issues) | built-in API (always-on-top, content protection, ignore cursor) | turnkey |
| Electron | 41 (Chromium 146) | yes | sidecar / napi | bundled Chromium, Wayland default 38.2+ | built-in | turnkey |
| SwiftUI/AppKit | n/a | yes | UniFFI/C ABI | none (macOS only) | full native | Xcode |
| GPUI | gpui 0.2.2 (2025-10-22); gpui-pre 0.3.9 weekly (2026-10-12) | no, snapshot API | in-process | native Vulkan/Wayland/X11 | needs own platform code | DIY |
| gpui-component | 0.7.3 (2026-10-12) | no | — | — | — | — |
| Slint | 1.18.1 (2026-09-21) | yes (stable 1.x) | in-process | native | DIY | DIY |
| cxx-qt | 0.10.0 (2026-08-24) | no | bridge | Qt native | Qt APIs | Qt deploy tools |
| Dioxus | 0.7.10 / 0.8 alpha (2026-10-16) | no | in-process | webview (desktop) or native renderer | via tao/wry | dx bundle |
| iced / egui | 0.14.0 (2025-12-07) / 0.37.0 (2026-10-15) | no | in-process | native | DIY | DIY |

All version cells from crates.io API on 2026-10-17 except Electron (Electron blog) — the remaining cells are inferences from the cited findings above.
