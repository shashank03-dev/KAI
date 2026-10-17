# kai-ui: Kai's own component kit

Design note, 2026-10-17. Status: **approved by the owner, not built yet.**
The execution plan for parallel workers is at the end.

## What

Replace the vendored Ely GPUI Components (`vendor/ely-gpui-component`) with
`crates/kai-ui`, a small component kit that Kai owns, writes from scratch,
lints and tests. It stays on GPUI. The look moves to the owner's brief: **a
combination of ChatGPT, Raycast and Linear.** When every screen is on
`kai-ui`, `vendor/` is deleted.

## Why

- **The one recorded macOS crash came from Ely.** `Select::render` asserts
  when a select has zero choices. The Settings page hit that with Codex or
  Claude Code selected and no catalog fetched (`bb46617`, DECISIONS.md
  "Mac crash, account connections and the DMG").
- **Ely is unowned.** About 251,000 lines with no upstream: the repository
  stopped being cloneable on 2026-10-12. It is excluded from lint and test,
  and already carries a local edit. Fixing a crash in it means editing a
  dependency nobody maintains.
- **Kai uses little of it.** `kai-app` imports about 100 Ely types and 51
  icons, out of 40-odd module families (maps, finance, mail, git, calendar
  and others Kai never shows).
- **GPUI itself is not the problem.** It renders Zed 1.0 on macOS and
  Linux. The stack research (`docs/RESEARCH_2026_10_STACK.md`) rejected
  Tauri (WebKitGTK failures on Linux, a TypeScript UI over IPC), Electron
  (size, a sidecar or N-API bridge) and SwiftUI (no Linux UI).

## What was rejected

- **Keep Ely and patch it.** Every fix lands in 251k unlinted lines with no
  upstream, and the next crash will be the same kind.
- **Depend on `gpui-component` (Longbridge) instead.** It is maintained and
  good, but it is another fast-moving external kit tied to its own GPUI
  version, and it would bring the same risk back. Read it (and GPUI's own
  examples) for technique, especially text input; do not depend on it or copy
  code from it without checking and recording its licence.
- **Port Ely's code into `kai-ui`.** It would import its assertions and
  structure. Components are written fresh against the contract below.
- **Change the UI framework.** See "Why".

## Design direction: ChatGPT × Raycast × Linear

What each one contributes, made concrete:

| From | Take | In Kai |
|---|---|---|
| ChatGPT (Mac) | Conversation as the main surface; the composer anchored at the bottom; quiet message stream; streamed text; left sidebar of past sessions; almost no chrome | The run view reads like a chat: your task, then Kai's steps as a stream (thought, action, screenshot) with the composer for steering at the bottom. Sessions in a collapsible sidebar. |
| Raycast | Keyboard-first command palette at the centre; list rows with icon, title, subtitle and right-hand accessories; an action panel (⌘K on a row); shortcut hints beside every action; small HUD toasts | ⌘K palette for every command and page; list rows for skills, models, history; ⌘K actions on the selected row; key-cap hints in the footer of each view; HUD toast for "Copied", "Stopped". |
| Linear | Dense, crisp sidebar; hairline borders; neutral palette with one restrained accent; status icons inline; fast transitions (≈150 ms); views a chord away | Sidebar with sections and counts; 1 px hairlines; graphite/porcelain neutrals; ember only for what is alive; ⌘1–5 pages; status glyphs in rows. |

Kai's identity stays: graphite (dark) and porcelain (light) canvases, ink as
the primary colour, ember (`#F5A524` dark, `#C2410C` light) only for what is
alive, cobalt orbit for the mark, Inter and JetBrains Mono.

### Tokens (`kai-ui/src/theme/`)

- **Colour:** semantic only: `bg`, `surface`, `surface_raised`, `overlay`,
  `border`, `border_strong`, `text`, `text_muted`, `text_faint`, `ink`,
  `ink_hover`, `ember`, `ember_soft`, `orbit`, `success`, `warning`,
  `danger`, `info`, `focus`, `selection`. Values for light and dark derive
  from `brand.rs` constants. Components never use a literal colour.
- **Space:** 4 px grid: 2, 4, 6, 8, 12, 16, 20, 24, 32, 48.
- **Radius:** 6 (controls), 8 (rows, inputs), 12 (cards, popovers), 16
  (dialogs, composer).
