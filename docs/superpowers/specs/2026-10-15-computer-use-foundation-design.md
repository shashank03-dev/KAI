# Kai computer-use foundation

Date: 2026-10-15. Status: proposed written specification for review.
Baseline: `1bc97a65cbd18373ca1feceaf35ee646de89e5bd`.
Related evidence: `docs/AUDIT_2026_10_15.md` (A01–A20).

## Intent and scope

The owner wants local open models to receive a capable, consistent computer-use
environment, with Rust responsible for orchestration and policy. Reuse existing
implementations when they provide a proven benefit. Tool execution, cancellation,
observations and outcome evidence must be dependable independently of the model.
Start with the audit blockers and shared tool contract; model catalog expansion
follows later. Do not require an OpenAI account for local operation.

This first milestone closes A01–A07 and establishes the capability, coordinate,
privacy and test foundations needed by A11–A14. It is not a claim of full audit
closure. General UI scrolling/responsiveness (A08–A10), complete multi-monitor
support, clipboard preservation (A15), artifact deletion (A16), catalog profiles
(A17), release workflow changes (A18), and dead launcher/design cleanup (A19–A20)
remain explicitly tracked subsequent changes. Permissions redesign and embeddings
are outside this milestone. No frontier-model accuracy or seconds-per-task promise.

## Research and reuse decision

Research fetched on 2026-10-15. Public sample revision inspected:
`f2a3dc523ae406f9b704f9a420a05402a63b4522`; LICENSE SHA-256:
`13df7812ca53ecaae1cb4a868844bb598373047ae1d580e4debfbef1dd5b6915`.

