# Kai open-model computer-use readiness — 16 October 2026

**Assessment: 6/10 overall, 8/10 for the underlying action harness, and
roughly 4/10 for demonstrated plug-and-play reliability with diverse open
computer-use models.** These are an engineering assessment of code, existing
benchmarks and the machine inspected today, **not** a measured success rate or
an externally comparable leaderboard score. It is too early to claim that Kai
beats GPT-6 computer use across applications.

The practical question is not whether every mouse/keyboard primitive exists.
It is whether each model gets the *observation, native prompt/action language,
coordinate mapping, session, safety policy and verifier* it was trained to use,
on every supported platform, and whether a real task passes automatically.

## Evidence and dimensions

| Dimension | Score | What the repository currently supports | Main qualification |
| --- | ---: | --- | --- |
| Computer actions / boundaries | **8/10** | Native and CDP observe, click, move, drag, type, keypress, 2-axis scroll, wait; window/app/URL actions, shell, cancellation and serialized desktop mutations; protocol and modifier/drag regression tests. | Platform-specific controls and foreground behavior still differ; no arbitrary persistent browser-code tool in the model loop. |
| Visual grounding / accessibility | **7/10** | Accessibility Set-of-Marks, grid fallback, native pixel frames, coordinate frames (`marks`, `pixels`, `norm1000`, `smart-resize`), optional specialist grounder. | No real Holo3.1 coordinate calibration; some professional canvas and scaled-screen cases remain unverified. |
| Open-model interoperability | **4/10** | GGUF vision catalog, auto-pull, hash-pinned runtime, Gemini/vLLM/local backends, a shared OpenAI-shaped action path behind a flag. | Catalog entries do **not** imply correct model-native chat templates or action parsers; Fara1.5 is not on the curated shelf. |
| Safety and supervision | **8/10** | Human approvals, kill switch, injected-instruction checks, site rules, sensitive-field latch, cancellation and safety-check acknowledgement. | Some local rules and the screenshot monitor are opt-in; production false-positive/recall data is absent. |
| Outcome checking / recovery | **6/10** | Outcome ledger, probes, DONE gate, action effect labels, macro-history, task checks and recovery routing exist. | Outcome checking and history improvements are off by default pending paired live trials. Unprobed `done` can still be self-reported completion. |
| Cross-app / cross-OS reach | **5/10** | macOS AX, Windows UIA, Linux AT-SPI, native pointer input, Chromium CDP on Unix, shell tasks and MCP exposure. | Windows browser CDP is unavailable; Wayland control is limited; mixed-DPI/multimonitor/Qt and background-window releases need acceptance runs. |
| Speed / resource use | **6/10** | Replay harness ~65 ms p50/step excluding inference; image budgeting, caching, small JSON outputs, deterministic skips and local runtimes. | The measured Holo3.1 4B local browser calls were ~46 s p50 on 4 vCPU; screenshots and model tokens dominate real latency. No controlled GPT-6 cost/time comparison. |
| External evaluation evidence | **4/10** | Fixed local tasks, check commands, calibration scorer, A/B bench runner, scripted native API fixtures. | No valid two-runs-per-side OpenAI-contract comparison, OSWorld-Verified/OSWorld 2.0 score, comprehensive real-app platform matrix, or direct GPT-6 head-to-head. |

### What is implemented, and where

- **Actions:** `crates/kai-core/src/computer/`, `crates/kai-os/src/input/`,
  `crates/kai-os/src/web/`, `crates/kai-core/src/cua_protocol/`; the old
  decision tool also supports double and right click, file/OS shell work,
  app launching and URLs. Drivers advertise only supported capabilities.
- **Grounding:** `crates/kai-core/src/oai_contract/`,
  `crates/kai-core/src/engine/oai.rs`, `crates/kai-os/src/perceive.rs`.
- **Safety and verification:** `crates/kai-core/src/risk.rs`,
  `safety_checks.rs`, `safety_domains.rs`, `engine/injection_monitor.rs`,
  `engine/gate.rs`, `ledger.rs`, `engine/effect.rs`. Most advanced flags
  remain off by default.
- **External agents:** `kai mcp` provides `kai_observe`, `kai_act`,
  `kai_probe`, `kai_run_task` (`docs/MCP.md`). Its simple `kai_act` does not
  advertise the entire internal drag/move/coordinate computer-tool surface,
  and the no-human MCP channel deliberately refuses high-risk actions.