- **Type:** Inter 11 / 12 / 13 (body) / 14 / 16 / 20 / 28; JetBrains Mono 11
  / 12 for key caps, counters, step meta and code. Line heights fixed per
  size.
- **Motion:** 120 ms (hover, press), 160 ms (popover, toast), 200 ms
  (sheet, dialog), ease-out; no idle loops except the live dot while a run
  runs; reduced motion shows the end state.
- **Elevation:** hairline + one shadow level for overlays only.
- **Sizes:** `ControlSize::{Sm 24, Md 28, Lg 36}` px heights.

All values are checked by tests: text on every surface meets WCAG AA (4.5:1,
3:1 for large text and UI glyphs) in both themes.

## Architecture

New workspace member `crates/kai-ui` (linted, tested, in `default-members`
only if it builds without a display; otherwise built with kai-app). It
depends on `gpui` and nothing from Kai: no `kai-core`, no `kai-os`. Kai-
specific screens stay in `kai-app`; reusable pieces live in `kai-ui`.

```
kai-ui/src/
  theme/      tokens, Theme (light/dark), ActiveTheme trait, contrast
  assets.rs   fonts + Lucide icons (only the icons used), IconName enum
  primitives/ Text, Label, Icon, Kbd/KbdCombo, Divider, Stack helpers,
              Surface, FocusRing, Avatar/Orb, Skeleton
  controls/   Button, IconButton, Switch, Checkbox, Radio/RadioCard,
              SegmentedControl, Select, Slider (only if used)
  input/      TextInput core (editing, selection, IME, clipboard, undo),
              PasswordInput, SearchInput, UrlInput, PromptInput (composer),
              HotkeyInput
  containers/ Card, Panel, ScrollArea, SplitPane, Tabs, Sheet, Dialog,
              ConfirmDialog, Popover, Menu/DropdownMenu, Tooltip
  feedback/   Toast + Toaster, Banner, Callout, InlineMessage, Badge,
              Spinner, ProgressBar, EmptyState, ErrorView
  data/       ListRow (+ accessories), List (virtualised), DataTable, Tree,
              Statistic, Sparkline, BarChart
  shell/      AppShell, Sidebar, TitleBar, StatusBar, CommandPalette,
              ActionPanel, ShortcutsList, MiniWindow, Splash, Wizard
  agent/      ComputerUseViewer, BrowserPreview, StepStream (step card,
              tool call card), ThinkingIndicator, AgentStatus, Markdown,
              AnsiText, ApprovalCard, HumanInputRequest, SuggestionChips
  gallery.rs  every component in every state, for capture and review
```

### The component contract (what makes them reusable)

1. **Two shapes only.** Stateless components are builder structs that
   implement `RenderOnce` (`Button::new(id, label).variant(..).size(..)`).
   Stateful ones (inputs, select, scroll, palette, menus, tabs with
   keyboard focus) are `Entity` views whose logic lives in a plain-Rust
   `*State` struct in its own file, testable without a window.
2. **Shared vocabulary.** Every component takes the same enums from
   `theme`: `Variant {Primary, Secondary, Ghost, Danger}`, `Tone {Neutral,
   Info, Success, Warning, Danger, Ember}`, `ControlSize`, `Severity`.
   New components reuse them instead of inventing their own.
3. **Composition over options.** Containers take children
   (`ParentElement`); rows take `leading`, `title`, `subtitle`,
   `accessories`. A future component is built from existing ones, not by
   adding flags.
4. **Never panic on data.** No `assert!`, `unwrap`, `expect`, `panic!`,
   `unreachable!` or indexing that can go out of range in `kai-ui` outside
   tests. Empty, missing and oversized data render a defined state: a
   select with no choices is disabled and says why; a list with no rows
   shows its `EmptyState`; an image that fails shows a placeholder; text is
   truncated with an ellipsis or wraps. A test greps the crate for these.
5. **Tokens only.** No literal colours, sizes or durations in components.
6. **Keyboard and focus.** Every interactive component is reachable with
   Tab, shows the focus ring, and has its keys documented in its doc
   comment (Enter/Space activate, arrows move, Esc closes).
7. **Callbacks, not app state.** Components report through `on_click`,
   `on_change`, `on_select` closures; they never reach into `kai-app`.
