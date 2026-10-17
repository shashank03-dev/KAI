# Verified media completion (2026-10-17)

## Problem and design

The engine previously stopped only on a model-issued `done`, a human check,
or a run limit. Successful media playback could therefore be followed by
unrelated clicks, including queued/batched computer actions. A prompt asking
the model to stop is not an execution boundary. With this feature enabled,
a model-issued `done` also requires independent playback evidence and is
refused after bounded attempts if the evidence cannot be established.

Add a narrowly scoped completion barrier for single-goal requests to
play a specifically named song/video on YouTube. The barrier runs before the
first action (if the requested media is already playing), next model decision
or queued action, and between actions in replay, batches,
and executor plans, including **immediately after native/computer-tool actions**
(so success on the final permitted step wins over `StepLimit`). A desktop
observation must identify the requested title,
a YouTube window, and its **Pause** playback control on two separate reads.
An instrumented browser must also report a non-ad, unmuted media element whose
`currentTime` advances while `paused` and `ended` are false. Absent/ambiguous
evidence fails closed; it cannot silently declare a general task complete.
The browser URL must parse as a real YouTube `/watch` page with a video ID.
Only a single-line media request qualifies, except for the precise GUI-only
instruction appended by `kai-app`; a second user goal must not be discarded.
If the outcome ledger is enabled, required outcome probes must pass before
the media barrier finalizes a run. A slow model response or batched action
must still respect the run deadline. Automatic completion is recorded in the trace so it can be audited. The
guard is enabled automatically by desktop KAI, including GUI-only sessions.
There is no per-site setting for the user to enable. Lower-level `Config`
and CLI runs still default off for rollout/benchmark isolation; CLI users can
opt in with `--auto-complete-media` for `kai run`/`kai bench`.

For other tasks, desktop KAI also enables outcome checks by default. Kai asks
the model to describe its success conditions on the first turn, but the model
does not decide whether the conditions passed. A separate, read-only verifier
checks them. A conservative autonomous stop requires a task-derived check to
have failed before the first action, then pass after an action; all model
outcomes must also have valid, passing probes. Compound requests are excluded.
For tasks without sufficiently clear task-derived checks, Kai uses the normal
model `done` gate rather than guessing that one partial result is sufficient.

This is not a universal task-success classifier. Desktop accessibility cannot
prove audio output or distinguish every unlabelled preroll advertisement; it
rejects visible ad controls and requires corroborated Pause state. It also
depends on browsers exposing accessible playback controls. In those cases,
Kai continues using the existing model-done gate. Do not substitute screenshot
appearance, a successful click, or the model's prose for playback evidence.
The desktop title/control check does not independently authenticate the browser
process, measure actual audio output, or ensure the browser requested by name
was used. Cross-platform player labels and live playback still need validation.

Considered and rejected: trusting a single screenshot/window title, a model's
`expect` string, or any screen with a Pause button (all can falsely report
success). An unbounded second-model verifier would add latency and could itself
keep issuing actions.

## Evidence and evaluation

- Chrome DevTools Protocol, `Runtime.evaluate` (read-only state query):
  https://chromedevtools.github.io/devtools-protocol/tot/Runtime/
- WHATWG/MDN `HTMLMediaElement.paused`, `.ended`, `.currentTime`, and `.muted`:
  https://developer.mozilla.org/en-US/docs/Web/API/HTMLMediaElement
- Existing engine behavior and verification: `engine/model_step.rs`,
  `engine/batch.rs`, `engine/gate.rs`, `docs/VERIFICATION.md`.
- Read-only macOS accessibility inspection of Zen on 2026-10-17 exposed the
  player's paused control as `Play keyboard shortcut k`. Regression mocks
  now use the corresponding playing-state description
  `Pause keyboard shortcut k`; actual playback was not changed in the probe.

Regression tests cover zero subsequent actions after confirmed playback,
including queued batch actions; they refuse mismatched titles, paused media,
and missing controls. They also cover native computer-tool actions at the final
step, provider-queued actions, multiline goals, misleading URLs, and generic
file-output completion with baseline, failed-probe, and partial-goal safeguards.
Before expanding to other task types, measure live
success/false-completion rate and extra observe latency (two runs with guard
enabled versus two disabled) on desktop Zen and the managed browser. No live
A/B results are claimed here.
