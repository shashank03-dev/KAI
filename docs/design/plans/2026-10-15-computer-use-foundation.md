# Computer-use Foundation Implementation Plan

> **For agentic workers:** Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox syntax for tracking.

**Goal:** Stabilize the audited computer-use boundaries, then introduce a shared bounded action contract for local and provider models.

**Architecture:** Retain the Rust core/OS ports. Normalize provider input into capability-checked actions, guard cancellation before mutations, and sanitize observations before model/UI/recording fan-out.

**Tech Stack:** Rust, Tokio, serde, Enigo, xcap, existing GPUI and CDP.

**Spec:** docs/superpowers/specs/2026-10-15-computer-use-foundation-design.md

## Global Constraints

- Rust 1.95 minimum; owned files at most 500 lines.
- Preserve legacy Decision JSON and existing prompts by default.
- New contract exposure experimental/off by default; safety fixes unconditional.
- No paid model calls, downloads, pushes, deployment or catalog changes.
- Record untested OS behavior explicitly; a mock is not desktop acceptance.

## Review Focus

- Unicode at the byte cap cannot crash or emit invalid UTF-8.
- Cancellation during a provider/approval await cannot start another mutation.
- Partially entered sensitive input cannot leak through images or driver errors.
- Unknown actions/buttons/units cannot silently become a supported mutation.
- Coordinate/session mismatch and zero/diagonal scroll cannot hit another target.

### Task 1: Unicode and cancellation regressions

**Files:** schema.rs; ports.rs; engine/{mod,record,act,batch,execute,procedure,human}.rs; app/control.rs; os/kill.rs; tests/cancellation_e2e.rs.
**Interfaces:** Consume existing Decision/Interrupt. Produce nonblocking Interrupt::cancelled(), cancelled async waits, and final pre-mutation guards.

- [x] Write parser tests for two-, three-, four-byte text crossing byte 8192; run narrow schema tests and observe panic.
- [x] Truncate at floor_char_boundary(8192); preserve byte cap; rerun schema tests.
- [x] Write fake-client/approver tests tripping cancellation before returning; assert zero subsequent actuator mutations and Stopped outcome.
- [x] Add nonblocking cancellation query with compatibility default, override paused app control, race decision/approval waits, and guard side effects centrally.
- [x] Run cancellation integration, existing loop/batch/human tests and core unit tests. Commit.

### Task 2: Sensitive observation boundary

**Files:** engine/{run,human,observe,record}.rs; new engine/privacy.rs; tests/privacy_e2e.rs.
**Interfaces:** Consume Run and observations. Produce a sensitive-input latch before typing; sanitized text/image observations before fan-out.

- [x] Test a visible synthetic code after successful and partial failed typing; assert no later image/crop in model requests or Observed events.
- [x] Latch before actuator access, redact target field and known values, suppress images for the remainder of the run; hand off when useful tree data is absent.
- [x] Test raw actuator error does not reach trace with sensitive payload. Run privacy/human/safety integration. Commit.

### Task 3: Exact approval content

**Files:** app/approval.rs; state.rs if needed.
**Interfaces:** Consume normalized Decision. Produce full selectable action content with a bounded scroll area, fixed footer controls.

- [x] Replace the truncation test with full-action serialization tests distinguishing suffixes beyond character 48.
- [x] Render full JSON in a scrollable region; preserve all Stop/Skip/Approve choices.
- [x] Run app tests and inspect layout code at 720×520; record missing live visual evidence. Commit.

### Task 4: Shared action contract

**Files:** new core/computer/{mod,action,validate,result}.rs; lib.rs; tests/computer_contract.rs.
**Interfaces:** Produce versioned requests, tagged actions, observation transforms, capabilities and explicit statuses. Legacy Decision remains unchanged.

- [x] Test malformed version/session/points/batches, unsupported buttons/units, zero and diagonal deltas; tests initially fail to compile because module is absent.
- [x] Implement pure validation/conversion/coordinate mapping. Limits: type 8192 bytes, drag 2–256 points/5000ms, keypress 1–8 keys, scroll 4096px or 20 notches, wait 3000ms, batch 4.
- [x] Run contract and core tests. Commit.

### Task 5: Executor primitives and common dispatch

**Files:** core/computer/dispatch.rs; os/input/{mod,execute,clipboard}.rs; desktop.rs; web/act.rs.
**Interfaces:** Consume validated requests and cancellation; produce per-action results with capability, session and stale-target checks.

- [x] Add recording-executor tests for no mutation on rejection/cancellation, ordered action results, drag cleanup and diagonal scroll.
- [x] Implement guarded dispatcher and native move/drag/button primitives using existing dependencies; no fake Wait fallback.
- [x] Advertise only implemented capabilities and return Unsupported for unavailable pixel scroll. Keep provider/legacy behavior separate.
- [x] Run contract/OS/input/browser mock tests. Commit.

### Task 6: Provider adapters and release evidence

**Files:** core/cua/{mod,adapter,runner}.rs; settings.rs; MCP boundaries; docs/AUDIT_2026_10_15.md evidence appendix.
**Interfaces:** Consume common dispatch. Produce legacy/current OpenAI protocol parsing and schema-equivalent local-model tool exposure behind computer_tools_v1.

- [x] Test current actions arrays/screenshots, legacy requests, incomplete responses, unknown actions/buttons, safety checks, and text-only completion evidence.
- [x] Implement versioned adapters without routing around policy; preserve explicit unsupported outcomes.
- [x] Run focused tests, fmt, full workspace tests, host clippy, skills and mock bench once. Attempt other OS checks only when tooling is present; report limitations.
- [x] Review whole branch independently, fix substantive findings with red/green regressions, commit evidence and leave worktree available for review.

## Execution ruling

The owner explicitly said "start building" after reviewing the written design.
Proceed inline through the plan without another approval handoff. Preserve scope;
if implementation cannot satisfy a gate, record it rather than claim completion.

## Implementation notes — 2026-10-16

Tasks 1–6 are implemented. The audit appendix records the remaining product
release gates; implementation completion does not close live OS/UI/model gates.
The existing native preview adapter now shares the engine/dispatcher too.
Browser v1 is deliberately narrowed to implemented Observe/Move/Scroll/Wait.
Provider batches preflight four total actions, and abort on skipped predecessors
or changed source context. Ordinary shell agents retain parallelism.

Host verification: `cargo test --workspace` passed 616 tests, with two desktop
integration tests ignored. Workspace clippy passed with `-D warnings` for owned
crates; vendored Ely still emits pre-existing deprecation warnings. Final skill
and mock benchmark results are linked from `docs/AUDIT_2026_10_15.md`.
