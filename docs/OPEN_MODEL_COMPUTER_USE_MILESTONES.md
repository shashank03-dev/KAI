# Open-model computer-use engineering milestones

Updated 2026-10-17. This checklist tracks the objective in
`reviews/OPEN_MODEL_COMPUTER_USE_SCORECARD_2026_10_16.md`. **An implemented feature is
not a completed milestone until its acceptance tests and live evidence exist.**
The historical 6/10 readiness estimate has not been superseded by a measured
score.

## 1. Baseline, architecture and research — in progress

- [x] Inspect existing computer control, safety, model backends, datasets and
  benchmark infrastructure; record the initial estimates and limitations in
  `reviews/OPEN_MODEL_COMPUTER_USE_SCORECARD_2026_10_16.md`.
- [x] Prioritize native model integration, coordinate calibration, measured
  verification, browser-code execution and representative benchmarks.
- [ ] Establish a reproducible **live** baseline for the installed Mac,
  including actual local checkpoint/backend, wall time, RAM, tokens and
  independently checked task outcomes.

## 2. Native model compatibility — in progress

- [x] Fara1.5 XML action and UI-TARS textual action parsers, conversion to
  canonical computer actions, and scripted parser/executor tests.
- [x] Record the actual prompt and model-visible screenshot for accepted native
  replies, and resolve the system prompt **per step** during SFT/JSONL export.
  See `native_trace_e2e.rs` and `data/tests.rs`.
- [ ] Verify Fara1.5 and UI-TARS from authentic checkpoint outputs through
  supported serving backends, with retries, cancellation and safety checks.
- [x] Implement a strict Holo3.1 **function-calling transport** for local and
  vLLM OpenAI-compatible chat servers: force one `computer_use` function call,
  reject partial/multiple/unknown calls, preserve raw arguments for duplicate
  key rejection, and route decoded actions through the shared executor.
  Scripted OpenAI-compatible and vLLM HTTP, parser and engine regressions cover
  the contract. The fixture includes a valid observation region so the shared
  coordinate validation and execution actually run.
- [x] Save each native function definition with the model call and include it
  in `all`/`sft` dataset exports; test recorded Holo traces and JSONL export.
- [ ] Confirm actual Holo3.1-4B tool-call generation from a checkpoint/server.
  The 0..1000 coordinate convention is provisional until live calibration.
- [ ] Publish a per-checkpoint/backend matrix with reproducible native
  parsing, licensing, hardware-fit and multi-step acceptance results.

## 3. Grounding and computer actions — in progress

- [x] Canonical action validation and shared execution/supervision boundaries
  exist; two screenshot sizes are covered by synthetic native trace tests.
- [ ] Collect >=10 independent **real model predictions** at 1280x800 and
  1440x900 for each checkpoint; require >=9/10 target hits at each resolution
  before adopting that model's coordinate convention.
- [ ] Run browser/desktop acceptance for click, drag, modifiers, nested scroll,
  file dialogs and scaling; record OS and driver versions.

## 4. Persistent sandboxed code execution — in progress

- [x] Implement an experimental persistent QuickJS worker with bounded
  memory, stack, script size, microtasks, logs, actions and time. No host
  filesystem, network or process modules are exposed. Scripts enqueue typed
  browser operations rather than executing in the page origin. This is
  capability isolation; stronger OS process isolation remains outstanding.
- [x] Connect the worker's operations to the existing `Dispatcher` with an
  explicit host `ComputerPolicy` and post-authorization selector, document,
  stable DOM backend node ID and focus freshness checks. The real-browser
  tests exercise refusal, rejected navigation, same-URL document replacement,
  identical cloned elements and shifted focus.
- [ ] Expose script execution as a model-facing tool through the engine's
  **full** safety/approval rules, trace and outcome verification. The worker
  currently has no production caller; do not expose an allow-all policy.
- [ ] Add real Playwright-compatible methods, screenshot outputs and stronger
  worker process sandboxing before claiming the original full requirement.
- [x] Pass a real two-call dynamic browser fixture with an independently
  checked initial/intermediate/final browser state and time-limited script;
  see `kai-os/tests/browser_script.rs`.
- [x] Pass a dedicated external runtime cancellation test in required Chromium
  on macOS, proving queued clicks do not execute after cancellation. Include
  disabled/occluded target and stale typing/focus regressions; request IDs
  are monotonic across calls.
- [ ] Repeat browser-script acceptance on headed macOS, Windows and Linux.