- **Models / evaluations:** `crates/kai-core/src/models/catalog.json`,
  `models/rank.rs`, `docs/LOCAL_MODELS.md`, `docs/MODEL_STORE.md`,
  `docs/CALIBRATION.md`, `docs/PHASE4_BEST_OF_K.md`, `bench/tasks.yaml`.

### Important model-format incompatibility

**Do not assume an OpenAI-shaped JSON wrapper turns an arbitrary open VLM into
a trained computer-use model.** As of today:

- Gemma 4 is productive with accessibility marks, but its native training is
  general VLM/instruction use; pixel-accurate GUI grounding and multi-step
  recovery remain a harness/model limitation.
- Holo3.1 4B/9B/35B and Holo4 are computer-use trained and in the catalog,
  but the catalog mostly describes weights, hardware and optional coordinate
  frame. Holo3.1 still requires empirical coordinate-frame calibration and
  native call/template compatibility tests.
- The curated `fara-7b-q4` is **Fara-7B**, not Microsoft's newer
  **Fara1.5**. Fara1.5 is open-weight (MIT, 4B/9B/27B). The official 9B
  model card requires a specific system prompt and emits XML `<tool_call>`
  blocks containing `computer_use` actions such as `left_click`,
  `left_click_drag`, `web_search`, `pause_and_memorize_fact`, and `terminate`.
  Kai's current JSON contract is **not** a native Fara1.5 adapter. The card
  recommends Microsoft Fara/MagenticLite's own harness. A community GGUF
  exists, but architecture support, projector pairing, template fidelity,
  coordinates and benchmark results must be validated before curation.
- UI-TARS-1.5 also uses its own action language; Kai's model card explicitly
  warns that this needs an adapter. UI-TARS-2 paper numbers cannot be assumed
  for a different publicly downloadable checkpoint.
- A model getting the right coordinate target on one screenshot is **not**
  proof that its next 20 actions will succeed. Keep vendor claims, our small
  local benchmark and third-party leaderboards separate.

