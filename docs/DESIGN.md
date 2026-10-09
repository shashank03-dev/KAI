# Kai — Design

Written before any UI code. Every component named here is a real Ely GPUI
component at the pinned commit; where the build prompt's name differs from
Ely's, the Ely name is used and the mapping is in the table at the end.

## Who and why

A student or developer on an ordinary Windows or Linux laptop — often a
low-end one — who wants a repetitive job done on their own PC: search and
open something, file and rename things, fill a form, change a setting, check
their machine or their own website's security. They are handing an AI their
mouse, keyboard and screen. **The one feeling to design for is trust through
visibility**: at every moment they can see what Kai sees, what it is about to
do, and how to stop it.

## Principles

1. **Status is always visible.** The agent's state, the step, the model, the
   last step's latency and the connection are on screen in every state: in
   the status bar while idle, in the mini window while running.
2. **The user stays in control.** Stop works at any moment from three places
   (the interrupt button, the global hotkey, the top-left corner). Risky
   actions ask first. Nothing happens silently.
3. **Recognition over recall.** Starter prompts drawn from installed skills,
   skills as cards, `Ctrl+K` for every command, `?` for every shortcut.
4. **Errors say what happened and what to do next**, never a raw error string:
   "The DigitalOcean server didn't answer in 45 s. Kai switched to Gemini for
   this step. Is the Droplet up? Run `deploy/digitalocean/up.sh`."
5. **Keyboard first.** Every action has a shortcut; Tab order follows reading
   order; the focus ring (Ely's 1 px focus border) is always visible.
6. **Calm by default.** Ely's warm, quiet palette and one accent — Kai saffron.
   Colour only where it carries meaning: **green** running, **amber** waiting
   for you, **red** stopped or error. Motion explains change and holds still
   under reduced motion (`motion::duration`).
7. **Fast on low-end hardware.** No continuous animation while idle, frames
   shown as JPEG files from disk (never re-encoded in the UI), lists
   virtualised, the agent loop entirely off the UI thread.

## Brand

- **Mark:** an open hand of five rounded strokes (`assets/icons/kai-hand.svg`),
  registered as an icon through Ely's asset source (`Assets::before`).
- **Name:** "Kai" in the title bar and on the splash.
- **Accent:** saffron. Light `#B45309` on Ely's light background, dark
  `#F59E0B` on Ely's dark background — both checked for WCAG AA (4.5:1 for
  text) with Ely's contrast checker; the numbers are in `DECISIONS.md`.
- **Language:** English. UI strings live in one table (`i18n.rs`).

## Information architecture

```
Splash ──► Onboarding (first run) ──► Home (AppShell)
                                        ├─ Run        (idle ▸ running ▸ done)
                                        ├─ Skills     (grid ▸ detail sheet)
                                        ├─ History    (list ▸ trace)
                                        ├─ Benchmarks
                                        └─ Settings   (6 sections)
Everywhere: Command palette (Ctrl+K) · Quick launcher (Ctrl+Shift+Space, global)
            · Shortcut cheat-sheet (?) · Toasts · Offline banner
While running: Mini window (always on top, corner) ⇄ full Run view
```

## Screens, components and states

Each screen lists its Ely components and every state. All screens exist in
light and dark; dark is designed (surfaces lift, accents lighten), not
inverted — Ely's palettes do this per mode.

### 1. Splash
`shell::SplashScreen` with the hand mark, "Kai", tagline "Gemma 4's hands
on your computer", and status `typography::ShimmerText` "Waking Gemma…". Under
one second: it closes as soon as settings load; the model warm-up continues in
the background and reports into the status bar.