## 5. Independent verification and recovery — in progress

- [x] Outcome ledger, verification gate, action effect detection and recovery
  routing exist behind configuration flags.
- [ ] Verify each benchmark oracle on untouched, known-good and plausible
  incorrect states; measure false completion and recovery success.
- [ ] Validate bounded retries and no-op/dialog/navigation recovery in real
  browser and desktop tasks.

## 6. Performance optimization — unmeasured

- [ ] Capture task p50/p95 time, model calls, action count, tokens, peak RAM,
  failure labels, retries and safety stops per configuration.
- [ ] Evaluate screenshot reduction/routing against the same tasks; adopt only
  changes that preserve verified success.

## 7. Platform and integration — partially tested

- [x] Existing OS abstractions and Rust scripted native integration tests.
- [x] Run `KAI_REQUIRE_BROWSER=1` acceptance on a real Chromium-installed Mac.
  Playwright Chromium headless shell 153.0.8010.12: 12 passed, 1 intentionally
  ignored (requires a headed desktop); discovered a same-profile race and
  fixed it with an exclusive OS-managed browser-profile lock.
- [ ] Repeat headed-desktop and cross-platform acceptance on appropriate hosts.
- [ ] Record real macOS, Windows and Linux driver capabilities separately,
  including accessibility, browser CDP, Wayland and multi-display limitations.
- [ ] Measure safety refusals and prompt-injection false positives on realistic
  fixtures. Keep `.github/workflows/ci.yml` **manual-only**.

## 8. Final comparative evaluation — pending

- [ ] Pin/reset tasks, fixtures, assets and environments; run >=2 attempts per
  configuration and report per-task independent outcomes and confidence bounds
  where sample size permits.
- [ ] Compare the original Gemma flow, validated native open models and an
  available GPT computer-use baseline under comparable conditions.
- [ ] Reassess the readiness dimensions from evidence. Do not assign the 9/10
  target by assumption.

## Quality gate checkpoint (2026-10-17)

- Latest Milestone 4 browser-script changes: five real-Chromium acceptance tests
  **passed** with `KAI_REQUIRE_BROWSER=1` on macOS; `cargo test -p kai-os
  --quiet` **passed** (149 unit tests, runnable integration tests, two
  intentionally ignored desktop/headed tests).
- `cargo clippy -p kai-os --all-targets -- -D warnings`, targeted `rustfmt
  --check`, and `git diff --check` all **passed** after the latest browser
  script hardening. The existing tree-wide formatting gate below remains
  unchanged.
- `cargo test --workspace --quiet` **passed** during the Milestone 4 work;
  the final DOM backend node identity change was subsequently checked again
  by the complete `kai-os` test suite and targeted Clippy.
- `cargo clippy --workspace --all-targets -- -D warnings`: **exit 0**, with
  deprecation warnings emitted by the vendored `ely-gpui-component` dependency.
- `cargo fmt --all --check`: **exit 1**; formatting differences in multiple
  previously modified Rust files. Preserve unrelated edits and review the
  intended formatting scope before changing them.
- `cargo test --workspace --quiet`: **exit 0** before the latest Holo changes;
  one desktop/toolkit test and one headed browser test intentionally ignored.
- Focused recorder missing/corrupt hash tests, CLI dataset export and four
  native trace integration tests: **passed** after the recorder fix.
- Required real-browser suite (`KAI_BROWSER` set to the local Playwright
  headless-shell path, `KAI_REQUIRE_BROWSER=1`): **12 passed, 1 ignored**.
- Holo3.1 transport: fixture coverage added; needs final targeted and workspace
  verification plus live checkpoint validation.
- Live open-checkpoint calibration and direct GPT comparison: **not yet run**.

## Next independent work

1. Verify the Holo3.1 tool-call adapter end to end against a real local
   checkpoint, including serving backend tool-call parsing. The official 4B
   chat template emits XML function calls; the server must expose them through
   the OpenAI-compatible `tool_calls` response that Kai expects.
2. Collect 10 independent target predictions at each of 1280x800 and
   1440x900 for each candidate, measuring native coordinate behavior.
3. Run headed desktop, Windows and Linux acceptance; implement the isolated
   persistent browser-code worker under Kai's shared safety layer.
4. Establish fair verified open-model/GPT comparison and update the score.

Do not reset the heavily modified working tree or overwrite unrelated changes.
No commit, push, merge or deployment is required for these milestones.