Primary model references: [Fara1.5 model card](https://huggingface.co/microsoft/Fara1.5-9B),
[Fara1.5 paper](https://arxiv.org/abs/2606.20785),
[Holo3.1 model card](https://huggingface.co/Hcompany/Holo-3.1-4B),
[UI-TARS-2 paper](https://arxiv.org/abs/2509.02544).

### External-source cross-check (16 October 2026)

The original evidence above was code/repository inspection. The following
claims were separately cross-checked against the linked public sources on
16 October; **published vendor numbers are not Kai benchmark results**.

- [Microsoft's Fara repository](https://github.com/microsoft/fara) confirms
  Fara1.5 4B, 9B and 27B weights were released July 22, 2026, and the
  [9B card](https://huggingface.co/microsoft/Fara1.5-9B/blob/main/README.md)
  explicitly recommends a 1440×900 viewport, native XML `<tool_call>`
  actions and Microsoft's own Fara/MagenticLite harness. It currently
  targets *browser* workflows: don't assume its web scores transfer to
  arbitrary macOS applications.
- [Holo3.1's model card](https://huggingface.co/Hcompany/Holo-3.1-4B)
  describes native function calling for computer-use tasks. The important
  integration experiment is an actual OpenAI-compatible `tools` request to
  the exact local serving binary, followed by strict parser/executor tests;
  an OpenAI-style JSON answer fabricated by a general prompt does not prove
  native tool-calling compatibility.
- [H Company's Holo4 announcement](https://hcompany.ai/newsroom/holo4),
  dated September 28, reports **85.2% OSWorld** for 27B and **80.8%** for
  35B-A3B, **41.5%** and **12.3%** binary success on OSWorld 2.0 respectively.
  The Holo4 35B-A3B checkpoint is Apache-2.0; 27B has a restrictive research
  license according to [H's model index](https://hcompany.ai/research/models).
  Their self-reported scores use a particular harness, different test
  versions and sometimes single runs; they do not establish performance on
  Kai or superiority to GPT-6. The 35B-A3B 4-bit weights and working memory
  also make it a poor default for the inspected 16-GB Mac.
- [ByteDance's UI-TARS repository](https://github.com/bytedance/ui-tars)
  publishes a separate `ui_tars.action_parser` with a **1000-coordinate
  conversion path** and its own action syntax. Borrow the *tested parsing
  semantics* into a dependency-isolated Rust adapter; do not execute the
  library's generated PyAutoGUI source on the host as model output.
- [OpenAI's current guide](https://developers.openai.com/api/docs/guides/tools-computer-use)
  supports structured computer actions but recommends **isolated persistent
  code execution for GPT-6 Astra**. Its
  [integration recipes](https://developers.openai.com/api/docs/guides/tools-computer-use-integration)
  specify a persistent session, screenshots, deadline/resource limits,
  bounded access, user handoff, and independently checked outcomes. Kai
  has low-level computer actions and shell tools, but does not yet implement
  that unified model-facing, isolated, persistent browser-code facility.
- For long tasks, pin the entire
  [OSWorld 2.1 benchmark release](https://github.com/xlang-ai/OSWorld-V2):
  code, task files, assets, mocked websites and VM images from the same
  `osworld-v2.1` release. Its September 16 update supersedes the initial
  June 2026 setup. Cross-release or cross-harness model scores are not a
  controlled comparison.

**First install targets on 16 GB:** trial Fara1.5-4B in a disposable
1440×900 browser environment with its official prompt and action dialect;
trial Holo3.1-4B using its native tool-calling path, with calibrated
coordinates and measured vision serving compatibility. Treat Holo4
35B-A3B as a separate higher-memory/hosted candidate. This is a trial order,
not a recommendation to enable unvalidated checkpoints by default.

**Outcome verifier fixture quality:** the [Holo4 research report](https://hcompany.ai/newsroom/holo4)
describes checking that a task's verifier *fails on the untouched seed,
passes on a known-good state and rejects close wrong results*; the agent must
also complete it through the actual interface. Adopt those three checks for
Kai's browser/desktop fixtures to prevent false-green evaluation. Record
both binary task success and partial progress for long tasks, plus steps,
wall time, tokens, memory, retries, permission prompts and false completion.

## Can Kai's open models beat GPT?

**On a defined, reproducible subset of tasks, yes, potentially and sometimes
already in Kai's past small runs. Across arbitrary apps, not established.**
The repository's October 10 single-run, 14-task comparison reported Gemma 4
26B-A4B at **14/14**, GPT-5 mini **12/14**, GPT-4.1 mini **8/14** on
*Kai's particular marks-and-actions harness*. Small samples, dissimilar
providers, intermittent rate limits and model differences prevent a universal
ranking. This is neither a comparison with GPT-6 nor a like-for-like test of
OpenAI's native computer-use runtime (`README.md`).

Computer-use specialists have credible claims: H Company reported **78.85%**
OSWorld-Verified for Holo3 (vendor result; may not correspond to a local open
GGUF), while Microsoft reports **86.6% WebVoyager** and **63.4%
Online-Mind2Web** for Fara1.5-9B. Different task sets, harnesses and step
budgets cannot be compared as a single leaderboard. OSWorld 2.0's 108
long-horizon workflows expose weaknesses that Kai's 14-task win does not test.
The default Kai run is capped at **40 steps / 10 minutes**, so long-horizon
tasks may be impossible at default settings before accuracy even enters.

Sources: [Holo3 vendor report](https://huggingface.co/blog/Hcompany/holo3),
[Fara1.5 research](https://www.microsoft.com/en-us/research/articles/fara1-5-computer-use-agent/),
[OSWorld 2.0](https://arxiv.org/abs/2606.29537),
[OSWorld 2.0 released harness](https://github.com/Levelower/OSWorld-2.0),
[OpenAI computer-use guide](https://developers.openai.com/api/docs/guides/tools-computer-use).

## Prioritized changes

| Priority | Change | Why it matters | Concrete acceptance gate |
| --- | --- | --- | --- |
| **P0** | **Native adapters for 2–3 GUI specialists**, starting with Fara1.5 4B/9B and one Holo3.1 size. Preserve each native prompt/tool dialect; decode only into Kai's checked `ComputerAction` and ordinary approval gate. | A trained model can regress severely when required to speak a foreign format. | Unit golden vectors from model-card output; ≥95% parsed valid actions on held-out live prompts; no action escapes the executor/safety controls. |
| **P0** | **Calibration as an install/first-run benchmark**, including screenshot scaling, Retina/DPR and full-resolution checks. Never guess a model's coordinate frame. | A frame mismatch turns sound reasoning into wrong clicks. | Current `scripts/calibrate_coordinates.py` accepts ≥10 independent replies at **both** 1280×800 and 1440×900; ≥9/10 hits for each; report records exact model and image hashes. |
| **P0** | **Controlled, paired live benchmark** against Gemma marks, the specialist-native adapter and a GPT computer-use baseline. Split browser vs desktop. | A rating without verified task outcomes can reward faster mistakes. | ≥2 runs per side on identical resettable fixtures, comparable step budgets, approvals counted separately; report verified success, p50/p95 wall time, calls, tokens, RAM, false-done, safety stops and retries. |
| **P1** | **Persistent sandboxed code execution** (Playwright/JS for web first, desktop later), reusing the same browser session and exposing screenshot/display and concise logs. Restrict script/tool access to the permitted environment. | Current OpenAI guidance recommends code execution with loops and conditionals for capable models. Kai has shell and CDP actions, but not this model-facing isolated runtime. | Multi-step UI fixture with state persisted across two code calls, strict deadlines, observable action effects, user confirmation at consequential mutations, and no unreviewed host access. |
| **P1** | **Graduate outcome ledger/verified macro-history** using side-by-side trials, and add domain-specific artifact checkers for spreadsheets, slides and PDFs. | Correct clicks do not prove a file was saved or submitted correctly. | Lower false-done rate with at least equal verified success on real tasks; no increase in unapproved side effects; turn on by default only per evidence. |
| **P1** | **Robust recovery and visual precision**: specialized crop/re-ground, dialog/iframe/tab state, bounded replan after no-op, and selective verifier. | Professional UIs and long sequences require better diagnosis than retrying the same click. | Annotated task suite with hover, drag, scroll-inside-scroll, tab/dialog, modal, canvas, file pickers, failure recovery and no-op detection. |
| **P1** | **Actual browser acceptance in CI**, then Windows CDP and Wayland/Retina/multimonitor matrix. | A green Rust unit suite does not mean a GUI driver ran. | `KAI_REQUIRE_BROWSER=1` plus all browser suites; per-OS device/permission fixtures; screenshots + recorded outcome checks; no silent skips. |
| **P2** | **Fast-path routing and verified replay**, tuned per machine. Prefer a cheap local specialist for grounded steps, a larger model for planning/recovery, and code/procedures for repeat work. | Saves the expensive model inference that dominates Kai's overhead. | Same success, p50/p95 latency and total tokens decrease on paired real trials; fallback when confidence drops. |

Do not enable all experimental flags together without an A/B: otherwise
the source of improvement (or new failure) becomes unknowable. OpenAI's
integration guide supports the existing structured `computer` tool as well
as code execution; preserving both is worthwhile.

## Reproducible local status (this inspection)

- Machine via `kai doctor`: macOS Apple Silicon, **16 GB RAM**, Apple M5 GPU;
  llama.cpp runtime path exists, but **no local model is installed or
  serving**. Gemini credentials are configured; no direct OpenAI key is
  configured. No direct GPT-versus-open live trial can therefore be claimed.
- `cargo test -p kai-core --test cua_native --test safety_parity_e2e
  --test cua_native_monitor`: **46 passed** (30 + 10 + 6).
- `cargo test --workspace`: **exit 0**, with two explicitly ignored
  desktop/toolkit cases; some browser tests may quietly return early if
  Chromium is unavailable.
- `KAI_REQUIRE_BROWSER=1 cargo test -p kai-os --test browser_computer
  -- --nocapture`: **failed 2/2 at browser discovery** with `no
  Chromium-family browser was found`; this is a missing local test
  prerequisite, not evidence that browser input failed. CI runners have
  Chrome and set `KAI_REQUIRE_BROWSER=1`.
- **Small verified CI change in this audit:** `.github/workflows/ci.yml`
  now includes `--test browser_computer` in its browser-required job.
  The workflow is intentionally **manual-only** (`workflow_dispatch`) at
  the owner's earlier request; nothing has been auto-run on GitHub.
- The repository already contained many uncommitted changes before this
  inspection. This assessment did not reset or overwrite them.

To validate on a host with Chromium installed:

```sh
cargo fmt --all --check
cargo test --workspace
KAI_REQUIRE_BROWSER=1 cargo test -p kai-os \
  --test browser_lane --test browser_edges --test browser_computer
```

Then run the existing two-size calibration and paired controlled benchmark
in `docs/CALIBRATION.md`, first with the current Gemma contract and then
with a model-native specialist adapter. Promoting a model to *Best* requires
local reproducible success and hardware-fit evidence, not a vendor score.

## What counts as ready

Kai can claim **"open-model computer-use ready"** for a named model, OS,
driver and application class when: its native action template parses; its
grounding frame is calibrated; actions and cancellations pass real driver
tests; user approvals are enforced; outcome checkers independently verify
the final state; and at least two paired live runs show a useful success /
latency trade-off. It cannot responsibly claim **"any open model can control
any app as well as GPT"** on today's evidence.