### 2. Onboarding — `onboarding::OnboardingWizard` (5 steps)
| Step | Content | Ready when |
| --- | --- | --- |
| Welcome | One sentence; `chat::CapabilityCards` (Sees your screen · Clicks and types · Asks before risky things · Runs on open Gemma 4); privacy promise in a `feedback::Callout`. | always |
| Connect the brain | Three `forms::RadioCard`s: DigitalOcean server · Gemini API · Both (recommended). `forms::UrlInput` + `forms::PasswordInput` for the server, `PasswordInput` for Gemini. **Test** = `Button::loading` that sends a real image request; result as `generative::ModelStatus` + `typography::DurationText` + green `feedback::InlineMessage`, or `feedback::ErrorView` with the fix. | a test passed |
| Permissions | One `agent::PermissionPrompt`-style card per check (Screen capture, Accessibility, Mouse and keyboard) with a status `data_display::Badge` that turns green by itself (polled every second), why it is needed, and a button that fixes it (turns on AT-SPI on Linux). | none Missing |
| Safety | `buttons::SegmentedControl` for approval level (default: ask for high-risk only); `forms::HotkeyInput` for the kill switch, default filled in; `feedback::Callout` "Three ways to stop Kai". | always |
| Try it | `chat::SuggestionChips` with one safe demo task. Running it ends onboarding. | — |

States: each step's empty/filled; Test idle / loading / passed / failed (with
the fix); permissions polling.

### 3. Home — `layout::AppShell`
- **Title bar:** `shell::TitleBar` "Kai" (it draws `WindowControls` per OS
  and its own drag region), theme toggle and palette button as actions.
- **Sidebar:** `shell::ActivityBar` — Run, Skills, History, Benchmarks;
  Settings in the footer.
- **Status bar:** `shell::StatusBar` — left: `feedback::ConnectionStatus`
  (Droplet / Gemini fallback / offline) and the active model; right: the last
  step's latency and the stop hotkey as `typography::KbdCombo`.
- **Idle:** `chat::WelcomeScreen` (time-of-day greeting) over the composer:
  `chat::PromptInput` with a `forms::Select` model picker (26B fast /
  31B careful) and a Team switch (`forms::Switch`) as tools;
  `chat::SuggestionChips` from installed skills; `chat::FollowUpSuggestions`
  after a run. No voice button: speech-to-text is not implemented.

States: idle, offline (`feedback::Banner` + `shell::OfflineIndicator`, send
disabled with the reason), not configured (Banner linking to Settings).

### 4. Running
- The main window gets out of the way: Kai opens the **mini window**
  (`shell::MiniWindow`, a `WindowKind::PopUp` always-on-top window in the
  top-right corner) and minimises the main window, so it never covers what
  Kai clicks. On Windows both windows are excluded from capture; on Linux
  their rectangles are masked out of every frame.
- **Mini window:** `agent::AgentStatus` (green Working / amber Waiting for you /
  red Failed), the current step in words ("Clicking **Save**"), "Step 4 of 40",
  `chat::ThinkingIndicator` while the model decides, a large stop button
  (`chat::stop_button`), and Expand.
