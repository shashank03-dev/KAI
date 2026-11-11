# Shared computer tools v1

Kai's Rust harness can expose the same bounded action vocabulary to a local
OpenAI-compatible model and to the Responses computer-tool adapter. This is an
experimental foundation. It does not make a local model as capable as a frontier
model, or establish a speed advantage over Codex.

## Enable

The default remains off. Use `--computer-tools-v1` with `kai run` or `kai bench`.
For local operation, select an already installed model with `--backend local`
and the existing model/runtime settings; no OpenAI subscription is required.
The app reads the saved boolean setting `computer_tools_v1`.

For `--backend openai-native`, the flag selects the current Responses `computer`
protocol and requires an explicit compatible `--model`. Without it, the historical
`computer_use_preview` protocol remains available. Both adapters now execute
through Kai's engine, approval policy and dispatcher. The CLI supplies its normal
interactive approver; library callers use `CuaRunner::run_with_approver`. A
headless run cannot approve a side effect by omission. CLI `--yes` does not
silently approve canonical computer payloads.

The model catalog has not changed. No runtime or model download is part of this
feature. Public protocol references are in the
[approved design](design/specs/2026-10-15-computer-use-foundation-design.md).

## OpenAI's contract for open models (`--oai-contract`)

Off by default. `--oai-contract <frame>` (setting `oai_contract`) makes an open
model read and write OpenAI's computer-tool contract instead of Kai's Decision
JSON; it implies the computer tools. Design, sources and per-family evidence:
[RESEARCH_2026_10_OPENAI_PARITY.md](RESEARCH_2026_10_OPENAI_PARITY.md) §5.

- **Reply:** `{"thought", "actions": [OpenAI actions], "message", "phase"}`,
  schema-constrained (`oai_contract::schema`). `--oai-batch` permits more
  than one action; Kai observes again between them and stops the batch if
  the layout changes or an action fails. No actions plus a
  message: `final_answer` is Kai's `done` (checked like any `done`),
  `commentary` asks the user.
- **Same parser:** the action is wrapped as a `computer_call` and decoded by
  `cua_protocol::parse_response`, the parser the OpenAI path uses; then the
  normal validation, risk classification, approval and dispatch.
- **Coordinate frame (data, chosen per model):** `marks` (Gemma: the box
  number in `mark`, kept as an element target so the accessibility path
  still applies), `pixels` (OpenAI, Fara), `norm1000` (Qwen3-VL, Holo2,
  Gemini), `smart-resize` (Qwen2.5-VL family: UI-TARS-1.5, OpenCUA, GTA1).
  Holo3.1's frame is undocumented: calibrate before use (§5b).
- **Frame:** pixel frames get the unmarked screen (`Perceptor::native_frame`,
  up to 1600 px / 1.44 MP) and their points are mapped back onto the frame
  Kai observed. Never on a step without a screenshot (tree-only steps, the
  privacy latch): `oai_contract_e2e` fails if that guard is removed.
- **Prompt:** OpenAI's published loop shape (what changed, did it work, next
  step), untrusted-content and confirmation rules; byte-stable per frame and
  screen size.
- **Grid screens:** `--grounder <model-id>` lets the planner describe a mouse
  target and asks a grounding model for its point. Grounded clicks and drags
  need a person's approval because grid mode has no element identity.
- **Step sampling:** `--step-candidates K` asks for K replies and executes the
  action with the most matching candidates. Ties choose the earliest reply.
  This is an optional comparison mode, not a measured accuracy improvement.
- **Local safety checks:** `--local-safety-checks` adds approvals for a site
  outside those named in the task and for listed sensitive sites. The optional
  `--injection-monitor MODEL` also checks candidate screenshots for suspicious
  instructions (see [Phase 5](PHASE5_SAFETY_MONITOR.md)). Both remain off by
  default pending live false-positive and accuracy measurements.

Not measured yet: whether this beats Kai's own format for any model. Run the
§7 Phase 2 A/B (two runs per side) before turning it on anywhere.

## Model envelope

Existing Decision JSON remains valid. With v1 enabled the schema also offers:

```json
{
  "thought": "Hover over File",
  "action": "computer",
  "computer": {
    "type": "move",
    "target": {"kind": "element", "observation": 1, "mark": 1}
  },
  "risk": "none"
}
```

Each turn supplies a host-assigned `observation_id`. A point target uses that ID
and `x`/`y` in the exact screenshot's coordinates. The host maps each axis into
the captured native region, including its origin. Missing dimensions, invalid
points, invented marks and stale identities cannot become input. The host
assigns session/request IDs; model text cannot choose the execution session.
Only one canonical action is accepted in a local model envelope; legacy `then`
and `plan` cannot accompany it. `computer: null` permits legacy control replies
with strict structured output. The schema exposes each driver's actual actions,
buttons and scroll units.

