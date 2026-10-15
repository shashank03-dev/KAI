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
[approved design](superpowers/specs/2026-10-15-computer-use-foundation-design.md).

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
  two-axis wheel scrolling have recording-injector tests. It advertises wheel
  notches, not pixel scrolling. Back/forward buttons remain unadvertised.
- Native Type requires an enabled, focused editable element in the fresh tree.
  Click/focus the field, observe, then type. A click alone is not focus evidence.
- CDP v1 currently advertises Observe, Move, pixel Scroll and Wait. Shared
  click/type/key/drag remain unavailable pending browser acceptance. Legacy
  browser actions still use the existing lane with cancellation/release guards.
- Legacy and shared desktop mutations use the same ownership boundary. Ordinary
  terminal subagents remain parallel; known Linux GUI shell launches acquire
  desktop ownership. Legacy actions revalidate after approval/ownership waits.
- Current Responses arrays are bounded to four total actions and fully checked
  for arguments/capabilities before any input. Later actions require successful
  predecessor feedback and unchanged original image/tree context. A Skip,
  failure or layout change discards the remaining queue and hands back to the
  user. This conservative behavior needs live A/B evaluation before rollout.

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

See the [audit and remediation appendix](AUDIT_2026_10_15.md). Host fixtures are
not Windows/Linux desktop acceptance, mixed-DPI validation, live model accuracy,
or approval-layout visual evidence. Keep the switch off until live benchmarks
(two runs per side), browser contract acceptance and the OS release gates pass.
