# Roadmap: making Gemma use the computer better and faster than a frontier model

Written 2026-10-10. This builds on [RESEARCH.md](RESEARCH.md) (2026-10-09),
which covered speed inside the harness: frames, batching, tree-only steps,
accessibility-first actions, vLLM flags, Jev and the Gateway. This document
does not repeat that. It covers what comes next: how a small open model can
overtake frontier models **inside one product**, how to keep it improving, and
a phased plan with measurable targets.

Labels follow RESEARCH.md:

- **[Kai]**: measured by Kai, in this repo.
- **[Paper]**: an academic or lab paper reporting its own method's results.
- **[Indep.]**: measured by someone other than the people selling or proposing
  the thing.
- **[Vendor]**: claimed by the company that sells the model.
- **[Aggregator]**: a leaderboard that collects self-reported numbers.
- **[Est.]**: my own arithmetic or estimate. These are planning numbers, not
  measurements.

Every target in the phase plan is a **target**, not a prediction.

---

## 0. The bet in one paragraph

Frontier models are trained to be good at every computer. Kai only has to be
best on **its own tasks, in its own harness, on Linux and Windows desktops**.
The published record says this is exactly where small open models win. They
get there by training on their own verified trajectories, with rewards from
deterministic checks (§2.1). Kai already has the three expensive parts:

1. **A harness that records every step.** Frame, element list, decision,
   timings, outcome.
2. **Tasks with automatic checks.** 30 today, in `bench/tasks.yaml`.
3. **A model with open weights** (Gemma 4, Apache 2.0) that can be fine-tuned
   on one H100.

The plan:

1. Measure honestly.
2. Turn every run into data.
3. Fine-tune with supervised learning first, then with RL on verifiable
   rewards.
4. Distil the result into fewer, shorter steps.
5. Repeat on a fixed weekly cadence, with held-out gates.

"Better than GPT-6 Astra" is defined as a statistical test on held-out Kai
tasks (§3.2), not as a leaderboard claim.

---

## 1. Where Kai stands, and what the target is

### 1.1 Kai today

| What | Value | Label |
| --- | --- | --- |
| Gemma 4 26B-A4B via Gemini API (free tier), 30 bench tasks | **23/30 passed (77%)** | [Kai] 2026-10-10 |
| 95% interval on that pass rate (Wilson, n = 30) | **~59%–88%** | [Est.] |
| Step p50 / p95 | **3.1 s / 8.1 s** | [Kai] |
| Model call p50 / p95 | 2.85 s / 6.47 s | [Kai] |
| Observe p50 / p95; act p50 / p95 | 83 / 305 ms; 1 / 366 ms | [Kai] |
| Steps per task / model calls per task (mean) | **6.8 / 10.1** | [Kai] |
| Reference replies (hand-written `mock` replies) | ~3.0 steps per task | [Kai] (REPORT.md) |
| Tokens per task (mean, 287 of 304 calls reported) | ~19.0K in / ~0.9K out | [Kai] / [Est.] |
| Mean seconds per task | 105 s (includes free-tier 429 waits) | [Kai] |
| Failed | local-price-lookup, mousepad-line-numbers (one step took 183 s), spreadsheet-budget, csv-total, email-draft, dark-mode, pc-checkup | [Kai] |
| GPT-5 mini, GPT-4.1 mini in the same harness | runs in progress | [Kai] |
| Gemma 4 31B | available, but flaky on the free tier | [Kai] |

Results file: `bench/results/2026-10-10-gemini-gemma-4-26b-a4b-it.md`.

Three readings matter for the plan:

- **30 tasks cannot separate two good agents.** The 95% interval is about
  ±15 points wide. Beating anyone needs ≥ 100 held-out tasks and repeats (§2.6).