8. **Files ≤ 500 lines** (CLAUDE.md), one component (or one family) per
   file, a one-line doc comment saying what it is.

### Ely → kai-ui map

Built because a page uses them (counts are files in `kai-app` that import
them, 2026-10-17):

| Ely | kai-ui | Notes |
|---|---|---|
| Button (22), ButtonVariant (23), IconButton | controls::Button, IconButton | `Variant`, `ControlSize` |
| IconName (22), Icon, IconSize | assets::IconName, primitives::Icon | 51 Lucide icons |
| ActiveTheme (20), Theme, Mode, Palette, ThemeSelector, Appearance | theme | brand.rs feeds it |
| Severity (20), Tone, TextSize, ControlSize | theme enums | |
| Toast (12), Toaster, ToastViewport | feedback::Toast, Toaster | HUD style |
| ScrollArea (11) | containers::ScrollArea | keep the wheel-chaining fix from UI_UX_AUDIT |
| InlineMessage (10), Callout (6), Banner, ErrorView (5), EmptyState (3) | feedback | |
| Badge (6), Spinner, ProgressBar, Statistic | feedback / data | |
| TextInput (5), Input, PasswordInput, SearchInput, UrlInput, PromptInput, HotkeyInput | input | the hardest part, see Risks |
| SettingsSection (5), SettingsLayout | shell::Settings* | |
| Switch (4), SegmentedControl, Choice/Select (3), RadioCard | controls | Select: no panic when empty |
| ComputerUseViewer (4), BrowserPreview | agent | the showpiece |
| Dialog, ConfirmDialog, Sheet, Menu, MenuItem, DropdownMenu | containers | |
| MarkdownRenderer (3), AnsiText, SelectableText | agent::Markdown, AnsiText | parse with `pulldown-cmark` |
| KeyboardShortcutsList, KbdCombo, Shortcut | shell::ShortcutsList, primitives::Kbd | |
| CommandPalette, Command | shell::CommandPalette (+ ActionPanel) | Raycast style |
| ToolCallCard, ThinkingIndicator, AgentStatus, AgentState, AgentPlan, StepState, ActivityItem | agent::StepStream etc. | ChatGPT-style stream |
| MessageBubble, MessageFooter, Conversation, ConversationList, FollowUpSuggestions, SuggestionChips | agent / shell::Sidebar | |
| AppShell, TitleBar, StatusBar, StatusBarItem, ActivityBar, Panel, SplitPane, ResizeBorder | shell / containers | |
| MiniWindow, Corner, SplashScreen, OnboardingWizard, OnboardingStep, CapabilityCards | shell | |
| HumanInputRequest, Connectivity, ConnectionStatus, Warning | agent / feedback | |
| Card, CardHeader, Tabs, FocusScope, FocusRing | containers / primitives | |
| DataTable, Row, Column, Cell, Tree, TreeNode, Sparkline, BarChart, Series | data | |
| DropZone, Confetti, TimelineProfiler, Place, Span, Role, Ghost, Assets | rebuild only if the page still needs it; drop otherwise | |

Before building a row, check the import is still live (`grep` the type in
`crates/kai-app/src`); a type imported but unused is dropped.

### Assets

