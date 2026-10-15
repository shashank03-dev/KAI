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

## Remaining work before shipping

- **U05 / P2, open:** define transcript follow-newest versus reading older
  steps, with a deliberate return-to-latest control and live-run tests.
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