- **Kai takes ~2.3× the steps of the reference replies**, and 1.5 model calls
  per step. The ratio matches OSWorld-Human's finding that top agents take
  2.7–4.3× the human-minimal steps
  ([arXiv 2506.16042](https://arxiv.org/abs/2506.16042)) [Indep.]. Steps per
  task is the biggest speed lever left. Fine-tuning can attack it directly
  (§2.5).
- **Free-tier wall time is not a speed number.** Phase 0 moves the speed
  baseline to the Droplet.

### 1.2 GPT-6 Astra: what is public (as of 2026-10-10)

| Fact | Value | Label / source |
| --- | --- | --- |
| Release | System card published **2026-09-03**; limited preview 09-03, paid users 09-04 | Primary: [OpenAI system card](https://deploymentsafety.openai.com/gpt-6-astra); [Wikipedia](https://en.wikipedia.org/wiki/GPT-6_Astra) |
| On Vercel AI Gateway | `openai/gpt-6-astra`, announced **2026-09-04**, "designed for long-running agentic work" including navigating software. No price or benchmarks on the changelog. | [Vercel changelog](https://vercel.com/changelog/gpt-6-astra-now-available-on-vercel-ai-gateway) |
| OSWorld 2.0 | **72.6%** (vs GPT-5.6 Sol 65.7%, Claude Opus 5 70.2%) | [Vendor], from OpenAI's launch tables via [DataCamp](https://www.datacamp.com/blog/gpt-6-astra) and [BenchLM](https://benchlm.ai/models/gpt-6-astra) [Aggregator] |
| Time per OSWorld 2.0 task | **~40 min** vs ~75 min for GPT-5.6 Sol ("latency simulations") | [Vendor], via DataCamp |
| ScreenSpot-Pro (grounding) | **92.7%** (GPT-5.6 Sol 76.9%) | [Vendor] |
| OSWorld-Verified (the 369-task set) | **Not reported anywhere I could find** | — |
| Other agentic | Agents' Last Exam 59.3% [Vendor]; AutomationBench 41.4% [Vendor], 68.5% on Artificial Analysis' run [Indep.] | BenchLM |
| Price | **$10 / M input, $50 / M output**; "fast mode" up to 2.5× speed at 2× price | Secondary (DataCamp; OpenRouter listing per search). The openai.com launch page returned 403 to my fetcher. |
| Context | 1.05M tokens | [Aggregator], unverified |
| Prompt-injection attack success (Gray Swan, 15 attempts) | 8.5% vs 27.0% for GPT-5.6 Sol | Primary: system card |
| Successors | GPT-6 Sol and Luna (2026-09-22, cheaper); GPT-6.1 is listed as a successor | Wikipedia |

Caveats:

- **Not comparable with earlier OSWorld 2.0 numbers.** RESEARCH.md cites
  20.6% as the best OSWorld 2.0 binary-completion score (Claude Opus 4.8). The
  72.6% is described as the "offline set", so the metric or subset almost
  certainly differs. Do not compare the two.
- **Every Astra computer-use number is vendor-reported.** I found no
  independent computer-use evaluation of it yet.
- **Kai cannot call Astra on the free Gateway key.** That key refuses gpt-5.4
  and newer (403, RESEARCH.md §0), so the comparison needs paid credits.
- **Cost per Kai task.** At Kai's current usage (~19K in, ~0.9K out per task),
  one task costs about **$0.23 at list price** [Est.], before any reasoning
  tokens Astra adds. A 100-task held-out set × 3 repeats is roughly
  **$70–150** [Est.].
- **For contrast,** Gemma 26B-A4B on the Gateway lists $0.13 / $0.40 per M
  (RESEARCH.md §4), about $0.003 per task [Est.].

**What this means.** Astra is a strong, slow and expensive generalist: ~40
minutes per long task, $10/$50 per M tokens. On Kai's short everyday tasks, a
fine-tuned Gemma that answers in under a second per step can win on **speed
and cost** almost by default once it is on the Droplet. Winning on **success
rate** is the hard part, and it is what the training phases are for.

---

## 2. What the research says

### 2.1 Small or open models beating frontier models: the actual record

| Work | Small/open model | Beat whom, on what | By how much | Compute / data | Label |
| --- | --- | --- | --- | --- | --- |
| **DigiRL** ([2406.11896](https://arxiv.org/abs/2406.11896), 2024-06) | 1.3B VLM | GPT-4V AppAgent, Android-in-the-Wild | **67.2% vs 8.3%**; SFT alone was 17.7% | Offline then online RL; VLM evaluator; parallel Android emulators | [Paper] |
| **WebRL** ([2411.02337](https://arxiv.org/abs/2411.02337), ICLR 2025) | Llama-3.1-8B, GLM-4-9B | GPT-4-Turbo 17.6%, GPT-4o 13.9%, WebArena-Lite | **4.8% → 42.4%** (8B) | Curriculum built from failures; outcome reward model; online RL | [Paper] |
| **ARPO** ([2505.16282](https://arxiv.org/html/2505.16282), 2025-05) | UI-TARS-1.5-7B | its own base, OSWorld | OSWorld 23.5 → 29.9; **in-domain 32 tasks 43.8 → 81.3%** (plain GRPO 68.8%) | GRPO + replay buffer of successes; 256 parallel VMs, 8 rollouts per task, 15 epochs | [Paper] |
| **SEAgent** ([2508.04700](https://arxiv.org/abs/2508.04700), 2025-08) | UI-TARS-based | itself on 5 unfamiliar apps (OSWorld) | **11.3% → 34.5%** | Curriculum generator, step-level world-state judge, GRPO on successes plus imitation against failures | [Paper] |
| **GTA1** ([2507.05791](https://arxiv.org/html/2507.05791v5), 2025-07) | 7B grounder + o3 planner | OpenAI CUA (o3) on OSWorld | **45.2% in 100 steps vs 42.9% in 200 steps** | RL on click correctness; K = 8 sampled actions plus a judge. Not "small alone": the planner is o3. | [Paper] |
| **ScaleCUA** ([2607.11185](https://arxiv.org/html/2607.11185), 2026-07) | Qwen3.5-9B, Qwen3-VL-8B | Claude Sonnet 4.5 (62.9%), OSWorld | **41.8% → 68.7%** (9B); still below GPT-5.4 (75.0%) | 24K generated tasks → ~3K RL tasks, each with a deterministic Python judge; 32 tasks × 8 rollouts per iteration; 16–64 GPUs in ablations | [Paper] |
| **Holo3-35B-A3B** ([model card](https://huggingface.co/Hcompany/Holo3-35B-A3B), [blog](https://hcompany.ai/newsroom/holo3), 2026-03-31) | 35B MoE, **3B active**, Apache 2.0 | GPT-5.4 (75.0%), OSWorld-Verified | **77.8%** | "Synthetic environment factory": coding agents build websites with verification scripts; flywheel; RL | [Vendor] |
| **Qwen3.8-27B** ([model card](https://huggingface.co/Qwen/Qwen3.8-27B), 2026) | 27.8B dense, Apache 2.0 | Opus 4.6 (72.7%), OSWorld-Verified | **84.3%** | not described | [Vendor] |
| **DeskForge-Gemma4-E4B** ([model card](https://huggingface.co/docling-project/DeskForge-Gemma4-E4B), 2026) | **Gemma 4 E4B**, LoRA r = 8 on the language model only | its own base, grounding | ScreenSpot-Pro **1.8 → 22.0**; OSWorld-G **10.6 → 44.2**; WebArena-Infinity 5 → 40 of 119 tasks | 200K grounding examples, 1 epoch, 8×H100 | [Paper/model card] |
| **UI-R1** ([2503.21620](https://arxiv.org/abs/2503.21620)) / **GUI-R1** ([2504.10458](https://arxiv.org/abs/2504.10458)) (2025) | Qwen2.5-VL-3B | OS-Atlas-7B (trained on 76K / 13M samples) | Competitive or better with **136 / 3K** RL samples | Rule-based action rewards, GRPO | [Paper] |
| **Fara-7B** ([2511.19663](https://arxiv.org/abs/2511.19663), 2025-11) | 7B, screenshots only | "competitive with much larger frontier models" on web tasks | — | Synthetic tasks, several verifiers, **~$1 per verified trajectory** | [Paper] |

The counter-evidence is just as important. OpenComputer
([2605.19769](https://arxiv.org/html/2605.19769), 2026-05) re-tested models on
33 new apps (1,000 tasks) [Indep.]:

| Model | Self-reported OSWorld(-Verified) | OpenComputer success |
| --- | --- | --- |
| GPT-5.4 | 75.0% | **68.3%** |
| Claude Sonnet 4.6 | 72.5% | 64.4% |
| Qwen-3.5-27B | 56.2% | 32.3% |
| EvoCUA-8B | 46.1% | **10.9%** |
| GUI-Owl-1.5-8B | 52.3% | **5.7%** |

**Lessons for Kai:**

1. **In-distribution training beats frontier models; out-of-distribution
   generalisation mostly does not.** ARPO's in-domain jump (+37 points) is
   ~6× its general OSWorld gain (+6.4). Small models that top OSWorld collapse
   on new apps. Kai's advantage is that *its users' tasks are its distribution*.
   Its risk is mistaking bench gains for general gains. Hold out whole task
   families, and keep an external yardstick (§2.6).
2. **Reward quality decides the size of the gain.** ZeroGUI's VLM-estimated
   rewards had **61.5% precision / 51.1% recall** and gave only
   **+2.5 points** on OSWorld for UI-TARS-7B (17.7 → 20.2, 4 runs)
   ([2505.23762](https://arxiv.org/html/2505.23762)) [Paper]. ScaleCUA's
   deterministic judges agreed with experts 82.5% of the time and gave
   **+27 points**. This is a cross-paper comparison, so it is confounded, but
   the direction is consistent. Kai's shell `check` commands are deterministic
   judges. **That is Kai's single biggest training asset.**
3. **An MoE with ~3B active parameters can be a top computer-use model.**
   Holo3-35B-A3B is the existence proof, though vendor-reported. Gemma 4
   26B-A4B (3.8B active) has the same shape, so its active size does not rule
   it out.

### 2.2 Data flywheels from the product's own runs

| Pattern | What it does | Where from | For Kai |
| --- | --- | --- | --- |
| **Rejection-sampled self-play** | Run the current model k times per task; keep runs that pass the check | DigiRL, WebRL, ARPO task filtering ("solved ≥ 1 of 16") | `kai bench` with temperature > 0 and `--repeat k`; the check decides |
| **Task variants** | Same skill, new parameters: file names, numbers, apps, themes, window layouts | ScaleCUA VeriGen; DeskForge held-out "theme / app / resolution" splits | **Being built**: the variants generator. Each variant must keep a working `check`. |
| **Synthetic task factory** | An LLM proposes a task plus setup plus check; a dry run validates the check; then it is filtered | ScaleCUA (proposer/judger/checker; **94.5%** of judges executable); Holo3 environment factory | Phase 1. Generate `tasks.yaml` entries; reject any whose check passes *before* the task runs. |
| **Reverse synthesis** | Explore first, then write the instruction that the trajectory fulfils | OS-Genesis ([2412.19723](https://arxiv.org/abs/2412.19723), ACL 2025) | Cheap source of grounding/navigation data in apps without checks; label them "unverified" |
| **Tutorial replay** | Turn how-to text into tasks, run them, judge them | AgentTrek ([2412.09605](https://arxiv.org/abs/2412.09605)): **$0.55 per trajectory** | Linux/Windows how-tos for the apps Kai supports |
| **Exploration at scale** | Explore websites, derive intents | Explorer ([2502.11357](https://arxiv.org/abs/2502.11357)): 94K trajectories at **~$0.28 each** | Web part of Kai only |
| **Human demonstrations** | Annotation tool records people | OpenCUA / AgentNet ([2508.09123](https://arxiv.org/abs/2508.09123)): 3 OSes, 200+ apps; OpenCUA-72B **45.0%** OSWorld-Verified | Opt-in "show Kai how" recording later; expensive |
| **Curriculum from failures** | New tasks seeded from what failed; sample where success is 0 < p < 1 | WebRL; SEAgent; ScaleCUA "frontier sampling" | Failure taxonomy → targeted variants |
| **Failure taxonomy** | Every failed run gets a cause | — | **Being built.** Proposed causes: wrong target, wrong app knowledge, premature or missing `done`, loop / no progress, format or repair, harness or infra (hang, timeout), environment flake, unsafe action blocked. |

**Privacy rule for the flywheel.** User traces stay local by default
(HARNESS.md). Training data comes from bench, variant and synthetic runs on
Kai's own machines. User traces enter only through an explicit opt-in export
(`kai data export`, **being built**) with redaction of typed text, secure
fields and window titles. Free-tier Gemini API traffic may be used by Google
to improve its products, so real user screens should not go through it. That
is a privacy point as well as a data point.

### 2.3 Training recipes for open VLM GUI agents

In the order Kai should use them:

1. **SFT on successful trajectories** (behaviour cloning on Kai's exact JSON).
   - Each training example is one step: system prompt, frame (or tree-only
     text), element list, last-3 history in, the decision JSON out.
   - Loss is on the reply only.
   - This teaches the format (no repair retries), Kai's action vocabulary
     (`shell`, `then`, `need_screen`) and app knowledge.
   - Every RL paper above starts from an SFT'd policy.
2. **Rejection sampling / STaR / expert iteration.**
   - Sample k runs per task, keep the passing ones, prefer the shortest
     passing run, retrain, repeat.
   - Agent-RLVR adds **guidance on retry** (a plan or the error fed back),
     then trains on the guided successes. That took Qwen-2.5-72B from **9.4%
     to 22.4%** on SWE-bench Verified
     ([2506.11425](https://arxiv.org/abs/2506.11425)) [Paper].
   - Kai can do the same: when a run fails, retry with the failure cause in
     the prompt, keep the success, and **drop the guidance from the training
     input**.
3. **Distillation from a stronger teacher.** Licensing decides what is
   allowed:
   - **OpenAI's terms forbid using output "to develop models that compete
     with OpenAI"** ([terms](https://openai.com/policies/terms-of-use/),
     effective 2026-01-01).
   - The **Gemini API terms forbid using the Services to develop competing
     models** ([terms](https://ai.google.dev/gemini-api/terms)).
   - So GPT-5 mini / GPT-4.1 mini / GPT-6 Astra traces from Kai's comparison
     runs are **evaluation-only**. Never put them in a training set.
   - The safest source of training rollouts is **Gemma itself on Kai's own
     vLLM Droplet**. The terms may also cover Gemma served by Google's API;
     that is unclear, so prefer the Droplet.
   - **Apache-2.0 open models are clean teachers.** Holo3-35B-A3B and
     Qwen3.8-27B are both Apache 2.0, and Apache 2.0 places no restriction on
     using outputs. Run them on the Droplet as teachers for tasks Gemma cannot
     solve yet.
   - Not legal advice. Have the owner confirm before relying on this.
4. **RL with verifiable rewards (online, in real desktops).**
   - **GRPO** (group-relative advantages, no value network) is the common
     choice: ARPO, UI-R1, GUI-R1, SEAgent, ZeroGUI, ScaleCUA. ScaleCUA uses
     **DAPO-style asymmetric clipping plus a KL penalty** to the reference
     policy.
   - **Reward:** the task's `check` (0/1), plus a small step penalty, plus a
     format term. Never use a model judge where a check exists.
   - **Stabilisers:**
     - A replay buffer of past successes, so a group is never all zeros
       (ARPO; plain GRPO 68.8% vs ARPO 81.3% in-domain).
     - Task selection at the frontier (0 < success < 1).
     - A sliding window of recent frames (ScaleCUA: **2.83×** faster
       training).
   - **UI-TARS-2** ([2509.02544](https://arxiv.org/abs/2509.02544)) reports
     OSWorld 47.5 and WindowsAgentArena 50.6 with a data flywheel plus
     multi-turn RL in a hybrid GUI + terminal sandbox. Kai's `shell` action
     makes it the same kind of hybrid.
5. **Process rewards and step verifiers.**
   - Use them where no final check exists, or for credit assignment on long
     tasks.
   - Examples: GUI-PRA ([2509.23263](https://arxiv.org/abs/2509.23263)),
     milestone rewards (ADMIRE, [2602.11524](https://arxiv.org/abs/2602.11524),
     >10-point AndroidWorld gain [Paper]), and a verifier agent that runs
     read-only probes (VAGEN, [2602.00575](https://arxiv.org/abs/2602.00575)).
   - Every one of these papers warns about **reward hacking**.
   - Kai's step-level signal already exists: `expect` versus the next tree,
     plus the dHash no-progress detector. Use it as a shaping term only, never
     as the main reward.
6. **Grounding-specific training.** It matters less for Kai, because Kai
   answers with a mark number, not pixels. It matters for the grid fallback
   (games, canvases, Electron).
   - GTA1-7B: ScreenSpot-Pro **50.1%**, OSWorld-G 67.7% [Paper].
   - DeskForge shows Gemma 4 E4B learns grounding from 200K examples.
   - Jedi, OS-Atlas and UGround are the public data sources (RESEARCH.md §3.6).
   - **Defer** until the failure taxonomy shows grid misses matter.
7. **Done detection is its own skill.** "Teach it to stop, not just to click"
   ([2607.17136](https://arxiv.org/abs/2607.17136), 2026-07) [Paper]:
   - A constrained done-detection repair reached **0.97 ± 0.06**.
   - Free-form field filling reached only 0.14 ± 0.04.
   - Kai's failures include premature/missing `done` candidates (pc-checkup is
     judged by outcome). Give `done` its own training signal: pass/fail of
     the check exactly when the model says `done`.

### 2.4 Fine-tuning Gemma 4 specifically

**Licence.**

- **Gemma 4 is Apache 2.0**, not the older Gemma Terms of Use
  ([Google Open Source Blog](https://opensource.googleblog.com/2026/03/gemma-4-expanding-the-gemmaverse-with-apache-20.html)).
- Derivatives, fine-tunes, hosted use and redistribution are allowed with the
  usual notice and attribution.
- The README's credits line still says "Gemma Terms of Use"; it should say
  Apache 2.0 (not changed here).
- Training Gemma on outputs of *other* models is governed by those models'
  terms, not Gemma's (§2.3.3).

**Memory on one H100 80 GB.** Unsloth's guide, page undated, accessed
2026-10-10 ([guide](https://unsloth.ai/docs/models/gemma-4/train)) [Vendor]:

| Model | What Unsloth says | Fits one H100? | Notes |
| --- | --- | --- | --- |
| **26B-A4B (MoE)** | LoRA needs **> 40 GB**; prefer bf16 LoRA; MoE QLoRA "not recommended"; full fine-tuning not advised | **Yes**, bf16 LoRA (weights ~50 GB [Est.]) with gradient checkpointing, batch 1, ≤ 4K tokens | Vision notebook exists (`Gemma4_(26B_A4B)-Vision`); `finetune_vision_layers` optional |
| **31B (dense)** | QLoRA in **22 GB** | Yes (QLoRA; bf16 LoRA is tight at ~62 GB of weights [Est.]) | Slower per token than the MoE at inference |
| **E4B** | LoRA 10–17 GB (the page contradicts itself) | Easily | 150M vision encoder, much weaker on vision (RESEARCH.md §1.1). A candidate only for text/tree-only steps. |

**Unsloth caveats from the same page:**

- Put the image **before** the text.
- Use `UnslothVisionDataCollator`.
- RL notebooks load with `fast_inference=False`, because their vLLM path did
  not support Gemma 4 when the page was written.
- A mismatched chat template at export is the most common cause of worse
  quality after training.

**Kai's sequences are short.** A ~264-token image, a ~650–800-token element
list, plus the prompt is about 2–4K tokens per step. That keeps memory well
inside these limits.

**Serving fine-tunes with vLLM.** This matters for the RL loop and the
Droplet:

- **LoRA for Gemma 4 landed in vLLM in April 2026** (issue
  [#39246](https://github.com/vllm-project/vllm/issues/39246) closed by PR
  #39291).
- **LoRA on the MoE experts is still broken.** PR
  [#50252](https://github.com/vllm-project/vllm/pull/50252) (open, needs a
  rebase as of 2026-09-18) shows stacked expert LoRA tensors for 26B-A4B
  **load silently and apply nothing**. Before the fix, 0 of 10 prompts
  differed from the base model.
- Vision-tower LoRA is tracked separately (#42662).

**Decision for Kai:**

- **Train LoRA on attention (and non-expert) modules.** For anything that
  touches experts, **merge the adapter into the weights** before serving.
- Add a smoke test: the adapter must change outputs on a fixed probe set, or
  the run fails. That catches silent no-op adapters.
- Multi-LoRA hot-swapping (one base, several adapters) becomes an option once
  #50252 lands.

### 2.5 Speed over time

Speed comes from four places. RESEARCH.md covers 1 and 2; fine-tuning opens 3
and 4.

1. **Serving, which is lossless:**
   - Prefix caching and async scheduling are already set.
   - **MTP speculative decoding for Gemma 4 merged in vLLM on 2026-05-06**
     (PR [#41745](https://github.com/vllm-project/vllm/pull/41745)). The
     recipe still calls it nightly-only.
   - vLLM's AMD blog reports **2.83×** for 26B-A4B in its tested
     configurations ([2026-08-23](https://vllm.ai/blog/2026-08-23-speculative-decoding-amd-gpus))
     [Vendor-adjacent].
   - With images, the drafter gets **text-only inputs** (issue
     [#58580](https://github.com/vllm-project/vllm/issues/58580)), so
     acceptance on screen steps may be lower.
   - Measure it on the Droplet with Kai's own traffic before turning it on.
2. **Harness, already built:** batching (`then`), tree-only steps, frame
   sizing, a11y actions, plan cache, skills.
3. **Fewer steps, learned:**
   - **Trajectory compression.** Rewrite each successful trajectory into its
     batched form: consecutive certain actions with no visual dependency get
     merged into one decision with `then`. Then train on that. OSWorld-Human
     shows grouped actions cut Calc steps **13.6 → 5.9**.
   - **Prefer the shortest passing sample** in rejection sampling.
   - **Step penalty in the RL reward.**
   - **Skill mining.** Recurring deterministic prefixes across successful
     runs become skills or plan-cache entries automatically (AWM: +24.6% /
     +51.1% relative, RESEARCH.md §3.8).
4. **Fewer tokens per step, learned:**
   - **Shorter thoughts.** Train on thoughts ≤ 12 words, or empty when the
     action is obvious. Today: ≤ 25 words, ~86 completion tokens per call
     [Est. from 26,276 / 304].
   - **Context distillation.** Once behaviour is trained in, the long rules in
     the system prompt can shrink. That means less prefill every step; the
     prefix cache hides much of it, but not on a cache miss.
   - **Trained `need_screen` routing.** More steps can be tree-only once the
     model reliably asks for the screen when it needs it.
   - **Optional E4B route** for tree-only steps, distilled from the fine-tuned
     26B. It needs proof on held-out tasks first.

Verifier-gated screenshots (Jev / small LLM) stay off the critical path. The
measured LLM judge p50 was 803 ms (DECISIONS.md), slower than what it saves.

### 2.6 Evaluation discipline

- **Variance is large and often ignored.** In "Teach it to stop":
  - The *data draw* explained up to **48%** of outcome variance on the
    hardest cell.
  - A single run had about a **30%** chance of hitting a bimodal failure
    mode.
  - The authors caught two of their own over-claims by replicating across
    seeds.
  - ZeroGUI reports mean ± std over 4 runs; most papers report one run.
  - **Kai rule:** n ≥ 3 repeats for every reported number, with
    paired-bootstrap or McNemar tests for A vs B on the same tasks.
- **Fixed held-out tasks.**
  - `bench/tasks.yaml` (30 tasks) becomes the **dev set**. It is looked at
    every day, so it will be overfit.
  - A new **held-out set of ≥ 100 tasks**, written by a person, in families
    disjoint from training (whole families held out, not just variants), is
    run only at phase gates.
  - Refresh it quarterly with new tasks; retire leaked ones.
- **External yardsticks.**
  - **OSWorld-Verified** (369 Ubuntu tasks; human 72.36%) and **Windows Agent
    Arena** (154 tasks; human 74.5%), each run through Kai's harness.
  - Run a fixed subset (e.g. 50 OSWorld tasks in apps Kai never trained on)
    at each gate.
  - Their job is to catch the OpenComputer-style collapse: dev up, external
    flat or down means overfitting.
  - Never train on them.
- **Contamination.** Synthetic task generators must not see held-out prompts.
  Keep held-out tasks in a separate file outside the generator's context, and
  hash-check training prompts against them.
- **Fair comparison with a frontier model.** Report **both**:
  - **(a) Same harness.** Astra via the Gateway with Kai's Set-of-Marks
    prompt, the same steps cap, machine, tasks and checks.
  - **(b) Native loop.** OpenAI's own computer tool or code mode, which
    needs a direct OpenAI key, as `CuaRunner` already supports for
    `computer-use-preview`. Whether `gpt-6-astra` accepts the computer tool
    has to be checked.
  - Report success, steps, wall time per task, s/step p50/p95 and $ per
    task, with Gateway overhead (~0.16–0.2 s TTFT, RESEARCH.md §4) noted.
  - A fine-tuned Gemma trained on Kai's prompt has a home advantage in (a).
    (b) is the stronger claim.

---

## 3. Strategy and definition of "better than GPT-6 Astra"

### 3.1 Why this can work

- **Narrow distribution.** Everyday Linux/Windows tasks in a known harness,
  with a known action schema.
- **Verifiable rewards.** Every task has a deterministic check.
- **Open weights, cheap inference.** Many rollouts per task are affordable on
  one Droplet.
- **The harness does part of the job.** Set-of-Marks plus a11y actions remove
  pixel grounding, which is where frontier generalists spend their scale
  (Astra's 92.7% ScreenSpot-Pro).

### 3.2 Definition (proposed; owner to confirm)

Gemma-Kai **beats GPT-6 Astra** when, on the held-out set (≥ 100 tasks, n = 3
repeats each, same machine image):

1. **Success.** Gemma-Kai's success rate is ≥ Astra's in the same harness
   **and** ≥ Astra native. A paired test gives p < 0.05, or at minimum the 95%
   interval of the difference excludes −3 points (non-inferiority) for the
   first public claim.
2. **Speed.** Median wall time per task is ≤ 50% of Astra's in the same
   harness, and step p95 on the Droplet is ≤ 2.5 s.
3. **Generalisation.** On the external OSWorld-Verified subset, Gemma-Kai is
   no worse than its own Phase-0 baseline (no overfitting collapse).

Until all three hold, the README says "not yet" with the numbers.

---

## 4. The phased roadmap

Infrastructure prices:

- **H100 Droplet ~$3.39/hour**, billed while it exists
  (`deploy/digitalocean/README.md`).
- CPU Droplets for headless desktops are much cheaper (price not checked here).

GPU hours below are **[Est.]**. Each phase starts with a small pilot that
measures throughput, then re-plans.

### Phase 0: measure honestly (weeks 1–2)

**Goal:** trustworthy baselines for speed and success; held-out set in place.

- Move the speed baseline to the **Droplet** (vLLM, 26B-A4B; 31B as careful).
  Record TTFT vs decode per step (`client/stream.rs`).
- Write the **held-out set**: ≥ 100 tasks, ≥ 15 families, Linux first, with
  ~20 Windows tasks. Every task has a setup and a check, and the check is
  proven to fail before the task runs.
- Baselines, n = 3 each, on dev and held-out:
  - Gemma 26B-A4B and 31B (Droplet)
  - GPT-5 mini and GPT-4.1 mini (in progress)
  - **GPT-6 Astra** in Kai's harness (paid Gateway credits)
  - Astra native (direct OpenAI key), if available
  - Optional open references: Holo3-35B-A3B and Qwen3.8-27B on the Droplet
- Add the external yardstick harness: 50-task OSWorld-Verified subset; WAA
  subset when a Windows VM is available.
- `kai bench`: `--repeat n`, a seed, per-run JSON, paired comparison in
  `kai bench report`.

**Targets:**

- Droplet step **p50 ≤ 1.5 s, p95 ≤ 3.5 s** for Gemma 26B-A4B (to be
  confirmed; never measured).
- Model calls per step ≤ 1.3 (today 1.5).
- Fix the hang behind the 183 s step in `mousepad-line-numbers`.

**Infrastructure:**

- ~30–50 H100 hours ($100–170) for serving and baselines.
- ~$100–200 of API for the Astra and GPT runs [Est.].

**Exit:** a REPORT.md table with intervals for every backend; the held-out
file frozen and hashed.

### Phase 1: the data flywheel (weeks 2–5, overlaps Phase 0)

**Goal:** turn runs into a growing, clean, labelled dataset.

**Already being built this session:**

- Trajectory recording and `kai data export`. Format described in
  `docs/DATASET.md`, also being written.
- The failure taxonomy.
- The task-variants generator.

**Add:**

1. **Rollout farm.**
   - 8–16 headless Linux desktops (the existing Xvfb/Xvnc container scripts)
     on CPU Droplets.
   - All point at the one vLLM Droplet.
   - Each worker runs `kai bench` on a task shard with temperature 0.7 and
     k = 4–8 samples per task.
2. **Synthetic task factory.**
   - An LLM proposes (setup, prompt, check) for Kai's apps (files, Mousepad,
     LibreOffice, Firefox on the local fixture server, settings).
   - Keep a task only if its check fails before the run and the setup is
     idempotent.
   - The generator may be any model: tasks are not model outputs Kai trains
     *on* as behaviour. Check the generator's terms anyway.
3. **Labelling.**
   - Pass/fail from the check.
   - A failure cause from the taxonomy (rules first, then a small judge,
     human spot checks on 5%).
   - The shortest passing run per task is marked "preferred".
4. **Dataset hygiene.**
   - Dedupe near-identical trajectories.
   - Strip tasks that overlap held-out families (prompt-hash and family-tag
     checks).
   - Redact secure fields.
   - **Provenance tag per trajectory:** model, backend, licence class
     (`own`, `apache-teacher`, `eval-only`). The trainer refuses `eval-only`.

**Targets:**

- ≥ 300 task templates × variants → ≥ 3,000 distinct task instances.
- **≥ 3,000 verified successful trajectories** (~20K step examples at ~7
  steps each) plus all failures kept for analysis and RL.
- ≥ 90% of generated checks executable; ≤ 5% check disagreement in human
  audits.

**Infrastructure:**

- Rollouts at k = 6 over 3,000 tasks ≈ 18K runs × ~7 steps ≈ 126K model calls.
  At ~1 s per call with 8–16 parallel workers on one H100 that is **~3–5 GPU
  hours of pure model time**, but env time dominates. Plan **40–80 H100 hours**
  of wall clock ($140–270) [Est.].
- CPU Droplets for the desktops.

**Exit:** `kai data export` produces a training split and a dev split; a
taxonomy report for the dev set; provenance enforced.

### Phase 2: SFT and rejection sampling (weeks 4–8)

**Goal:** a fine-tuned Gemma 26B-A4B that never needs a repair retry, takes
fewer steps, and passes more held-out tasks.

- **Being built this session:** `train/` scripts for SFT (and GRPO). They
  read the `kai data export` format.
- **Recipe:**
  - bf16 LoRA on attention and non-expert linear layers; rank 16–32.
  - Vision tower frozen first.
  - 1–2 epochs; loss on the reply only.
  - Image before text, matching Kai's prompt order.
- **Data transforms before training:** trajectory compression into `then`
  batches; thought shortening; keep the shortest passing sample.
- **Expert iteration:** 2–3 rounds of (roll out → filter by check → retrain).
  - Agent-RLVR-style guided retries on failures, with the guidance removed
    from the training input.
  - Apache-2.0 teacher rollouts (Holo3 / Qwen3.8 on the Droplet) only for
    tasks Gemma never solves, tagged `apache-teacher`.
- **Serving:** merge the adapter, serve the merged checkpoint, and run the
  adapter-changes-outputs probe.
- Run 31B through the same recipe once (QLoRA) to decide whether the careful
  slot is still needed.

**Targets (held-out, n = 3, vs Phase-0 Gemma baseline):**

- **Success +10 points** absolute.
- **Model calls per task −30%.**
- **Steps per task −25%.**
- JSON valid without repair ≥ 99.5%.
- External subset not lower than baseline.

**Infrastructure:**

- ~20K–40K step examples × ~3K tokens ≈ 60–120M tokens per epoch.
- At an assumed 1.5–3K tokens/s for MoE LoRA on one H100, that is
  **10–25 GPU hours per epoch**.
- With 3 rounds plus rollouts: **~100–150 H100 hours** ($340–510) [Est.,
  measure in the pilot].

**Risks:**

- MoE LoRA quality.
- The silent expert-adapter bug (mitigated by merging).
- Overfitting to the dev set (mitigated by held-out families).

### Phase 3: online RL with verifiable rewards (weeks 8–14)

**Goal:** pass Astra on held-out success.

**Recipe:**

- **Loop:** GRPO with DAPO-style clipping and a small KL to the SFT policy.
  - Group size 8; 32 tasks per iteration (ScaleCUA's shape); ≤ 30 steps per
    episode.
- **Reward:** `check` (0/1), plus −0.01 per step, plus a format term. A
  milestone/expect shaping term is optional and capped.
- **Stabilisers:** an ARPO-style replay buffer of successes; frontier task
  sampling (tasks with 0 < success < 1 in recent rounds); curriculum from the
  failure taxonomy (generate variants of failing families).
- **Done signal:** reward the model's `done` only when the check passes at
  that moment. Penalise `done` with a failing check more than a timeout.
- **Rollout/train split on one GPU:** alternate phases.
  - vLLM serves the merged current policy while 16–32 desktops roll out.
  - Then vLLM stops and the trainer takes the GPU.
  - Merge the new weights and restart vLLM.
  - ~5–10 min of swap overhead per iteration [Est.].
  - A second H100 for rollouts halves wall time; decide after the pilot.
- **Windows:** RL on Linux first. Windows data comes from SFT on
  Windows-VM rollouts, since VMs are slower and need licences. WAA's VM image
  is the reference setup.

**Targets (held-out, n = 3):**

- **Success ≥ GPT-6 Astra in Kai's harness**, with non-inferiority at −3
  points first, then superiority (§3.2).
- Steps per task ≤ Phase 2.
- External subset ≥ baseline.

**Infrastructure:**

- ~100–200 iterations × (256 rollouts + 1 update) ≈ **150–300 H100 hours**
  ($510–1,020) [Est.].
- 16–32 CPU desktops.

**Risks:**

- **Reward hacking of checks.** For example, a `shell` command that writes
  the expected file without doing the GUI task. Mitigations:
  - Checks inspect app state where the task is GUI.
  - `kind: gui` tasks forbid `shell` for the deciding step (the harness can
    enforce this per task).
  - Audit 5% of passing RL rollouts by hand.
- **Collapse or instability.** Mitigated by KL, the replay buffer and
  checkpoint gates on dev.
- **Single-run illusions.** Mitigated by n = 3 repeats and two training seeds
  for the final claim.

### Phase 4: speed distillation (weeks 6–16, in parallel)

**Goal:** make each step cheaper and each task shorter without losing success.

- **MTP on the Droplet.** A/B on Kai traffic (image steps and tree-only steps
  separately); keep it only if p50 improves by ≥ 15% at equal success.
- **Context distillation.** Train with a shortened system prompt; drop rules
  the model has learned; re-check held-out.
- **Learned routing:**
  - `need_screen` / tree-only share up, with screenshot recall held (no rise
    in "wrong because it couldn't see").
  - Optional E4B tree-only model distilled from the RL policy.
- **Skill mining.** Cluster successful trajectories; propose skills (Agent
  Skill format, `metadata.kai` steps) and plan-cache entries; the owner
  approves them in the Skills page.
- **Image budget per step:** 280 by default; 560 only for dense screens or
  retries (RESEARCH.md §1.2). Needs a per-request budget or a second server.

**Targets (Droplet, held-out):**

- **Step p50 ≤ 0.8 s, p95 ≤ 2.0 s.**
- **Steps per task ≤ 1.5× the reference replies** (today ~2.3×).
- Model calls per step ≤ 1.05.
- Completion tokens per call ≤ 50.
- **Median task wall time ≤ 50% of Astra's.**
- Inference cost ≤ $0.01 per task at Droplet utilisation ≥ 50% [Est.].

**Infrastructure:** ~30–60 H100 hours for A/Bs and the E4B distillation
[Est.].

### Phase 5: keep improving (ongoing, from week ~14)

**Weekly cycle:**

- Collect: bench, variants, synthetic and opt-in user exports.
- Label: check plus taxonomy.
- Retrain: SFT refresh, then a short RL round.
- **Gate:** dev, held-out, external subset and speed; a new model ships only
  if held-out success ≥ the previous model's (paired test, no significant
  drop) and speed does not regress.

**Each quarter:**

- Refresh held-out with ~25 new tasks.
- Re-run the frontier comparison against whatever is current. GPT-6.1 is
  already listed.
- Re-run OSWorld-Verified and WAA subsets.

**In the product:**

- The app's History page shows which model version answered.
- A one-click "this run was wrong" control feeds the taxonomy, opt-in and
  local first.

**Budget:** ~20–40 H100 hours per week [Est.] ($70–140).

### Phase summary

| Phase | Weeks | Held-out success target | Steps/task | Step p50 / p95 (Droplet) | Data | H100 hours [Est.] |
| --- | --- | --- | --- | --- | --- | --- |
| 0 Measure | 1–2 | baseline (n = 3, with interval) | baseline (dev today 6.8) | ≤ 1.5 / 3.5 s | held-out ≥ 100 tasks | 30–50 |
| 1 Flywheel | 2–5 | — | — | — | ≥ 3K verified successes, ≥ 3K task instances | 40–80 |
| 2 SFT + RFT | 4–8 | **+10 pts** vs baseline | **−25%** | ≤ 1.2 / 3.0 s | 20–40K step examples | 100–150 |
| 3 Online RL | 8–14 | **≥ Astra (same harness)** | ≤ Phase 2 | same | ~3K RL tasks, ~50K rollouts | 150–300 |
| 4 Speed | 6–16 | no drop | **≤ 1.5× reference** | **≤ 0.8 / 2.0 s** | compressed trajectories | 30–60 |
| 5 Ongoing | 14+ | ≥ previous model, every release | no regression | no regression | weekly increments | 20–40/week |

Total to the end of Phase 4: **~350–640 H100 hours ≈ $1.2K–2.2K** at
$3.39/hour [Est.], plus API spend for comparisons (~$300–600 [Est.]).

---

## 5. What already exists in the repo, and what is being built

| Piece | State | Where |
| --- | --- | --- |
| Step traces: frame, element list, decision, timings, path used, outcome | **Built** | `crates/kai-core/src/trace.rs` |
| 30 tasks with setup / check / teardown, Linux + Windows | **Built** | `bench/tasks.yaml`, `bench/fixtures/` |
| `kai bench` with per-backend metrics and a head-to-head report | **Built** | `crates/kai-cli/src/bench/` |
| Same-harness comparison via the Gateway (GPT models, Gemma) | **Built**; GPT-5 mini / 4.1 mini runs in progress | `client/openai.rs` |
| OpenAI native computer-use loop | **Built**, tested only against a mock server | `crates/kai-core/src/cua.rs` |
| Plan cache, skills, `then` batching, tree-only steps, a11y actions | **Built** | HARNESS.md |
| Trajectory export (`kai data export`) | **Being built** this session | format in `docs/DATASET.md` (being written) |
| Failure taxonomy | **Being built** | — |
| Task-variants generator | **Being built** | — |
| `train/` SFT and GRPO scripts | **Being built** | `train/` |
| `--repeat n`, seeds, paired tests in `kai bench report` | **To do** (Phase 0) | — |
| Held-out task set, external OSWorld-Verified / WAA subset | **To do** (Phase 0) | — |
| Rollout farm, synthetic task factory, provenance enforcement | **To do** (Phase 1) | — |

---

## 6. Risks

| Risk | Likelihood | Effect | Mitigation |
| --- | --- | --- | --- |
| Gemma 4 is a weaker computer-use base than Qwen3.8 / Holo3-class models (they claim 77.8–84.3% OSWorld-Verified; Gemma has no published number) | Medium | The ceiling is lower; more training needed | Phase 0 measures the open references in Kai's harness. If the gap is huge, tell the owner plainly; Kai's harness stays model-agnostic. |
| Overfitting to Kai's bench (OpenComputer-style collapse) | High without discipline | Fake progress | Held-out families, external subset at every gate, quarterly refresh |
| Reward hacking of `check`s | Medium | Learns shortcuts, not skills | GUI checks inspect app state; per-task action limits; 5% human audit of RL successes |
| Licence or terms misuse (training on GPT/Gemini outputs) | Low if enforced | Legal | Provenance tags; the trainer refuses `eval-only`; owner sign-off |
| vLLM MoE LoRA silently no-op (#50252) | Known | Wasted runs, false negatives | Merge adapters; adapter-changes-outputs probe |
| MTP gives little at batch 1 with images | Medium | Speed target slips | Speed comes mainly from fewer steps and tokens; MTP is a bonus |
| One H100 is the bottleneck for RL (rollout and train share it) | High | Slow iterations | Alternate phases; a second GPU only if the pilot shows > 50% idle desktops |
| Windows coverage lags Linux | High | Weaker Windows quality | Windows SFT data from VM rollouts; separate Windows held-out slice reported on its own |
| Frontier model moves (GPT-6.1 etc.) | Certain | Target moves | Define the win per release date; re-run quarterly; speed and cost lead is durable |
| Free-tier variance (429s, flaky 31B) pollutes numbers | High today | Noisy baselines | All official numbers from the Droplet or paid keys |
| Privacy of user screens in training | Medium | Trust | Local by default; opt-in export with redaction; no free-tier Gemini for user screens |

---

## 7. Sources (accessed 2026-10-10 unless dated)

**GPT-6 Astra**

- [OpenAI system card](https://deploymentsafety.openai.com/gpt-6-astra) (2026-09-03)
- [Vercel changelog](https://vercel.com/changelog/gpt-6-astra-now-available-on-vercel-ai-gateway) (2026-09-04)
- [Wikipedia](https://en.wikipedia.org/wiki/GPT-6_Astra)
- [DataCamp](https://www.datacamp.com/blog/gpt-6-astra)
- [BenchLM](https://benchlm.ai/models/gpt-6-astra)
- openai.com launch pages returned 403 to my fetcher, so vendor numbers are
  taken from these secondary summaries of OpenAI's tables.

**Leaderboards**

- [Steel OSWorld leaderboard](https://leaderboard.steel.dev/leaderboards/osworld/)
- [BenchLM OSWorld-Verified](https://benchlm.ai/benchmarks/osworld-verified)
- [OpenComputer](https://arxiv.org/abs/2605.19769) (2026-05)

**Open computer-use models**

- [Holo3 model card](https://huggingface.co/Hcompany/Holo3-35B-A3B) and
  [blog](https://hcompany.ai/newsroom/holo3) (2026-03-31)
- [Qwen3.8-27B card](https://huggingface.co/Qwen/Qwen3.8-27B)
- [DeskForge-Gemma4-E4B](https://huggingface.co/docling-project/DeskForge-Gemma4-E4B)

**RL and recipes**

- [DigiRL](https://arxiv.org/abs/2406.11896)
- [WebRL](https://arxiv.org/abs/2411.02337)
- [ARPO](https://arxiv.org/abs/2505.16282)
- [ZeroGUI](https://arxiv.org/abs/2505.23762)
- [SEAgent](https://arxiv.org/abs/2508.04700)
- [ScaleCUA](https://arxiv.org/abs/2607.11185)
- [UI-TARS-2](https://arxiv.org/abs/2509.02544)
- [UI-R1](https://arxiv.org/abs/2503.21620)
- [GUI-R1](https://arxiv.org/abs/2504.10458)
- [InfiGUI-R1](https://arxiv.org/abs/2504.14239)
- [GTA1](https://arxiv.org/abs/2507.05791)
- [Agent-RLVR](https://arxiv.org/abs/2506.11425)
- [Teach it to stop](https://arxiv.org/abs/2607.17136)
- [GUI-PRA](https://arxiv.org/abs/2509.23263)
- [ADMIRE](https://arxiv.org/abs/2602.11524)
- [VAGEN](https://arxiv.org/abs/2602.00575)

**Data**

- [OS-Genesis](https://arxiv.org/abs/2412.19723)
- [AgentTrek](https://arxiv.org/abs/2412.09605)
- [Explorer](https://arxiv.org/abs/2502.11357)
- [Aguvis](https://arxiv.org/abs/2412.04454)
- [OpenCUA](https://arxiv.org/abs/2508.09123)
- [Fara-7B](https://arxiv.org/abs/2511.19663)
- [OSWorld-Human](https://arxiv.org/abs/2506.16042)

**Gemma and serving**

- [Gemma 4 Apache 2.0](https://opensource.googleblog.com/2026/03/gemma-4-expanding-the-gemmaverse-with-apache-20.html)
- [Unsloth Gemma 4 training](https://unsloth.ai/docs/models/gemma-4/train)
- [vLLM #39246](https://github.com/vllm-project/vllm/issues/39246)
- [vLLM #50252](https://github.com/vllm-project/vllm/pull/50252)
- [vLLM #41745](https://github.com/vllm-project/vllm/pull/41745)
- [vLLM #58580](https://github.com/vllm-project/vllm/issues/58580)
- [vLLM AMD spec-decode blog](https://vllm.ai/blog/2026-08-23-speculative-decoding-amd-gpus)

**Terms**

- [OpenAI terms of use](https://openai.com/policies/terms-of-use/)
- [Gemini API additional terms](https://ai.google.dev/gemini-api/terms)

**Not found / not verified**

- No published OSWorld-Verified number for GPT-6 Astra.
- No published end-to-end computer-use score for any Gemma 4 model (only
  grounding numbers for the E4B fine-tune).
- No primary source for Astra's per-step latency.
- TRL's current multi-turn VLM GRPO API was not checked; the `train/` scripts
  being built should pin a version.
- The H100 training throughput figures in §4 are assumptions to be measured
  in the Phase 2 pilot.
