# Native Responses protocol completion

## Experimental Holo3.1 function transport (2026-10-17)

`--native-model holo31` sends a single `computer_use` function definition
through the OpenAI-compatible `/chat/completions` `tools` array with
`tool_choice: required`, `parallel_tool_calls: false`, and streaming disabled.
Local llama-server and vLLM clients implement this path. The transport
requires one completed `tool_calls` reply with the matching function name and
string arguments; partial, text-only, unknown, or multiple calls fail closed.
The original JSON argument string reaches the native decoder, which rejects
duplicate keys, unsupported actions, unknown fields, code execution, and
out-of-range coordinates. Actions use Kai's shared executor and safety gates.

Holo3.1-4B's published chat template supports native function calls, with an
XML-formatted `<tool_call><function=...>` generation dialect:
`https://huggingface.co/Hcompany/Holo-3.1-4B/blob/main/chat_template.jinja`.
The serving backend must translate that dialect into an OpenAI-compatible
`message.tool_calls` response. A server that returns the raw XML as plain
text is rejected; the availability and configuration of compatible parsing
must be confirmed against an actual Holo-serving backend.
The `computer_use` function vocabulary is a deliberately bounded Kai schema.
Its provisional normalized 0..1000 coordinate frame and exact server behavior
**still require real Holo3.1-4B calibration**. Fixture tests establish wire
shape, strictness, and shared execution, not real checkpoint success. Tool
definitions are now saved as `tool_spec` in each recorded native call and
exported with the corresponding SFT step. The fixed `tool_choice: required`
transport and model/backend version still need to accompany any dataset
that is reassembled into the server's complete multi-message conversation.

Design note (2026-10-16), continuing `RESEARCH_2026_10_OPENAI_PARITY.md` §3 and §7.

Incomplete generations sometimes contain syntactically valid computer actions.
They must never execute: the adapter returns a distinct `incomplete` reason,
retries the unchanged request once, and stops with a recorded error if the
second response is incomplete. Only a completed response may advance the
previous-response ID, request human approval, or queue actions. We rejected
using partial actions and unbounded retries because either can change the
computer after an uncertain model decision. Scripted HTTP tests cover both
the recovery and stop branches and check that no partial action ran.

`--cua-reasoning-effort <none|low|medium|high|xhigh|max>` supplies
`reasoning: {effort, summary: "concise"}` only on the GA
`--computer-tools-v1` path; the option is absent by default because model
families accept different values. The legacy preview tool never receives the
field. A compatible GA model still needs live validation with a direct
OpenAI key, plus a safe test environment; fixture tests only prove the wire
shape and safety behavior.

## Phase 5 native safety integration (2026-10-16)

`CuaConfig.local_safety_checks` opts the native adapter into the shared Phase 5
rule-based checks; it defaults to `false`. The adapter checks the latest page
and each queued action immediately before dispatch, merging OpenAI's
`pending_safety_checks` with Kai's local checks by code. It asks the human
approver once for the merged list. A refusal stops the queue without input.
For native coordinate clicks, Kai conservatively grounds a hit to a semantic
element only when an observation has screenshot dimensions and the point falls
inside **exactly one** enabled element; the execution target stays a pixel
coordinate. When grounding succeeds, the local `malicious_instructions` rule
can examine native clicks on agent-directed text.

Native observed-link tracking is wired into the same bounded `RunDomains`
policy as the shared engine (`cua/turn.rs` and `cua/provider.rs`). With local
checks enabled, a coordinate click can arm a navigation grant only if it
uniquely hits an enabled accessibility element with `Role::Link`. The provider
records that link's observed source URL when handing off the action and,
**only after a successful action**, offers it to `RunDomains`. A later
observation must show a different destination site before a hop is granted.
The shared policy allows at most two such hops from a task-named, allowlisted
or approved site. Skipped
or failed clicks, other actions, ambiguous hits and clicks without a known
link grant no hop. It never treats a pixel click alone as proof of navigation.

**Tree-only compatibility:** desktop and browser `observe_tree_only()` may
return an **empty screenshot** with an accessibility tree. Coordinate grounding
uses the previously validated full observation's image dimensions instead of
attempting to decode missing tree-only JPEG bytes. Scripted provider tests
deliberately use image-free tree observations to cover link grants, ambiguous
and skipped clicks, target-text injection checks and the maximum two-hop
chain. Real desktop/browser navigation under the native provider remains to
be validated.

The provider acknowledges **only OpenAI's original checks** with unchanged
IDs, codes and messages; generated `kai_sc_*` identifiers never go to the
Responses API. Checks on a sensitive site require fresh confirmation on every
queued action, even after the site's initial acknowledgement. We rejected
approving an entire batch from its first screenshot, because pages and
focused targets can change between actions. This also means multiple calls in
one model response are approved separately as they reach the execution gate.

Scripted HTTP regressions verify the merged provider/local prompt, de-dup by
code, byte-equivalent provider acknowledgement, refusal without a second HTTP
request or input, per-action sensitive-site review, coordinate-click
injection review, and screenshot-free tree mocks for one/two link hops
(including ambiguous, skipped-click and third-hop denials). Shared domain
unit tests exercise delayed landing. `parse_response` continues rejecting
incomplete responses for regular callers; the native network adapter uses
`parse_response_for_retry` to retry incomplete output once without executing
partial actions. On 2026-10-16, `cargo test -p kai-core --test cua_native
--test safety_parity_e2e` passed **30/30 native** and **10/10 Phase 5** tests.
These fixtures reproduce screenshot-free tree observations but do not validate
live desktop/browser navigation or a direct paid OpenAI GA API call.
The companion `cua_native_monitor` loopback suite passed **6/6** tests,
covering the native opt-in monitor, conservative handling of flagged or failed
verdicts, unattended review refusal and cancellation.

The CLI forwards `--local-safety-checks` to the native adapter's `CuaConfig`;
API callers can also opt in by setting that field. This enables local rules
(`irrelevant_domain`, `sensitive_domain`, and target-text
`malicious_instructions` where the click can be grounded). The independent
`--injection-monitor MODEL` also works with the native single-agent path:
`CuaRunner::run_with_monitor` passes the selected model client through the
shared engine's screenshot monitor gate. It remains disabled unless explicitly
selected; positive, failed or malformed verdicts require human safety review
before any action. Unattended runs cannot accept these checks. Native loopback
tests cover opt-out, benign/positive/error verdicts and cancellation while the
monitor is pending; live OpenAI-provider and model-monitor evaluations remain
pending.

Parity status against `RESEARCH_2026_10_OPENAI_PARITY.md`: Phases 1–2 have
fixture-tested wire/protocol handling, Phase 3 has an optional grounder,
Phase 4 has optional candidate voting and guarded bench rollouts, and Phase 5
has opt-in rules plus a shared engine screenshot monitor for native and local
single-agent runs. Phase 6
training-data export is unbuilt. Native API compatibility and on-device
behavior need live tests; real Holo3.1 calibration is pending. No valid
two-runs-per-side accuracy or production safety recall/false-stop measurement
has established parity or justified enabling the experimental flags by default.