- **Run view** (expanded): `layout::SplitPane`.
  - Left: `agent::AgentPlan` (sub-agents' plan in team mode),
    `agent::AgentStepList`, and one `agent::ToolCallCard` per step — action,
    thought, target, and the timing breakdown (capture · tree · encode ·
    model · act) as its result.
  - Right: `agent::ComputerUseViewer` with the latest marked frame, the pointer
    ring at the target and the caption; below it `agent::BrowserPreview`'s
    frame strip (Ely's ScreenshotStream) to scrub earlier frames.
  - A collapsible `layout::Panel` with `chat::TokenCounter`,
    `data_display::UsageBar` (context window) and `chat::CostEstimator`
    (Droplet $/h × run time).
- **Approval:** an `overlays::Dialog` ("Kai wants to click **Send**") with the
  frame, the element and the exact action; actions Stop · Skip · Approve;
  Enter approves, Escape stops. The mini window turns amber. (Ely's
  `ToolApprovalDialog` has only Run/Deny; the prompt needs Skip too, so Kai
  composes the same `Dialog` it is built on.)
- **Question:** `agent::HumanInputRequest` when Kai uses `ask_user`.

States: thinking, acting, waiting for approval, waiting for an answer,
fallback active (status bar amber "Gemini fallback"), stopping.

### 5. Done
The window returns. `feedback::ResultView` success or failure in plain words,
`data_display::Statistic`s for steps, total time and seconds per step,
`motion::Confetti` once on success (still under reduced motion). Actions:
**Save as skill**, **Run again**, **View trace**. On failure, the failed step's
frame and reason, and `feedback::ErrorView` with the next step to take.

**Save as skill** opens a `layout::Sheet`: `documents::MarkdownEditor` with the
draft `SKILL.md` (Gemma drafts it from the trace) beside a
`documents::MarkdownRenderer` preview; validation errors from `kai skill
validate` show live as `feedback::InlineMessage`s; Save is disabled until
valid.

### 6. Skills
`forms::SearchInput`; a grid of `layout::Card`s (name, description, `Badge`
built-in / yours, `forms::Switch` to enable); `feedback::EmptyState` with an
action when there are no custom skills; import via `forms::DropZone` for a
folder or zip. Detail `layout::Sheet`: rendered `SKILL.md`, its files in
`lists::FileTree`, the validation result, and benchmark runs.

### 7. History
`forms::SearchInput` over `chat::ConversationList` (Ely's ConversationSearch);
a trace opens in `debug::TimelineProfiler` (Ely's TraceViewer: a track per
phase) with its frames in `agent::BrowserPreview`; export with
`chat::ConversationExport` to a zip (trace JSON + frames).

### 8. Benchmarks
A row of `data_display::Statistic`s (success rate, p50, p95, total),
`charts::BarChart` of seconds per step per backend, `tables::DataTable`
(Ely's EvalResultTable) of the latest runs, and **Run benchmark** with
`motion::ProgressBar`. This is the projector page.

### 9. Settings — `settings::SettingsLayout`
Model & server (endpoint, keys in the OS keychain via `PasswordInput`, Test
buttons, default model) · Safety (approval level, kill switch, limits) ·
Appearance (`settings::ThemeSelector`) · Privacy (where traces live, delete
all with `overlays::ConfirmDialog`) · Shortcuts (`settings::KeyboardShortcutsList`)
· About (licence, credits for Gemma, Ely, Lucide).

### 10. Everywhere
`navigation::CommandPalette` on `Ctrl+K` (new task, any screen, any skill,
toggle theme, stop); global `Ctrl+Shift+Space` opens
`navigation::QuickLauncher` from any app; `feedback::Toaster` toasts;
`?` opens the shortcut list.

## Name mapping (prompt → Ely at the pinned commit)

| Prompt | Ely |
| --- | --- |
| TrafficLights, WindowControls | `shell::WindowControls` (inside `TitleBar`) |
| WindowDragRegion | `shell::drag_region` |
| NavigationRail | `shell::ActivityBar` |
| MiniWindow, CompactMode, AlwaysOnTop | `shell::MiniWindow` (a PopUp window) |
| ShortcutRecorder | `forms::HotkeyInput` |
| Greeting | `chat::WelcomeScreen` |
| Composer | `chat::PromptInput` |
| ModelSelector | `forms::Select` |
| StarterPrompts | `chat::SuggestionChips` |
| ScreenshotStream | `agent::BrowserPreview` (its frame strip) |
| InterruptButton | `chat::stop_button` |
| ContextWindowMeter | `data_display::UsageBar` |
| TraceViewer | `debug::TimelineProfiler` |
| EvalResultTable | `tables::DataTable` |
| StatCard | `data_display::Statistic` |
| ConversationSearch | `forms::SearchInput` + `ConversationList::query` |
| EmptyStateWithAction | `feedback::EmptyState` |
| Result | `feedback::ResultView` |
| LoadingButton | `buttons::Button::loading` |
| SystemNotification | not finished in Ely (`prove`); Kai uses the mini window's amber state plus a toast instead |
| TrayIcon / TrayMenu | macOS-only in Ely; not used (Linux/Windows build) |