Fonts and icons come from Ely today. Move them before deleting `vendor/`:
`assets/fonts/` (Inter, JetBrains Mono, IBM Plex Sans only if used, each
with its licence file) and `assets/icons/lucide/` (only the 51 icons used,
plus Lucide's `LICENSE`). `kai-ui::assets` serves them through one
`AssetSource`; `kai-app`'s `KaiAssets` keeps the mark and app icon.
`scripts/macos/bundle.sh` copies licences from the new paths.

## How it will be tested

1. **State tests (no window).** Every stateful component's `*State`:
   empty, one, many, long text, unicode and emoji, disabled, keyboard
   navigation, wrap-around, selection out of range after the data shrinks.
   Text input: insert, delete, selection, word moves, undo, paste of
   multi-line text into a single-line input, IME marked text.
2. **GPUI tests.** If `gpui-pre` exposes `test-support`
   (`TestAppContext`/`VisualTestContext`), render each component and drive
   clicks and keys. Check the feature first; if it is absent, record that
   and rely on 1 and 3.
3. **Gallery capture.** `KAI_CAPTURE` gains a `kai-ui` gallery screen
   (every component in every state, light and dark) and keeps every app
   screen. In the cloud: `xvfb-run -a -s "-screen 0 1600x1000x24" bash -c
   'openbox & sleep 1; exec target/release/kai-app'` (as in
   `.github/workflows/screenshots.yml`), at 720×520, 1180×780 and
   1600×1000. Reviewers look at the PNGs; a capture that panics or misses a
   shot fails.
4. **Token tests.** Contrast for every text/surface pair in both themes;
   every token used by at least one component.
5. **Guard tests.** The no-panic grep (contract rule 4); `kai-ui` has no
   dependency on `kai-core`, `kai-os` or Ely (checked from `cargo
   metadata`).
6. **Regression.** The Settings crash case (Codex backend, empty catalog)
   is a named test that fails if `Select` panics or the guard is removed.

Done means: `vendor/` deleted; all CLAUDE.md checks pass (fmt, clippy
Linux, Windows and macOS targets, `cargo test --workspace`, skill
validate, mock bench); capture walks every screen in both themes and three
sizes without a panic; release build time and `kai-app` binary size are
measured before and after and recorded in DECISIONS.md. The UI does not
change agent behaviour, so no live bench A/B is needed; the mock bench
must still pass.

## Risks

| Risk | Mitigation |
|---|---|
| Text input (IME, selection, clipboard, undo) is hard to get right | Build it first and alone; follow GPUI's `examples/input.rs` in the Zed tree at the pinned snapshot; state tests before rendering |
| A big-bang switch breaks the app | Migrate page by page; Ely and kai-ui coexist until the last page moves |
| Look drifts from the brief | Gallery and screen captures reviewed against the table above before each page lands |
| GPUI snapshot upgrade breaks kai-ui | kai-ui uses only core GPUI (`div`, `img`, `svg`, `uniform_list`, `canvas`, actions, focus); upgrade deliberately |
| Markdown or ANSI edge cases | `pulldown-cmark` parser, fuzz-style tests on odd input |

## Execution plan for parallel workers

Waves; each wave's workers own disjoint files. Within a wave, run in
parallel. One integrator runs the full checks between waves.

- **Wave 0, one worker (foundation):** create `crates/kai-ui` (Cargo,
  workspace member), `theme/` with tokens, `ActiveTheme`, contrast tests,
  `assets.rs` with fonts and the 51 icons moved, `primitives/`, the
  shared enums, the no-panic guard test and an empty `gallery.rs` wired
  into `KAI_CAPTURE`. Everything else builds on this.
- **Wave 1, five workers in parallel:**
  1. `input/` (TextInput core and its variants), state tests first.
  2. `controls/` (Button, IconButton, Switch, Checkbox, Radio/RadioCard,
     SegmentedControl, Select).
  3. `containers/` (Card, Panel, ScrollArea, SplitPane, Tabs, Sheet,
     Dialog, ConfirmDialog, Popover, Menu, Tooltip).
  4. `feedback/` and `data/`.
  5. `agent/` (Markdown, AnsiText, StepStream, ComputerUseViewer,
     BrowserPreview, ThinkingIndicator, AgentStatus, ApprovalCard,
     HumanInputRequest, SuggestionChips).
  Each adds its components to the gallery.
- **Wave 2, one worker:** `shell/` (AppShell, Sidebar, TitleBar,
  StatusBar, CommandPalette + ActionPanel, ShortcutsList, MiniWindow,
  Splash, Wizard), then the integrator captures the gallery and fixes
  what looks wrong.
- **Wave 3, page migration, three workers in parallel:**
  (a) run view (`pages/running.rs`, `done.rs`, `approval.rs`,
  `human_dialog.rs`, `mini.rs`, `follow.rs`), (b) home, onboarding,
  skills, history, sessions, chrome, nav, switcher, (c) settings, accounts,
  models, local models, memory, bench, experimental, connections, capture.
- **Wave 4, one worker:** delete `vendor/`, the `exclude` and
  `ely-gpui-component` lines in `Cargo.toml`, update `bundle.sh` licence
  paths, `docs/DESIGN.md` (component names), `README.md`, `CLAUDE.md`
  layout, `vendor/README.md` mentions; run every check; capture every
  screen; record the measurements and the decision in `DECISIONS.md`.