- [Official API guide](https://developers.openai.com/api/docs/guides/tools-computer-use):
  persistent code execution and structured UI actions are both integration choices;
  existing custom tools can remain the interface.
- [Official public sample](https://github.com/openai/openai-cua-sample-app):
  MIT-licensed examples use Python/PyAutoGUI and JavaScript/Playwright. Its README
  distinguishes the samples from OpenAI product runtimes and production controls.
- [Python worker](https://github.com/openai/openai-cua-sample-app/blob/f2a3dc523ae406f9b704f9a420a05402a63b4522/python-app/app/desktop/worker.py)
  and [supervisor](https://github.com/openai/openai-cua-sample-app/blob/f2a3dc523ae406f9b704f9a420a05402a63b4522/python-app/app/desktop/runtime.py):
  examined request correlation, persistent state, output limits and interruption.
  The sample executes supplied Python; adopting it unchanged would bypass Kai's
  per-action approval and observation sanitization.
- [PyAutoGUI documentation](https://pyautogui.readthedocs.io/en/latest/):
  mouse movement/dragging are available, but primary-monitor limitations and
  default action delays make it an unsuitable automatic cure for Kai's gaps.
- [Enigo 0.6.1](https://docs.rs/enigo/0.6.1/enigo/trait.Mouse.html):
  Kai already has movement and button primitives; its missing move/drag contract
  can be implemented using that dependency. Scroll notches and pixel deltas must
  remain different units.
- [Playwright](https://playwright.dev/docs/intro): a reusable browser automation
  option if Kai's current CDP lane lacks a required browser capability.

Three approaches considered:

| Approach | Benefit | Cost / decision |
| --- | --- | --- |
| Rust orchestration and current native libraries, with executor adapters | Retains current ports, local deployment and policy; permits selective reuse | **Selected.** Add missing semantics rather than a second parallel harness |
| Adopt the public Python/JavaScript sample wholesale | Immediately supplies a persistent code runner | Requires replacing its model loop and policy; extra runtimes; not product Codex parity |
| Reimplement every OS/browser API in Rust | Complete implementation ownership | Unnecessary maintenance; reject where maintained dependencies suffice |

Reuse the existing xcap, Enigo, AX/UIA/AT-SPI and Chromium protocol integrations.
Use upstream examples as protocol/lifecycle references and possible evaluation
fixtures. No upstream source is copied or dependency installed by this spec.
Any later copied source must pin a commit, retain its license and document changes.
No private Codex executable, internal service, app credentials or subscription
session becomes a runtime dependency. No evidence currently identifies a required
primitive that is impossible through Rust or its existing native dependencies.

## Architecture

```text
Local model / provider API / MCP
             |
       provider adapter
             |
    versioned ToolRequest
             |
 Rust dispatcher: validate -> capability -> privacy/risk -> approval -> cancellation
             |
  executor: native desktop | current CDP | optional future supervised bridge
             |
     ToolResult + fresh sanitized Observation
             |
     model feedback / UI / trace / verifier
```

`kai-core` owns contracts, validation, risk, cancellation coordination and results.
`kai-os` owns screen/input/session resources and OS-specific execution. `kai-app`
and `kai-cli` consume the same dispatcher. Provider response parsing must never
call a driver directly. MCP keeps its existing stricter headless approval policy.
The native OpenAI comparison runner must use the same dispatcher and approval
path rather than its current shortcut directly to the actuator.

A desktop session has one mutation owner. Browser sessions have their own IDs;
their shell/app-launch fallback must acquire the desktop owner. Existing team
terminal concurrency remains possible without simultaneous screen mutation.
Cross-process desktop locking is a later deployment requirement; this milestone
does not claim concurrent independent Kai processes are safe.

## Shared tool contract v1

The contract revision is independent of a model/provider name. Preserve the old
`Decision` JSON as an adapter; do not add optional fields to every existing struct
literal at once. Introduce a separate tagged action representation and normalize
both old decisions and provider calls into it.

Proposed core type boundaries (exact implementation details belong in the plan):

- `ToolRequest`: version `1`, request ID, session ID, optional observation ID,
  one `ComputerAction`, and an execution deadline.
- `ComputerAction`: `Observe`, `Click`, `Move`, `Drag`, `Type`, `Keypress`,
  `Scroll`, `Wait`, `OpenApp`, `OpenUrl`; shell remains a separate existing
  capability. `Done` and `AskUser` remain harness control flow.
- `Target`: an element reference bound to an observation, or a point in the
  exact screenshot coordinate space. A point always carries an observation ID.
- `Capabilities`: supported actions, click buttons, scroll units, target spaces,
  image/tree availability, OS/session restrictions and executor identifier.
- `ToolResult`: request/session IDs, status, execution path, elapsed time,
  sanitized error, whether a fresh observation is required, optional evidence.
- `ToolStatus`: `Executed`, `Unsupported`, `StaleObservation`, `PermissionDenied`,
  `Cancelled`, `TimedOut`, `Failed`, `NeedsUser`. `Executed` is not proof of outcome.

Use integer request/observation sequences scoped to the session. Reject unknown
versions, unknown action fields, nonfinite numbers and mismatched sessions before
OS calls. Never convert an unsupported action or button into a different action.

### Primitive semantics and limits

| Primitive | Required meaning / bounds |
| --- | --- |
| Observe | Full sanitized screenshot and/or accessibility snapshot; no mutation |
| Click | Explicit supported button; count 1 or 2; exact target |
| Move | Move/hover without clicking; exact target |
| Drag | Explicit button; 2–256 points; duration 0–5,000 ms; finally release owned button |
| Type | Explicit target or fresh focused editable field; UTF-8 cap 8,192 bytes at a character boundary |
| Keypress | 1–8 known keys; modifiers precede one final key; finally release owned keys |
| Scroll | Explicit target; signed `dx` and `dy`; unit `pixels` or `wheel_notches`; both axes retained |
| Wait | 0–3,000 ms; cancellation aware; no fabricated input |
| OpenApp / OpenUrl | Existing launcher/address policy; report launch separately from visible readiness |

For new scrolling, bound absolute pixel deltas to 4,096 per axis and wheel notches
to 20 per axis. Zero deltas are successful no-ops. Reject over-limit explicit
contract arguments rather than silently changing their meaning. Legacy decisions
retain their documented validation/clamping and normalize `direction/amount` to
wheel notches for desktop; legacy browser behavior stays in its versioned adapter.
The new contract uses the same declared units in every lane. Unsupported pixel
scrolling returns Unsupported; do not approximate pixels as wheel notches.

Bounded provider action batches contain at most four actions. Validate the entire
batch structurally before executing any action, then apply policy before each
action. Return each result in order and stop on failure, cancellation, changed
targets or required approval. No silent truncation of oversized batches. A later
action that depends on changed layout requires a fresh target observation.

## Coordinates and freshness

Observations carry session/observation IDs, capture region in native input units,
encoded image dimensions and display/window identity. Transform screenshot points
using separate X/Y ratios and the captured origin. macOS points, Windows desktop
coordinates and browser CSS pixels are explicit executor spaces. Reject points
outside the observed image rather than clamping them onto an unrelated control.

Before a coordinate side effect, validate target window/display geometry and screen
state against a fresh capture; a changed context returns StaleObservation. An
element action may instead resolve the original native reference and validate its
identity and eligibility immediately before execution. Mark numbers alone are
insufficient. Reobserve after layout mutations and before follow-ups targeting them.

This milestone supports the existing selected primary display and browser viewport,
with correct origins and scaling. Other desktop displays are advertised unsupported
until tested. Repair the corner gesture to check a bounded region of the selected
monitor; negative coordinates alone must not trip it.

## Cancellation and resource cleanup (A02)

Use one cancellation signal shared by run, dispatcher and executor input queue.
Keep pause distinct: pause holds between actions, while cancellation is observable
without blocking inside `Interrupt::stopped()`. Existing interrupt implementations
need a nonblocking cancellation query for racing in-flight futures.

Race model requests and approval waits against cancellation and the run deadline.
Recheck immediately before every side effect, including retries, batched actions,
skill/cache/procedure execution and native provider calls. Queued input commands
carry the session/cancellation signal so the input thread rejects cancelled work.
The cancellation linearization point is the last check before an OS call. Already
submitted OS input cannot be recalled; no subsequent primitive may start.

Use owned-input guards to release every button/key Kai pressed, including partial
chords and failed drags. Report cleanup failure explicitly and invalidate the
session. Do not release unrelated keys held by the person. An optional future
sidecar must have the same cancellation, cleanup and request-correlation contract;
terminating a process alone is not proof its held input was released.

Acceptance target: fake executor dispatch after cancellation is zero; local
queued-input cancellation is observed within 100 ms under the fixture workload.
Measure real OS response separately; do not advertise a universal latency bound.

## Privacy and approvals (A03–A04)

All model inputs, events, recordings, crops and viewer artifacts pass through a
single observation sanitization boundary before fan-out. Raw captures are memory
only and must not be logged or persisted. The model never chooses privacy rules.

For this milestone, use a conservative rule: before entering a harness-supplied
one-time code or detected typed secret, latch `SensitiveInput` on the session,
including partial/failed typing. Suppress all subsequent outgoing screenshots and
crops for that session, rather than assume field-box masking can catch copied or
relocated text. Redact known secret values from all outgoing text and field values;
mask the sensitive target's entire text value even after partial typing. Driver
errors and logs must not include raw input payloads or worker traceback locals.
If safe tree-only continuation is unavailable, hand control to the person. Do not
clear the latch automatically after Enter, navigation or a field disappearing.
Ending the session is required to resume image delivery.

This provides a defined boundary for secrets entered through Kai's sensitive path.
It does not promise detection of arbitrary pre-existing secrets anywhere on screen.
Screen-copy wording must state that limit. A later privacy design can offer
less restrictive modes after their own tests; none bypasses this latch implicitly.

Approvals show the exact normalized action in selectable wrapped/scrollable text,
with the destination, button/coordinates or full command as applicable. A compact
summary may truncate only while the full action remains inspectable. Stop, Skip
and Approve remain visible at 720×520. Bind approval to the specific normalized
action and target context; a material context change requires new review.

## Provider and local-model adapters (A05–A06)

Keep a versioned legacy OpenAI-preview adapter for historical comparisons, and add
a current adapter for `computer`/ordered `actions`/`computer_screenshot`. Both
normalize into the shared dispatcher. Handle unsuccessful/incomplete responses,
refusals and unsupported output explicitly. A text-only final response means the
model reported completion, not verified completion. Required safety acknowledgments
must go through user review; never synthesize approval from screen content.

Local models keep existing inference clients and receive the same primitive
descriptions, limits, capabilities and feedback. Models with structured tool calls
use schema function calls; existing JSON models use the compatibility adapter.
Do not require generated Python for small models. Model-specific prompts and
catalog recommendations are later work. Any future generated-code runtime needs
an explicit isolation and policy design before exposure; stdio is not a sandbox.

## Results, measurements and rollout

Keep task outcome and completion evidence separate. Existing trace formats remain
readable using optional/defaulted evidence fields. UI labels distinguish model
reported completion from independently verified completion. Do not promote an
unverified model claim into a verified procedure or benchmark pass.

Correctness fixes A01–A04 apply to the existing path. Contract-v1 exposure and new
provider adapters are experimental, off by default under `computer_tools_v1` until
repeated live A/B runs establish their value (at least two runs per side). Guards
and privacy corrections cannot be disabled with that feature switch. Old prompts
and schemas stay byte-stable when the feature is off, except necessary safety copy.

Measure observation, dispatch, input, settling and inference separately; collect
p50/p95, cancellations, errors, task outcomes and RAM. Report the tested machine,
model and sample size. No new GPU/model downloads, paid endpoint runs, hosted
workflows or releases are needed for the unit/contract milestone.

## File responsibility map

- `kai-core/src/computer/{mod,action,observation,result,validate,dispatch}.rs`:
  contract, compatibility conversion and common policy boundary.
- Existing `engine/{mod,act,batch,execute,procedure,human,record}.rs`:
  cancellation at await/side-effect boundaries and sanitized feedback.
- `kai-core/src/cua/`: split the oversized single runner into adapter and loop
  responsibilities before adding current protocol support.
- `kai-os/src/input/`: split input queue, primitive execution and clipboard code;
  shared cancellation and owned-input cleanup belong here.
- `kai-os/src/{desktop,perceive,frame,kill}.rs` and `web/{act,look}.rs`:
  capability reporting, transforms, input execution and privacy-safe capture flow.
- `kai-app/src/{agent,approval,state}.rs`: sanitized fan-out, exact approval content
  and truthful completion evidence; split agent.rs to respect the 500-line rule.
- `kai-core/src/mcp.rs`: expose the contract through existing stricter policy.
- Tests: focused contract, cancellation, image privacy, provider fixtures and
  no-network/no-desktop fake executors; OS fixtures remain explicitly separate.

## Acceptance gates

1. UTF-8: byte boundaries across 2/3/4-byte characters; long commands cannot panic.
2. Cancellation: during model call, repair retry, approval, launch, queued input,
   key chord, drag and shell; cleanup checked even after driver failure.
3. Privacy: synthetic visible code remains present in the fake page after typing;
   no image/crop is sent, emitted or saved; known secret text is absent from every
   output. Include partial typing, worker errors and an unavailable tree.
4. Approval: two commands differing after character 48 are fully distinguishable;
   multiline Unicode payload and Stop/Skip/Approve usable at minimum size.
5. Contract: no side effects for unsupported action/unit/button, malformed batch,
   stale observation, invalid coordinates, nonfinite values or mismatched session.
6. Primitive equivalence: drag changes a fixture object position; hover opens a
   fixture menu; key release survives failure; diagonal scroll retains both axes.
7. Provider fixtures: current and legacy schemas, ordered results, failures,
   safety handoff, incomplete response, no-call completion and cancellation.
8. Compatibility: existing skills/traces/JSON decisions remain readable; MCP
   cannot gain shell or high-risk execution through a new action representation.
9. Native validation: macOS, Windows and Linux builds/clippy and their desktop
   fixture runs must be recorded independently before each lane is marked ready.
   Missing OS runners remain unverified, never passed by inference.
10. Rollout: selected regression suites, full workspace suite, formatting, skill
    validation and mock benchmark; live A/B required before experimental defaults
    change. Update audit IDs individually with evidence, leaving unresolved IDs open.

## Next stage

Review this written design before implementation planning. The plan should start
with the independent regression fixes (A01–A04), then introduce contract/types,
native primitives, common dispatch and provider adapters in reviewable increments.
Use native execution in this chat unless the owner requests another method.
