# The executor: Gemma plans, jevos carries out routine steps

> **Gemma decides what, jevos decides where/whether, code acts, Kai's rules
> decide safety.**

Most steps of a desktop task are routine: type the name into the Name field,
press Enter, click "Send". Today every one of them costs a full Gemma call
(screenshot or element list in, one JSON action out). The executor lets Gemma
hand over a short **plan** of upcoming subgoals, written in plain words, and has
a small, fast, local **decision model** carry them out one by one. It hands
control back to Gemma the moment it is unsure.

The decision model is **jevos-v4** ([feder-cr/jev](https://github.com/feder-cr/jev),
MIT; 1B parameters, CPU only, OpenVINO int8). It runs as its own binary on
`http://127.0.0.1:8017` and speaks TypeSafe Jev's typed-question wire format.
The author reports 28–130 ms per request on a laptop CPU, and accuracy below
hosted Jev on hard tasks. **Kai has not measured either yet** (see the end of
this page).

The executor is **off by default** (`Config::executor = false`). It stays off
until it is measured on real tasks.

## How it works

```mermaid
flowchart TD
    G[Gemma step<br/>one action + optional plan] --> A1[Act: approvals as always]
    A1 --> P{plan and<br/>executor up?}
    P -- no --> G
    P -- yes --> S[next subgoal]
    S --> O[Observe: element list only<br/>tree-only]
    O --> C[Candidates: elements whose role fits,<br/>ranked by word overlap, top 12 + none]
    C --> J[ONE jevos request:<br/>target · already_done · ready ·<br/>expect_met of the previous subgoal]
    J --> V{previous subgoal met?}
    V -- "p ≤ 0.3, or unsure and nothing changed" --> E
    V -- ok --> GT{gates}
    GT -- "already_done ≥ 0.85" --> SK[skip, reuse the observation] --> S
    GT -- "ready < 0.7 · target none · p < 0.6 · margin < 0.2" --> E[Escalate: note to Gemma,<br/>screenshot if anything ran]
    GT -- pass --> R[risk::classify + approval<br/>exactly like a model step]
    R -- approved or not needed --> ACT[Act · record step<br/>backend 'executor (jevos)']
    R -- skipped --> E
    ACT --> S
    S -- "plan used up / done subgoal" --> F[final expect_met check] --> G
    E --> G
```

### 1. The plan (Gemma)

When the executor is on, the system prompt gets one extra rule
(`prompt::PLAN_RULE`) and the reply schema gets an optional `plan`
(`StepReply::json_schema_with_plan`):

```json
{ "thought": "fill the form", "action": "type", "target": 3, "text": "Ada", "risk": "none",
  "expect": "Name holds Ada",
  "plan": [
    { "do": "type",  "target": "Email field", "text": "ada@x.org", "expect": "Email holds ada@x.org" },
    { "do": "click", "target": "Send message button", "expect": "message sent" },
    { "do": "done",  "target": "", "expect": "" } ] }
```

- `do`: `click | type | key | select | open_url | open_app | shell | done`.
- `target` names the element in words, not by number. The screen will have
  changed by the time the subgoal runs, so a number from now would be stale.
- At most 6 subgoals (`plan::MAX_PLAN`). The first invalid item ends the plan
  (`plan::parse_plan`). `done` is always last. A `done`/`ask_user` reply
  ignores its plan.
- It is backward compatible. With the executor off, neither the prompt nor the
  schema mentions `plan`, so behaviour is exactly as before. A stray `plan` in
  a reply is parsed and ignored.

### 2. Ground (code + jevos)

For each subgoal, the executor takes a fresh observation. It is tree-only
whenever tree-only steps are allowed: jevos reads text only, so a screenshot
would be wasted. The observation still goes through the normal observe path,
so no-progress tracking and the UI's `Observed` event work as usual.

`executor::candidates` keeps **enabled** elements whose role fits the subgoal:

| `do` | roles |
| --- | --- |
| `type` | text field, password field, combobox |
| `click` | button, link, checkbox, radio, tab, menu item, list item, combobox |
| `select` | combobox, list item, menu item, radio |

They are ranked by word overlap between `target` and the element's name and
value (filler words like "field" and "button" are ignored), and the top 12 are
kept. Then **one** request goes to jevos (`executor::questions`):

| id | type | asked when |
| --- | --- | --- |
| `target` | choice. Criteria: `"3": "#3 textfield \"Email\""`, …, plus `"none"` | the subgoal needs an element |
| `already_done` | noul: "Is this already true: \<expect\>?" | not for `shell` |
| `ready` | noul: "Can '\<subgoal\>' be done on this screen right now?" | always |
| `expect_met` | noul: "Is this true now: \<previous expect\>?" | a previous subgoal ran |

The state is a JSON object: the task, the subgoal, its expectation, the
previous subgoal, and the compact element list (capped at 12,000 characters,
about 3K of jevos' 8,192-token context).

**Refinement over the brief.** The brief asked for a separate `expect_met`
request after each action. Here it rides on the *next* subgoal's grounding
request, so N subgoals cost N + 1 requests instead of 2N. The last subgoal gets
one standalone check.

### 3. Gates (constants in `executor::Gates`, tunable via `Config::executor_gates`)

| gate | default | why |
| --- | --- | --- |
| skip if `already_done ≥` | 0.85 | Re-typing or re-clicking something that already holds is the classic double-submit bug. The bar is high because a wrong skip loses a step. |
| act only if `ready ≥` | 0.7 | A dialog, a spinner or a disabled control means "not now", so the decision goes back to Gemma. |
| target confidence `≥` | 0.6 | Below this, the 1B model is guessing. |
| top-option margin over the runner-up `≥` | 0.2 | Two similar elements ("Send" and "Send later") is exactly where a small model fails. When only the top probability `p` is sent, the margin is the lower bound `2p − 1`. |
| target ≠ `none` | — | The element the plan expects is not on screen. |
| after acting: `expect_met ≥` | 0.8 | The subgoal is confirmed (the step is marked verified, and an unchanged element list does not count as "no progress"). |
| after acting: `expect_met ≤` | 0.3 | It did not work. Escalate and force a screenshot. In between (or unknown), an unchanged screen means it did not work. |
| subgoals per plan | 6 | Never more than the plan. |
| cooldown after two escalations in a row | 3 model steps | A plan that keeps failing costs a jevos round trip per Gemma step for nothing. |

The defaults are first guesses chosen to escalate rather than act. They need
real jevos measurements before tuning (see the end of this page).

### 4. Act (code) and safety (Kai's rules)

An executor action is an ordinary `Decision` (`Subgoal::to_decision`,
validated and clamped). It goes through **the same `risk::classify` and
`risk::needs_approval` as a model step**, and the approver sees it just as it
would see a model step. Nothing in the gates can lower a risk:

- "Send", "Delete", "Pay" and the like are high risk by effect words, so the
  approver is asked under the default level.
- A password or secure field always asks, even on autopilot.
- A skipped approval ends the plan and tells Gemma.
- jevos is never asked about safety, and its answers never decide an approval.

Each step is recorded with backend `executor (jevos)`. `StepRecord::executor`
holds the subgoal, target p, margin, ready, already_done, the later
`expect_met`, and the request latency. `Trace::executor` holds the run's
counters: plans, decisions, acted, skipped, escalations, every latency, and the
health-check result. The UI gets the usual `Decided` / `AwaitingApproval` /
`Acted` events.

### 5. Hand-back

Control returns to Gemma (`r.notes` carries the reason into its next turn) when
any of these happens:

- the executor is unsure (any gate fails; the note gives target p, margin and
  ready);
- the previous subgoal's result does not hold;
- nothing changed and the result is unconfirmed;
- an act fails;
- the user steers (the message is folded in as usual);
- the run is interrupted (it stops);
- the step limit is reached;
- jevos errors (a transport or availability error turns the executor off for
  the rest of the run);
- the plan reaches `done`. Gemma must confirm with a real look: the executor
  never declares a task done;
- the plan runs out.

After any executor action, Gemma's next look is a full screenshot, because it
has not seen what the executor did. Gemma re-plans on its next step.

## Failure modes and what guards them

| failure | guard |
| --- | --- |
| jevos picks the wrong element | role filter + confidence + margin gates; `expect_met` on the next request; approvals for anything risky |
| it types into the wrong field | the same, plus `already_done` stops a re-type; field values are in the element list |
| it acts on a stale screen | a fresh observation per subgoal; `ready` gate |
| it skips a step that did not happen | `already_done` ≥ 0.85; Gemma sees the screen after the plan |
| it loops on a failing plan | escalation on the first doubt; cooldown after two in a row |
| jevos is down or slow | health check at the start of the run (run without it); a request error turns it off for the run; 3 s request timeout |
| wrong "task done" | the executor never ends a run; a `done` subgoal hands back to Gemma |
| a prompt-injected page tells it to click "Pay" | approvals are deterministic (`risk.rs`), independent of jevos |
| the context overflows (8,192 tokens) | the element text is capped at 12,000 chars, top 12 candidates |

## Running it

1. Run jevos locally ([feder-cr/jev](https://github.com/feder-cr/jev)):
   `./jev serve` listens on port 8017. Kai does not download or run it for
   you.
2. Check it: `kai doctor` prints the executor line, which says whether the
   endpoint is reachable and the median and maximum `/health` round trip over
   5 calls.
3. Point Kai elsewhere if needed: the `executor_endpoint` setting (default
   `http://127.0.0.1:8017`) or env `KAI_EXECUTOR_URL`. If `JEV_API_KEY` is
   set, it is sent as a bearer key; otherwise no `Authorization` header is
   sent.
4. Use it: `kai run --executor "…"`, or A/B on the benchmark:

   ```sh
   kai bench --backend gateway --only <ids>              # executor off
   kai bench --backend gateway --only <ids> --executor   # executor on
   kai bench report
   ```

   Results files record `ab.executor`, and per task and in total
   `executor_decisions`, `executor_escalations` and `executor_steps`, plus
   `executor_p50_ms` / `executor_p95_ms` (decision-request latency) and
   `executor_available_tasks`. The Markdown report has an "Executor" row.

If the endpoint is down, every run logs it, records `executor.unavailable` in
the trace, and runs exactly as without `--executor`. It never fails the run.

## What is measured, and what is not

**Measured (mock only).** Scripted Gemma replies (`MockClient`) plus a scripted
fake jevos, through the real engine
(`crates/kai-core/tests/executor_e2e.rs`; run it with
`cargo test -p kai-core --test executor_e2e -- --nocapture`):

| scripted task | Gemma calls, executor off | Gemma calls, executor on | jevos requests | actions |
| --- | --- | --- | --- | --- |
| contact form (name, email, message, send) | 5 | 2 | 4 | 4 (same actions in both) |
| search and open the result | 5 | 2 | 4 | 4 (same actions in both) |

These counts only show the mechanism. The fake jevos is always right and
confident, so they are the best case, not a prediction.

**Not measured (pending a real jevos run).**

- jevos latency on Kai's prompts (the author reports 28–130 ms).
- Its target accuracy and calibration on real element lists.
- How often the gates escalate in practice.
- Wall-clock and success-rate effects on `bench/tasks.yaml`.

The next step is to run `./jev serve` on the bench machine and compare
`kai bench --executor` with a plain run (n ≥ 3 each), then tune the gates from
the recorded `StepRecord::executor` numbers.