Available vocabulary: Observe, Click, Move, Drag, Type, Keypress, Scroll, Wait,
OpenApp and OpenUrl. Shell remains a separately governed existing tool. Done and
AskUser remain engine control flow. Bounds include 8192 UTF-8 bytes for typed
text, two to 256 drag points / 5000 ms, one to eight normalized keys, waits up
to 3000 ms, and requests up to 30000 ms. Scroll deltas are signed on both axes:
at most 4096 pixels or 20 wheel notches per axis. Zero scroll does nothing.

## Current driver boundaries

- Native input reuses Enigo. Move, drag, left/middle/right clicks, keys and
  two-axis wheel scrolling have recording-injector tests. Pixel scroll deltas
  (OpenAI's) become notches by OpenAI's own recipe, one per 100 px, at least
  one, rounded half away from zero, at most 20. Modifiers on mouse actions
  (Shift+click) are held around the action and released on every exit path.
  Back/forward buttons remain unadvertised natively.
- Native Type requires an enabled, focused editable element in the fresh tree.
  Click/focus the field, observe, then type. A click alone is not focus evidence.
- CDP advertises Observe, Click (left, middle, right; back/forward navigate
  history), Move, Drag (left, middle and right), Type, Keypress, pixel Scroll
  (scaled to CSS pixels) and Wait. Click, keys and typing reuse the lane's
  release guards, page dialogs and typed-text wait. Drag issues ordered
  pointer events with the button held; modifiers issue key-down events and
  propagate CDP's modifier mask to every mouse event. Every held key and
  button has a same-session release guard, including on cancellation.
  Back/forward are navigation clicks only; a drag with either is refused.
- Legacy and shared desktop mutations use the same ownership boundary. Ordinary
  terminal subagents remain parallel; known Linux GUI shell launches acquire
  desktop ownership. Legacy actions revalidate after approval/ownership waits.
- Responses batches are bounded to 16 actions (OpenAI documents no maximum)
  and fully checked for arguments/capabilities before any input. Later
  actions require successful predecessor feedback and an unchanged layout. A
  Skip, failure or layout change discards the rest and tells the model which
  actions ran (a "Harness note" next to the screenshot), at most twice per
  run, then hands back to the user.
- OpenAI wire details (from real responses): `action`, `pending_safety_checks`
  and `keys` may be absent, `null` or `[]`; buttons are `left`, `right`,
  `wheel` (middle), `back`, `forward`; key names are uppercase with aliases
  (`RETURN`, `ESCAPE`, `OPTION`, `COMMAND`, `INSERT`); `wait` may carry `ms`
  and defaults to 2000 ms. Screenshots go back as `computer_screenshot` at
  `detail: "original"`, taken unmarked at up to 1600 px.
- Safety checks: a flagged call is shown to the user with each check's code
  and message; only on approval do its actions run, and the next request
  echoes them as `acknowledged_safety_checks` (the API refuses the call
  otherwise). Declining runs nothing. `Unattended` (the bench) always stops.

## Results and privacy

`ToolResult.error: null` means execution returned successfully. It is not task
verification. Errors are bounded categories: InvalidArguments, Unsupported,
StaleObservation, PermissionDenied, Cancelled, TimedOut, Failed and NeedsUser.
The provider's final message asks for task verification; it does not automatically
produce a successful run.

Cancellation reaches queued native input and checks immediately before native
accessibility/launch mutations. Held keys/buttons are released on errors or
cancellation. Native release failure shuts down its input queue. Unacknowledged
browser cleanup targets the original page session and poisons further lane input
until browser restart. Already dispatched OS/protocol operations cannot be rolled
back by dropping an await.

Sensitive entry latches before typing, including failed partial entry. Later
screenshots/crops are suppressed for the run, and known values are redacted from
observations, events, prompts, traces and recording metadata. Auxiliary executor
and verifier calls stop once the latch is active. This is not general OCR
redaction, and does not discover secrets the harness has never identified.

## Release evidence

**Browser shared-tool design (2026-10-16).** Extend the existing CDP input
path to handle pointer drag and mouse modifiers, with release guards attached
to the original tab session. A direct JavaScript drag injection was rejected
because it bypasses browser input and fails on canvases. The acceptance
fixture in `crates/kai-os/tests/browser_computer.rs` checks an actual Chromium
pointer listener, Shift+click release, a held-button drag, unsupported buttons,
and a cancelled drag that poisons the lane until restart. This is browser
driver acceptance only; cross-platform desktop and live-model accuracy still
require their separate gates.

See the [audit and remediation appendix](reviews/AUDIT_2026_10_15.md). Host fixtures are
not Windows/Linux desktop acceptance, mixed-DPI validation, live model accuracy,
or approval-layout visual evidence. Keep the switch off until live benchmarks
(two runs per side), browser contract acceptance and the OS release gates pass.
