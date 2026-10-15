# App UI/UX audit — 2026-10-16

Scope: the current GPUI app, preserving its graphite/porcelain identity. Improve
existing layouts and interaction defects before adding visual complexity.
Source baseline: `f27499b`. Changes stay on the attached foundation worktree.

## Research and design brief

- [WCAG reflow](https://www.w3.org/WAI/WCAG22/Understanding/reflow.html): use
  available width without losing information or controls. Applied here as a
  desktop layout heuristic, not a claim of WCAG certification.
- [Focus not obscured](https://www.w3.org/WAI/WCAG22/Understanding/focus-not-obscured-minimum.html)
  and [minimum target size](https://www.w3.org/WAI/WCAG22/Understanding/target-size-minimum.html):
  keep actionable controls reachable and distinguishable.
- GPUI pinned source `gpui-pre 0.3.8`, `elements/div.rs`: offsets are negative
  toward the end; scrolling has capture/bubble ordering and clamps at layout.
  Ely's `layout/scroll.rs` adds propagation suppression based only on content
  extent. Boundary decisions need offset and direction, not extent alone.
- [GPUI ScrollHandle](https://docs.rs/gpui/latest/gpui/struct.ScrollHandle.html):
  persistent handles permit deliberate navigation through a transcript.

Selected approach: targeted improvements to current components. Compact run
layouts should reuse the existing tabs, add a screen-view tab and a reachable
Tasks toggle, and keep steering/Stop controls visible. Wide layouts retain the
split view. Improve muted text contrast and fix wheel chaining at source.
A replacement design system or decorative redesign would obscure these defects
and increase regression risk without validating the current product.

## Evidence plan

Separate executable correctness, rendered appearance, interaction tests and
platform gaps. Use synthetic capture states with isolated config; do not call
models, read credentials, grant OS access or modify the user's preferences.
Validate minimum 720×520 and normal 1180×780, both themes, plus intermediate
1024×768. Test numeric contrast and scroll boundaries in the host suite.
Record live observations only when the bound Kai window is actually visible.

## Initial findings

- U01 / P1: 720px run page retains a 256px task list and a fixed two-column
  split. Remaining text/control panes do not have usable width.
- U02 / P2: nested ScrollArea consumes wheel events whenever content is larger
  than its viewport, even when already at the relevant end.
- U03 / P2: `FG_SUBTLE` colors are used for normal small text but omitted from
  contrast assertions. Test them on canvas and raised surfaces.
- U04 / validation blocker: old capture mode crops the first monitor, which can
  capture a different foreground app. Do not accept those images as UI evidence.
- U05 / P2: transcript has no explicit follow-newest/read-older policy. Requires
  a persistent scroll owner and a deliberate return-to-latest affordance.

The results below distinguish fixes from validation still required.

## Additional live findings

- U06 / P1: opening Model Settings synchronously calls `secrets::keys()`
  during render. The synthetic fixture timed out; a one-second macOS sample
  showed 864 samples in `settings::render → model_settings::model →
  secrets::keys → SecKeychainFindGenericPassword → mach_msg`. This is a real
  UI-thread stall, not a model/network timeout. Remove credential lookup from
  rendering and explain that blank fields retain existing keys. Presence-only
  lookups and async credential badges were rejected: neither is necessary to
  display settings, and both trigger avoidable credential access. Explicit
  Test/Save actions retain their existing behavior. Verify by reopening the
  same fixture without providing credentials or granting access.
- U07 / P2: navigation destinations and task rows lacked accessibility roles
  and names. Live AX tree omitted all six rail destinations. Add named,
  tabbable buttons, descriptive title actions and visible focus borders.
- U08 / P2: independent review identified focus advancing into clipped task
  rows. Use a persistent sessions scroll handle and per-run focus handles;
  reveal focused rows with `scroll_to_item`.
- U09 / P2: live Screen view could paint below the steering composer. Give
  scroll areas explicit full-size bounds and clip the transcript viewport.

## Implemented fixes

| Finding | Change | Evidence / limit |
|---|---|---|
| U01 | Below 1100px, use a single pane with Steps/Terminal/Screen tabs and a collapsible task list. Keep Pause, Stop and steering available. | Live 720×520 run and 1024×768 terminal; 1180×780 dark split view. |
| U02 | Clamp wheel offsets; consume events only when the child actually moves. Boundary events reach the parent. | Two regression tests first failed, then passed. Live nested approval scroll reached the final JSON field and closing brace. |
| U03 | Raise subtle text contrast in both themes; include subtle tokens on canvas and surface in the 4.5:1 test. | Contrast regression first failed on dark subtle text. Token checks pass; this is not full accessibility certification. |
| U04 | Automatic capture selects exactly one window by current PID and title; ambiguity fails without a desktop fallback. Add hold, size and dark-theme fixture controls. | Compiles; automatic capture itself was not accepted as visual evidence. Bound-window tools supplied live observations. |
| U06 | Remove credential lookup from Settings rendering. Blank fields explain retention of existing keys. | Before: 16.7s UI timeout and sampled Keychain stall. After: Settings AX tree returned and screenshot displayed without granting credential access. |
| U07 | Name and expose six rail destinations and task rows as buttons; add tab stops and focus styles. Name title-bar actions by purpose. | Before AX tree omitted rail. After tree includes Run, Models, Skills, History, Benchmarks, Settings. Full screen-reader behavior remains unverified. |
| U08 | Persistent task scroll/focus handles reveal a row once on focus entry. Do not force it back on every render. | Independent review caught and verified the correction to scroll snapping. Final live keyboard traversal remains unverified. |
| U09 | Bound and clip run scroll viewports above the composer. | Rebuilt minimum run has a viewport scrollbar and no transcript paint beneath the composer; compact Screen tab needs final visual recheck. |
| U10 | Explicitly size full-page scroll areas in Home, Skills, Models, Benchmarks, History details and results. Settings uses one bounded outer scroll area. Compact home uses less introductory space. | Live Skills catalog scroll moves to later cards; Home spacing change and other page endings need final recheck. |
| U11 | Replace six cramped backend segments with a labeled dropdown. | Before: several labels collapse to ellipses at 720px. Final dropdown compiles; final visual recheck pending. |
| U12 | History empty-state copy describes saved runs and the Privacy recording setting without promising every run is recorded. | Source review; recording is off in the isolated fixture. |

Vendor changes are listed in `vendor/README.md`; no unrelated assets or repo
materials were deleted. New audit environment controls apply only to explicit
`KAI_CAPTURE` fixture mode. No permission setting, model download or real task
was activated during these inspections. The Settings defect attempted a
credential-store lookup; no OS permission was granted or credential value
printed, and the blocked synthetic process was terminated.

## Validation record

Host: Apple Silicon macOS. Isolated config: `/tmp/kai-ui-audit-config`.
Synthetic capture states do not establish model task success or performance.

- Build: `cargo build -p kai-app` passes.
- App regression suite: 41 tests, including the two wheel policy regressions.
- Workspace suite: 618 passed, 0 failed, 2 ignored across 40 suites.
- Clippy: `cargo clippy --workspace --all-targets -- -D warnings` passes for
  owned workspace code. Ely still emits 90 existing deprecation warnings;
  `block 0.1.6` has an existing future-incompatibility warning.
- Formatting and `git diff --check` checked separately.
- Independent code review checked GPUI wheel dispatch ordering, keyboard
  activation, focus reveal and the Settings changes. Its final source review
  found no remaining concrete P1/P2 regression in the reviewed diff.

Live checks completed before the user stopped computer use:

| Fixture | Size / theme | Confirmed observation |
|---|---|---|
| Run | 720×520 light | Compact layout, named controls, bounded transcript, steering and Stop visible. |
| Run | 1180×780 dark | Task list and split panes retained; transcript and screenshot legible. |
| Terminal | 1024×768 light | Two outputs with distinct exit status, Copy controls and fixed composer visible. |
| Approval | 720×520 light | Stop/Skip/Approve remain visible; nested scroll reaches closing JSON brace. No approval submitted. |
| Model Settings | 720×520 light | Render-time Keychain stall reproduced; after fix page opens and exposes controls. Dropdown changed afterward. |
| Skills | 720×520 dark | Catalog/search/cards visible; wheel scroll reaches later cards. No skill toggled. |
| History | 720×520 light | Empty-state action visible. Copy corrected afterward. |
| Home | 720×520 light | Composer visible; oversized introduction and unbounded scrolling identified, patched afterward. |

Some native capture requests returned stale frames or ScreenCaptureKit error
`-3811`. Mismatched frames are inconclusive, not passes. Fresh fixture windows
were used where possible. On the user's instruction, computer-use tools were
stopped; no subsequent visual verification is claimed. Test-only GUI processes
started for this audit were cleaned up through process management.

## Second pass — 2026-10-16 (source baseline `6face92`)

Scope: source review, Rust tests and build checks only. The app was not
launched, no model was called, no credential was read and no OS permission
was changed, so every result below is source-level; none is a live visual
or OS observation.

### Review of `6face92`

The commit delivered what its table claims at source level: compact run
layout, wheel boundary policy (verified against `gpui-pre 0.3.8`: the
element's own scroll listener registers after mouse listeners and bubbles
first, so the undo-and-clamp in `scroll.rs` is ordered correctly),
contrast tokens, Settings render without a keychain read, named rail and
task rows. Its regression tests cover the wheel policy and contrast only;
the layout and accessibility changes have no automated test and rely on the
partial live checks recorded above. It left U05 open and missed the three
defects below (U13–U15).

### Findings and fixes

| Finding | Change | Evidence / limit |
|---|---|---|
| U05 / P2 | Transcript policy in `kai-app/src/follow.rs`. Steps and Terminal follow the newest entry. Any upward move off the end (wheel, trackpad nudge, scrollbar, keyboard focus) stops following; content that grows under a still offset (new step, a card finishing, a shorter window) keeps following. While reading older entries, new arrivals never move the view; a "Jump to latest · N new" button (tabbable, named) and ⌘/Ctrl+↓ return and follow again, as does scrolling back within 24 px of the end. One scroll handle and policy per run and tab, pruned when runs are cleared. Screen and Sub-agents tabs do not follow. | 10 unit tests: follow on growth, hold on scroll-up with arrival count, jump, return by scrolling down within the slack, a half-pixel upward nudge stops following (earlier drafts used the 24 px slack, then 1 px, upward too; review showed slow trackpad scrolling would snap back), a shrinking range at the end, resize while following, content that fits, shrinking counts, per-run/tab state and pruning. The GPUI wiring (`scroll_to_bottom` applied at prepaint, offset read at render) is checked against the pinned GPUI source, not observed live. |
| U13 / P1 | Keychain reads still ran on the UI thread at app start (`check_connection`), on every run launch and on each connection Test; U06 showed one such read stalling the window 16.7 s. Keys are now resolved on the agent runtime's blocking pool (`kai-app/src/keys.rs`); typed onboarding keys still win (`Keys::or` in `kai-core`). | `the_ui_thread_never_reads_the_keychain` fails if any app source other than `keys.rs` calls `secrets::keys` or `secrets::get` (a text guard: an aliased import would evade it); `typed_keys_win_and_stored_keys_fill_the_rest` covers the merge. Writes still run on the UI thread: Save, the connection Test (it saves typed keys first) and onboarding; a write can wait on a macOS consent prompt. A run whose key read waits on a prompt shows as running, and Stop takes effect only once the read returns. Not measured. |
| U14 / P2 | `valid_keys` accepted any single character, so a model or native-tool reply with `"\n"`, ESC or NUL reached OS injection as a raw Unicode key. Single keys must now be printable and non-whitespace; the Decision schema drops control-character keys (a key action left with none is rejected for a retry). | `computer_contract` rejects `\n`, ESC, NUL and NBSP; `control_characters_are_never_keys` covers the schema path. |
| U15 / P2 | The approval dialog's action JSON scrolled on both axes, so the end of a long command could sit off to the right behind an auto-hiding scrollbar, while the dialog promises the exact action. It now scrolls vertically and wraps. | Source change; the existing test proves the JSON holds the full command, not that it is visible. Needs a live check at 720×520. |
| U16 / P2 | Task rows, listed as tabbable in U07/U08, were not: their tracked focus handles came from `cx.focus_handle()`, which is not a tab stop, and GPUI applies `tab_index` only to handles it creates. Rows now use a tab-stop handle, so Tab reaches them and the U08 reveal can fire. | Found by review against `gpui-pre 0.3.8` `div.rs`/`tab_stop.rs`; live keyboard traversal unverified. |

`agent.rs` was 509 lines (over the 500-line rule); the approval and
human-check bridges moved to `asks.rs` unchanged.

### Independent review before merging to main

Two reviewers (read-only, against the pinned GPUI source and the full
`origin/main..HEAD` diff) checked every commit on this branch. Confirmed
findings and their fixes:

| Finding | Change | Evidence / limit |
|---|---|---|
| R1 / P1 | Browser lane: a click or key whose CDP call returned an error (a `confirm()` dialog opened, the tab closed) left the input-release guard armed, which poisoned the lane until restart; every later call failed although the click was reported done. An error now sends the release without poisoning; only a cancelled await poisons. | `an_error_releases_without_poisoning_the_lane` (`web/release.rs`). Live browser tests not run on this host. |
| R2 / P2 | Before every action the engine required the whole fresh element list to equal the old one, so a clock, ticker or a field's typed value refused every step. The check now covers the action's target (or the focused field for untargeted typing): role, name, bounds, enabled, secure; value and focus may change. Grid observations still compare pixels. | 6 tests in `engine/fresh.rs`; `cancellation_e2e` now renames the approved element during the approval (refused) and changes only another label (clicked). No live model run measured the effect on step success. |
| R3 / P2 | Computer tools refused any action when the exact screen hash changed (a blinking caret flips it). They now compare the element under the action's point or mark; pixels only for a grid. | `a_point_click_compares_the_element_under_it`; the `is_current` port now receives the action. |
| R4 / P2 | A typed card number was swapped for its label before the trace could remember it, so a reformatted echo elsewhere on screen reached the model. The raw text is remembered first. | `a_reformatted_card_number_elsewhere_on_screen_is_blanked` failed before the fix and passes after. Computer-tool `type` steps are not covered by this path (remaining risk). |
| R5 / P2 | The native bench stopped at its first click: every coordinate action is high risk and a runner without an approver says Stop. That default stays (`native_side_effect_requires_host_approval`); the bench now passes an explicit `Unattended` approver that approves except passwords, secure fields and sensitive typing, as the native loop did before. | `unattended_runs_go_on_but_never_type_secrets`. No live `openai-native` bench run. |
| R6 / P2 | Provider-batched actions after the first required a byte-identical screenshot, so every multi-action reply stopped after one action. The queue now continues while the element layout is unchanged (values and focus may differ) and stops when a dialog appears or the page moves. | `a_layout_survives_typing_but_not_a_new_dialog`. |
| R7 / P3 | Native action mapping panicked on a wrong field type from the model (`"text": 5`). It now asks instead. | `cua_native` maps that input to Ask. |
| R8 / P3 | Long text is typed key by key, not pasted, whenever an execution context is set (every engine action since `c9054dd`). Kept on purpose: pasting overwrites the clipboard and cannot be cancelled mid-way; typing checks cancellation per key. Slower for long text; not measured. | Decision recorded in `DECISIONS.md`. |

### Validation (second pass, after review fixes)

Host: Apple Silicon macOS, rustc 1.99.0, `CARGO_TARGET_DIR=/Users/shashank/KAI/target`.

- `cargo fmt --all -- --check`: passes.
- `cargo clippy --workspace --all-targets -- -D warnings` (macOS host):
  passes. Ely's 90 existing deprecation warnings are unchanged.
- Windows: `cargo clippy --workspace --exclude kai-app --all-targets
  --target x86_64-pc-windows-gnu -- -D warnings` passes, cross-checked from
  macOS with clang and stub C headers (`assert.h`, `string.h`, `windows.h`)
  for `ring` and `stacker`. `kai-app` is excluded: GPUI's Windows build
  script cannot run off Windows. Type-checks only; nothing ran on Windows.
- Linux clippy: **not run** (`wayland-sys` needs a Linux `pkg-config`).
- `cargo test --workspace`: 641 passed, 0 failed, 2 ignored, 40 suites
  (618 at `f27499b`).
- `kai skill validate skills` and the mock bench (`failures: none`, no
  model called) pass; `git diff --check` is clean.
- `crates/kai-core/tests/human_e2e.rs` (530 lines) is over the 500-line
  rule as it already was on `origin/main`; not changed here.
- Not run: any live app, model, browser, keychain or OS-permission check.

## Remaining work before shipping

- **Linux and Windows:** run CI's clippy on Linux (including `kai-app`) and
  Windows for `kai-app`; neither was possible from this Mac.
- **Live A/B for R2/R3/R6:** the narrower pre-action checks change how often
  a live step proceeds; per CLAUDE.md, measure with `kai bench` (two runs
  per side) before claiming an effect.
- **U05 live check:** run a synthetic live transcript at 720×520 and
  1180×780: follow during arrivals, scroll up, see the count, jump back,
  switch tabs and tasks; confirm ⌘/Ctrl+↓ is not taken by a focused input
  on each OS.
- Finish the visual matrix: each major page at all three sizes in both themes;
  populated History, Models, Benchmarks, onboarding, failure and human-input
  states; final compact Screen/Home/backend-picker and focus traversal checks.
- Screen-reader review: task progress announcements, read-only transcript
  content, contextual names for repeated card actions, modal focus return,
  reduced motion and large text. Added roles alone are insufficient.
- macOS input/permission behavior and automatic window capture need their own
  live acceptance checks. Windows and Linux rendering, wheel direction,
  fractional scaling, keyboard focus and dialogs were **not** validated here.
- The two ignored workspace desktop tests remain ignored; test totals do not
  establish cross-platform computer-use readiness or frontier-model parity.

This is a concrete audit and remediation pass, not a claim that the complete
product UI or all operating systems are ready to ship.
