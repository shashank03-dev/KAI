# Research: making Kai faster first, accurate enough second

Written 2026-10-09 for Kai's speed round. Every factual claim has an inline
source link and the source's date (or "accessed 2026-10-09" when the page shows
no date). Each number carries a label:

- **[Kai]**: measured by Kai, in this repo or this session.
- **[Indep.]**: measured by someone other than the vendor (a paper, a
  benchmark, or a community harness).
- **[Vendor]**: claimed by the company that sells the thing.
- **[Aggregator]**: from a third-party leaderboard that collects self-reported
  numbers. These are useful for ranking, not as exact figures.

When a number could not be found or checked, this document says so instead of
guessing.

---

## 0. Kai's starting point (what is already measured)

| What | Number | Label |
| --- | --- | --- |
| Kai's own overhead per step (capture, tree, encode, act; model excluded), `kai bench --backend replay`, 5 real tasks | p50 **65 ms**, p95 **93 ms** | [Kai] (README, DECISIONS.md) |
| Observe (capture in parallel with the tree, then encode), 81-element GTK window | **~200 ms** (capture 19–28 ms, AT-SPI tree 136–198 ms, downscale + marks + JPEG 41–52 ms, JPEG ~128 KB) | [Kai] (DECISIONS.md, Phase 2b) |
| Vercel AI Gateway, free-tier key, `google/gemma-4-26b-a4b-it`, one tiny text request (first call) | **~2.1 s** to first byte | [Kai] 2026-10-09 |
| Gateway free tier: `openai/gpt-5`, `gpt-5-mini`, `gpt-5-nano`, `gpt-5.2`, `gpt-4.1(-mini/-nano)`, `gpt-4o(-mini)`, `o4-mini`, `gemini-2.5-flash(-lite)`, tiny text requests | **~0.8–2.0 s** total each | [Kai] 2026-10-09 |
| Gateway free tier **refused (403)**: `gemma-4-31b-it`, `typesafe-ai/jev`, `gpt-5.4` and newer, `gemini-3` and newer | n/a | [Kai] 2026-10-09 |

**Not measured yet:** end-to-end seconds per step and success rate with Gemma 4
on the Droplet or on the Gemini API (no GPU and no Gemini key where Kai was
built). The model call is almost certainly most of each step: Kai's own share is
65 ms, while the fastest model round trip seen today was ~0.8 s for a tiny text
request through the Gateway. So the biggest wins are **fewer model calls** and
**fewer or cheaper tokens per call**, not more harness tuning.

---

## 1. Gemma 4: what matters for a computer-use agent

### 1.1 The models

| Model | Params | Notes | Source |
| --- | --- | --- | --- |
| `gemma-4-26B-A4B-it` | 25.2B total, **3.8B active**, 128 experts (8 active + 1 shared), 30 layers, 256K context, ~550M vision encoder | Kai's default | [HF model card](https://huggingface.co/google/gemma-4-26B-A4B-it), accessed 2026-10-09 |
| `gemma-4-31B-it` | dense 31B, ~550M vision encoder | Kai's "careful" model | [Gemma 4 tech report](https://arxiv.org/abs/2607.02770), v2 2026-07-24 |
| E4B / E2B | edge models, **150M** vision encoder | much weaker on vision | same report |

Benchmark scores relevant to screens [Vendor]:

- MMMU-Pro: 31B **76.9**, 26B-A4B **73.8**, E4B 52.6, E2B 44.2. 31B from the
  [tech report](https://arxiv.org/html/2607.02770) (Table 6, 1120-token
  budget, thinking on); 26B-A4B from the
  [model card](https://huggingface.co/google/gemma-4-26B-A4B-it); E4B/E2B as
  reported by [BenchLM](https://benchlm.ai/compare/gemma-4-26b-a4b-vs-gemma-4-31b)
  [Aggregator].
- **The budget matters for dense screens.** 31B at 1120 vs 280 tokens (both with
  thinking on): InfographicVQA **92.0 vs 82.8**, OmniDocBench edit distance
  **0.131 vs 0.201** (lower is better), MMMU-Pro 76.9 vs 75.8
  ([tech report](https://arxiv.org/html/2607.02770) Tables 6 and 12, 2026-07).
  Text-heavy pages gain a lot from more tokens; general reasoning gains little.
- τ2-bench (tool use): 26B-A4B **68.2** ([model card](https://huggingface.co/google/gemma-4-26B-A4B-it)).
- **No published GUI-grounding or computer-use scores for Gemma 4.** I found no
  ScreenSpot, OSWorld-G or OSWorld numbers in the model card, the tech report,
  or the [OSWorld-Verified aggregator](https://benchlm.ai/benchmarks/osworld-verified)
  (updated 2026-10-09). Kai's own bench will be the first number.
- Hugging Face's launch post shows Gemma 4 returning GUI element boxes as JSON
  (`{"box_2d": [...], "label": ...}`) in a **0–1000 normalised frame**, with
  "no need for specific instructions or grammar-constrained generation"
  ([HF blog](https://huggingface.co/blog/gemma4), 2026-04-02). Kai never asks for
  coordinates (it asks for a mark number), so this only matters for the grid
  fallback and the zoom step.

**26B-A4B vs 31B vs E-models for Kai.** The MoE runs ~3.8B active parameters
per token, so decoding is much cheaper than the dense 31B, at about 3 points
lower MMMU-Pro. Kai's current policy (26B-A4B by default, 31B for the one step
after a failure) matches the data. E4B/E2B are 20+ points lower on MMMU-Pro and
use a smaller vision encoder. They are not good enough to choose actions, but
an E-model could still do cheap text-only jobs such as summarising tree diffs.
That is not measured.

### 1.2 Image soft-token budgets and the best screenshot size

Facts:

- Budgets: **70, 140, 280 (default), 560, 1120** soft tokens per image, and the
  budget is a hard cap ([Gemma vision docs](https://ai.google.dev/gemma/docs/capabilities/vision),
  updated 2026-04-02).
- Patch size **16 px**, 3×3 pooling, so **one soft token = a 48×48 px area**.
  Height and width are resized to multiples of 48, aspect ratio is kept, and the
  total area must fit the budget ([Transformers Gemma4 docs](https://huggingface.co/docs/transformers/model_doc/gemma4),
  accessed 2026-10-09; [tech report §2.1](https://arxiv.org/html/2607.02770)).
- Pixel budget per image is about soft tokens × 2,304 px: 70 → ~161K px,
  140 → ~323K, **280 → ~645K**, 560 → ~1.3M, 1120 → ~2.6M (same Transformers page).
- Lower budgets suit classification and video frames. Higher budgets suit OCR,
  documents and small text ([model card](https://huggingface.co/google/gemma-4-26B-A4B-it)).
- On vLLM the budget is set server-wide with
  `--mm-processor-kwargs '{"max_soft_tokens": 560}'`. Offline inference can
  override it per request ([vLLM recipe](https://recipes.vllm.ai/Google/gemma-4-26B-A4B-it),
  updated 2026-09-12).

What this means for Kai (my analysis, not measured):

| Budget | Max area | Example 16:9 frame (my estimate, multiples of 48) | Tokens |
| --- | --- | --- | --- |
| 140 | ~323K px | ~720×384 | ~120 |
| **280** | ~645K px | **~1056×576** | ~264 |
| 560 | ~1.3M px | ~1488×816 | ~527 |
| 1120 | ~2.6M px | ~2112×1200 | ~1100 |

The exact rounding in the processor is not documented, so treat the frame sizes
as estimates.

- **Kai sends 1280×720 (921,600 px) at the 280 default.** That is more than
  280 tokens can hold, so vLLM shrinks it again to about 1056×576. Kai pays to
  JPEG-encode and upload ~43% more pixels than the model ever sees. The marks
  are also resampled twice, which can blur the 3×5 bitmap digits that
  Set-of-Marks depends on.
- **Recommendation:** resize once to the processor's target (about 1056×576 for
  16:9 at 280) and draw the marks at that size. Raise the budget to 560 only for
  dense screens (more than ~60 marks, or small text) or after a missed click.
  vLLM sets the budget per server, so per-step switching means running a second
  server or using a request-level override if the server allows one (verify on
  the deployed vLLM version).
- **The element list costs more than the image.** 81 elements at roughly 8–10
  text tokens each is ~650–800 tokens, against ~264 image tokens. Trimming the
  list (only on-screen, interactive, deduped elements, as Kai already does, plus
  shorter labels) saves about as much prefill as halving the image budget.
- **Prompt order:** Google says to put image content **before** text in the
  turn, matching how Gemma 4 was trained ([model card](https://huggingface.co/google/gemma-4-26B-A4B-it);
  [HF blog](https://huggingface.co/blog/gemma4)). Anthropic advises the
  opposite for Claude (instruction text before the screenshot,
  [Claude computer-use docs](https://platform.claude.com/docs/en/docs/agents-and-tools/tool-use/computer-use-tool),
  accessed 2026-10-09). This is a per-model setting. For Gemma: stable system
  prompt first (keeps the prefix cache), then in the user turn the image, then
  the element list, history and task.

### 1.3 Thinking on/off

- vLLM: thinking is **off unless asked**, per request with
  `chat_template_kwargs: {"enable_thinking": true}` plus
  `--reasoning-parser gemma4` ([vLLM recipe](https://recipes.vllm.ai/Google/gemma-4-26B-A4B-it)).
- Model level: thinking is turned on by a `<|think|>` token at the start of the
  system prompt. With thinking off, the 26B and 31B models (not E2B/E4B) still
  emit an **empty thought block** (`<|channel>thought\n<channel|>`) before the
  answer ([model card](https://huggingface.co/google/gemma-4-26B-A4B-it)). That
  costs only a few tokens. **Verify** that it does not conflict with
  `response_format: json_schema` on the Droplet: if the grammar starts at the
  first generated token, the empty block might be forced out, or might break the
  parse.
- Cost: neither the tech report nor the model card puts a number on latency or
  tokens for thinking ([tech report](https://arxiv.org/html/2607.02770): "All
  models are in thinking mode unless explicitly stated", no latency figures).
  For a one-action JSON reply, thinking adds hundreds to thousands of tokens
  per step, at roughly TPOT × tokens. **Keep it off** for routine steps. If
  anything, allow it on the 31B "careful" step after a failure.
- Gemini API: for Gemma 4, `thinkingLevel: "high"` turns thinking on and
  **`"minimal"` turns it off** ([Gemma on Gemini API](https://ai.google.dev/gemma/docs/core/gemma_on_gemini_api),
  updated 2026-07-02). Kai currently sends `"low"`. **Switch the fallback to
  `"minimal"`** and keep the existing retry-without-thinking path.
  (DECISIONS.md said "low" is the lowest level Gemma accepts; the current page
  says otherwise.)

### 1.4 JSON reliability

- vLLM: `response_format: {type: "json_schema"}` gives constrained decoding.
  **The model does not see schema field descriptions**, so output rules belong
  in the system prompt ([vLLM Gemma 4 guide](https://docs.vllm.ai/projects/recipes/en/stable/Google/Gemma4.html),
  2026-04). Kai already does this.
- Gemini API: Google's Gemma page does not document JSON mode or
  `responseJsonSchema` ([page](https://ai.google.dev/gemma/docs/core/gemma_on_gemini_api)).
  One developer reports that **`responseJsonSchema` works for `gemma-4-31b-it`**:
  parse success went from about 50% to 99%+, and stray thinking text
  disappeared, while `responseMimeType: application/json` alone was ignored.
  They also report that the free tier returns "Internal error" when a schema is
  sent ([DEV post](https://dev.to/ai_made_tools/responsejsonschema-the-undocumented-gemma-4-feature-that-changed-everything-2obm),
  2026-05-11) [Indep., anecdotal, n not given]. **Worth a try in
  `GeminiClient`:** send `responseJsonSchema` and fall back to JSON mode plus
  one repair if it is rejected. It could remove most repair retries on the
  fallback path.
- Keras notes that Gemma often writes a short explanation before the JSON when
  it is not constrained ([KerasHub guide](https://keras.io/keras_hub/guides/gemma4_multimodal_and_agentic_workflows/),
  accessed 2026-10-09). Kai's `extract_json_object` already handles this.

### 1.5 vLLM serving flags for latency (single user, batch ≈ 1)

| Flag | Effect for Kai | Source |
| --- | --- | --- |
| `--enable-prefix-caching` (on by default in current vLLM; Kai sets it) | Reuses the system prompt and schema KV every step. Disable it only when benchmarking. | [vLLM Gemma 4 guide](https://docs.vllm.ai/projects/recipes/en/stable/Google/Gemma4.html) |
| `--async-scheduling` | Overlaps scheduling with decoding; recommended. Kai sets it. | [recipe](https://recipes.vllm.ai/Google/gemma-4-26B-A4B-it), 2026-09-12 |
| `--max-num-seqs 8–16` | Recipe's "min latency" row (with TP 4–8). Kai has one H100, so TP=1; a small `max-num-seqs` still keeps the scheduler lean. Sub-agents may run 4 at once, so use ≥ 4. | same |
| `--mm-processor-kwargs '{"max_soft_tokens": N}'` | Image budget (see §1.2). | same |
| `--limit-mm-per-prompt.image 2` | Kai's screenshot plus an optional crop. Kai sets it. | same |
| `--kv-cache-dtype fp8` | ~50% less KV memory. That is a capacity win; at Kai's short contexts it is **not** a latency win, and it can cost a little accuracy. Leave it off unless memory runs short. | same |
| `--mm-processor-cache-gb` (default 4 GiB) | Caches processed images. Screenshots change every step, so this rarely hits; leave the default. | [vLLM multimodal config](https://docs.vllm.ai/en/latest/api/vllm/config/multimodal/), accessed 2026-10-09 |
| Chunked prefill | On by default in vLLM V1. With one request at a time it does not matter much. | general vLLM behaviour; not separately sourced |
| **MTP speculative decoding** | `--speculative-config '{"method":"mtp","model":"google/gemma-4-26B-A4B-it-assistant","num_speculative_tokens":4}'`. **Nightly-only** per the recipe (2026-09-12); images `vllm/vllm-openai:gemma4-0505-cu129` for Hopper. | [recipe](https://recipes.vllm.ai/Google/gemma-4-26B-A4B-it); [vLLM PR #41745](https://github.com/vllm-project/vllm/pull/41745) |

MTP numbers:

- Google claims **"up to 3x"** faster decoding with no quality loss. For the
  26B MoE on Apple Silicon it says batch 1 "presents routing challenges", and
  batch 4–8 reaches "up to ~2.2x" ([Google blog](https://blog.google/innovation-and-ai/technology/developers-tools/multi-token-prediction-gemma-4/),
  2026-05-05) [Vendor].
- An independent write-up reports **only +13% for 26B-A4B** on an M1 Max, with
  31B dense and E4B **slower**. Its explanation: verifying draft tokens loads
  extra experts ([lilting.ch](https://lilting.ch/en/articles/gemma-4-mtp-drafter-speed),
  2026-05-07) [Indep., one machine, no exact figures].
- One search snippet quotes a vLLM error that speculative decoding with draft
  models "does not support multimodal models yet". Its date is unclear. **Test
  MTP with image requests on the exact image before relying on it.**
- **For Kai:** replies are ~40–120 tokens, so decoding is a modest share of each
  step and MTP on the 26B MoE at batch 1 is likely a small win. On the **31B**
  careful step it could matter more. Treat MTP as "measure on the Droplet",
  not a default.

**Streaming and first token.** vLLM's OpenAI-compatible server streams SSE
chunks. Measuring **TTFT separately from total time** tells prefill cost
(image + list) apart from decode cost (reply length). That is what decides
between "fewer tokens in" and "fewer tokens out". The recipe's benchmark tool
reports TTFT and TPOT ([vLLM guide](https://docs.vllm.ai/projects/recipes/en/stable/Google/Gemma4.html)).

### 1.6 Structured-output backends and their overhead

- xgrammar is vLLM's default backend for JSON schema
  ([vLLM forum](https://discuss.vllm.ai/t/as-of-july-2026-what-are-some-general-tips-in-improving-the-speed-of-guided-decoding-in-vllm/2845),
  2026-07) [community].
- The best primary measurement is old: the XGrammar paper (vLLM 0.6.3, H100,
  Llama-3.1-8B) found per-token grammar cost near zero for xgrammar, with a
  compile step that can overlap with prefill
  ([XGrammar paper](https://arxiv.org/pdf/2411.15100), 2024-11;
  [JSONSchemaBench](https://arxiv.org/pdf/2501.10868), 2025-01) [Indep.].
  I found **no 2026 primary benchmark** of xgrammar overhead for Gemma 4.
- Practical points for Kai: keep the schema **identical across steps** so the
  compiled grammar is reused, and keep it small (enums, few optional fields).
  The new `then` / `need_screen` fields from the multi-action work should stay
  flat and bounded (`maxItems: 3`). Guidance (`llguidance`) is the alternative
  backend if a schema feature is missing; no Gemma-specific comparison was found.

---

## 2. Gemini API (Kai's fallback) and Google's computer-use model

### 2.1 Gemma through `generateContent`

- IDs: `gemma-4-26b-a4b-it` and `gemma-4-31b-it`. Image input and function
  calling are supported. Thinking is toggled with `thinkingLevel`
  (`"minimal"` off, `"high"` on) ([Gemma on Gemini API](https://ai.google.dev/gemma/docs/core/gemma_on_gemini_api),
  2026-07-02).
- Rate limits, streaming and caching for Gemma are **not documented** on that
  page. The general caching page says implicit caching is on by default for
  **Gemini 2.5 and newer**, with minimum prefixes of 2,048 tokens (2.5 Flash/Pro)
  or 4,096 (3.x). It does **not mention Gemma**
  ([context caching](https://ai.google.dev/gemini-api/docs/caching), 2026-09-02).
  Kai's stable prefix (system prompt + schema) is probably under 2,048 tokens,
  so implicit caching would likely not trigger even on Gemini models. Not
  measured.
- **Latency:** no official numbers exist for Gemma on the Gemini API. Kai
  measured ~2.1 s TTFB for one tiny text request to Gemma 26B through the
  Vercel Gateway [Kai]; direct Gemini API latency for Gemma was **not measured**
  (no key).
- `thinkingLevel` for Gemini models: `minimal / low / medium / high`, but not
  every model accepts `minimal` (for example, 2.5 Flash supports only
  low/medium/high). Google advises lowering the thinking level, not
  `max_output_tokens`, to cut latency
  ([thinking docs](https://ai.google.dev/gemini-api/docs/thinking), 2026-09-25).

### 2.2 Gemini computer use (for comparison)

- **Gemini 2.5 Computer Use** (2025-10-07): a `computer_use` tool loop; the
  model returns one UI action per call, then gets a screenshot and URL back.
  It was "primarily optimized for web browsers" and "not yet optimized for
  desktop OS-level control". Google claimed better results than the
  alternatives on Online-Mind2Web, WebVoyager and AndroidWorld "with lower
  latency". The chart shows "~225 s" and "70%+" on the Browserbase harness,
  without clearly saying which run that is ([Google blog](https://blog.google/technology/google-deepmind/gemini-computer-use-model/),
  2025-10-07) [Vendor]. Reported scores: Online-Mind2Web **65.7%** (Browserbase)
  and AndroidWorld **69.7%** ([VentureBeat](https://venturebeat.com/ai/googles-ai-can-now-surf-the-web-for-you-click-on-buttons-and-fill-out-forms))
  [Vendor/Indep. mix].
- **Today (docs updated 2026-10-01)** computer use is a tool on
  `gemini-3.8-flash` (recommended), 3.7/3.5 Flash, 3.5 Flash-Lite and
  3-flash-preview, with **browser, mobile and desktop** environments
  ([Gemini computer-use docs](https://ai.google.dev/gemini-api/docs/computer-use)).
  - Actions: click, double/triple/middle/right click, mouse down/up, move,
    type (`press_enter`), drag_and_drop, wait, press_key, key_down/up, hotkey,
    take_screenshot, scroll (`magnitude_in_pixels`), plus navigate/back/forward
    in the browser.
  - Coordinates are **0–999 normalised**, and every action has an `intent`
    field.
  - **Several function calls can come back in one response**, each needing its
    own result.
  - Safety: `safety_decision: require_confirmation | blocked` with policy
    categories (financial, communication, data modification and others). This
    is a useful reference for Kai's risk classifier.
- Kai cannot call Gemini 3+ through the free Gateway key (403 [Kai]). A direct
  Gemini key would be needed for a native Gemini computer-use comparison.

---

## 3. What fast computer-use agents do

### 3.1 Where the time goes

- **OSWorld-Human** (UCSD; v2 2026-05-18, MLSys 2026): top agents take
  **2.7–4.3× more steps** than a human-annotated minimal trajectory. Large model
  calls for **planning, reflection and judging** take most of the latency, and
  later steps can take **3× longer** than early ones as context grows
  ([arXiv 2506.16042](https://arxiv.org/abs/2506.16042)) [Indep.].
  - Grouping several actions under one observation ("grouped-action"
    trajectories) cuts steps a lot, for example LibreOffice Calc from **13.6 to
    5.9** average steps ([paper HTML](https://arxiv.org/html/2506.16042v1)).
  - Agent S2 with Gemini 2.5 scored 41.4% success but only **17.4%** on the
    efficiency score against grouped trajectories.
- **For Kai:** Kai already avoids the expensive parts (no separate
  planner/reflector/judge calls, only the last 3 steps as text, no old
  screenshots). The remaining lever is **steps per task**: batch the actions the
  model is sure about, and replace mouse round-trips with direct accessibility
  actions.

### 3.2 Action batching and multi-action steps (strong evidence)

- **OpenAI computer tool:** a `computer_call` holds an ordered **`actions`
  array** (the docs' example batches a click and a `type`). The advice is to
  "return a fresh screenshot after a short group of actions". The recommended
  path is now **code execution** (the model writes PyAutoGUI/Playwright code
  that can loop and branch), which removes round trips
  ([OpenAI docs](https://developers.openai.com/api/docs/guides/tools-computer-use),
  accessed 2026-10-09).
- **Anthropic** `computer_toolset_20260801`: Claude can return several actions
  in one response. Run them in order, **stop at the first failure**, and mark
  the rest "Not executed". The docs also say "Batch actions reduce round trips"
  and suggest ending each batch with a screenshot
  ([Claude docs](https://platform.claude.com/docs/en/docs/agents-and-tools/tool-use/computer-use-tool),
  accessed 2026-10-09).
- **Gemini computer use** also returns several calls per response (§2.2).
- **UFO2** (Microsoft, 2025-04): "speculative multi-action" checks each queued
  action against the live UI Automation (UIA) state before running it. Steps on
  OSWorld-W dropped **13.3 → 7.4** (GPT-4o) and **6.8 → 3.3** (o1), with the
  same or better success (22.4 → 24.5%, 24.5 → 26.5%). On Windows Agent Arena
  (WAA), steps fell about 11–12% at the same success rate
  ([arXiv 2504.14603v2](https://arxiv.org/html/2504.14603v2), Table 7) [Indep.,
  vendor-lab paper].
- **The design pattern to copy:** the model may add up to N follow-up actions
  it is certain about. Before each one, Kai re-checks the **tree** (target
  still present and enabled, no new dialog) and stops the batch on any
  surprise, on a screen-hash change it did not expect, or on any action that is
  not low risk. One screenshot comes after the batch.

### 3.3 Accessibility-first control (strong evidence)

- **UIA control patterns** on Windows give direct, mouse-free operations:
  Invoke (buttons), Value (set text), Toggle (check boxes), ExpandCollapse
  (menus, combos), SelectionItem, Scroll/ScrollItem, Text/TextEdit
  ([Microsoft Learn](https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-controlpatternsoverview),
  2025-07-14).
- **AT-SPI** on Linux: `Action.do_action(i)` with
  `get_n_actions`/`get_action_name` ([docs](https://docs.gtk.org/atspi2/iface.Action.html)),
  and `EditableText.set_text_contents` / `insert_text`
  ([docs](https://docs.gtk.org/atspi2/iface.EditableText.html)), accessed
  2026-10-09.
- Evidence it helps:
  - UFO2 with native API actions (12 APIs, 27 office tasks): o1 success
    **16.3% → 24.5%**, average steps **16.0 → 6.6**; GPT-4o 16.3% → 22.4%.
  - Hybrid UIA + vision element detection beat UIA alone (WAA o1: 25.3% →
    27.9%) ([UFO2](https://arxiv.org/html/2504.14603v2), Tables 3 and 5).
  - Agent S2's text/structure grounding experts drove its gains in the Office
    category ([arXiv 2504.00906](https://arxiv.org/abs/2504.00906)).
- **Why it is faster for Kai:** no pointer travel, no hit-test on a covered
  window, no focus race. Text is set in one call instead of typed or pasted
  (Kai currently pastes text over 32 characters through the clipboard). A
  Value/EditableText set can also be **confirmed from the tree alone**, with no
  screenshot.
- **Caveats:**
  - Some apps expose Invoke without actually working, and Electron/Chromium
    trees can be partial.
  - Keep the mouse path as the fallback, and verify each pattern call from the
    tree (value changed? element gone or expanded?).
  - **Risk rules must apply to a11y actions exactly as to clicks.** Invoking
    "Send" is still a send.

### 3.4 Skipping the screenshot (tree-only steps)

- There is no paper that tests "skip the image when the tree is enough" as a
  policy (searched 2026-10-09). The closest evidence:
  - An engineering PR in the Hermes agent split capture into lanes: tree-only
    took **~47 ms** against **248–478 ms** for a full capture
    ([NousResearch PR #126447](https://github.com/NousResearch/hermes-agent/pull/126447),
    accessed 2026-10-09) [Indep., not a benchmark].
  - Original OSWorld: GPT-4 with the **accessibility tree alone** was the
    best-performing setting (12.24%), as reported second-hand
    ([OSWorld paper](https://arxiv.org/abs/2404.07972), 2024-04; I could not
    re-check Table 3 directly).
  - Counter-evidence: trees are not always available and do not always help
    ([RedTeamCUA](https://arxiv.org/pdf/2505.21936), 2025-05), and full tree
    crawls can be slow on some apps ([PC Agent](https://arxiv.org/pdf/2412.17589),
    2024-12).
- **For Kai:** a tree-only step saves the JPEG encode (41–52 ms [Kai]), the
  upload (~128 KB [Kai]), the vision encoder and ~264 image tokens of prefill.
  Use it only when (a) the tree is rich (≥ N useful elements, not the grid
  fallback), (b) the last action's `expect` can be checked in the tree (a field
  value, a new window title, a dialog that appeared or closed), and (c) the
  model did not ask for the screen (`need_screen`). Force a screenshot every K
  steps and after any surprise.

### 3.5 Keyboard-first

- Anthropic: "use keyboard shortcuts when dropdowns or scrollbars are hard to
  manipulate" ([Claude docs](https://platform.claude.com/docs/en/docs/agents-and-tools/tool-use/computer-use-tool)).
- OpenAI's code mode exists to collapse many UI actions into one call
  ([OpenAI docs](https://developers.openai.com/api/docs/guides/tools-computer-use)).
- Kai already has `key`, `open_url`, `open_app` and `shell`. Add a prompt rule
  ("prefer a known shortcut, URL or shell command over a multi-click path") and
  make sure `type` can end with Enter in the same action (Gemini's `type` has
  `press_enter`).

### 3.6 Grounding models and zoom

| System | Result | Date | Label |
| --- | --- | --- | --- |
| ScreenSpot-Pro (pro apps, high-res) | best model **18.9%**; **ScreenSeekeR** cascaded zoom search **48.1%** with no training | [2025-04](https://arxiv.org/abs/2504.07981) | [Indep.] |
| Jedi / OSWorld-G | 4M-example grounding data; better grounding lifts general models on OSWorld from **5% to 27%** | [2025-05, v3 2025-10](https://arxiv.org/abs/2505.13227) | [Indep.] |
| GTA1 | RL-trained grounding plus test-time scaling (sample several actions, judge picks one); claims SOTA | [2025-07, v5 2025-10](https://arxiv.org/abs/2507.05791) | [Vendor/paper] |
| OmniParser v2 | screen parser; **0.6 s/frame on A100**, 0.8 s on a 4090; ScreenSpot-Pro 39.6 | [HF card](https://huggingface.co/microsoft/OmniParser-v2.0), 2025 | [Vendor] |
| ShowUI-2B | UI-guided visual token selection drops **~33%** of redundant tokens, **1.4×** faster training; 75.1% zero-shot grounding | [2024-11](https://arxiv.org/abs/2411.17465) | [paper] |
| Anthropic `zoom` action | returns a region at full resolution | [docs](https://platform.claude.com/docs/en/docs/agents-and-tools/tool-use/computer-use-tool) | [Vendor] |

**For Kai:** Set-of-Marks from the tree means Kai rarely needs pixel grounding.
Two cheap borrowings:

1. **Zoomed crop after a missed click** (ScreenSeekeR/Anthropic-style): when a
   grid-mode click produces no change, crop the cell's neighbourhood to the
   budget, number the sub-cells and ask again. This costs one extra call only
   on misses.
2. A separate parser model (OmniParser) costs 0.6–0.8 s per frame on a GPU.
   That is too slow for every step; use it only when the tree is empty.

### 3.7 Leaderboards (latest numbers I could find, with dates)

**OSWorld / OSWorld-Verified** (369 tasks; human **72.36%**,
[OSWorld site](http://osworld-v1.xlang.ai/), accessed 2026-10-09; the site's
live table did not load, so these are from papers and aggregators):

| Agent / model | Score | Date | Label |
| --- | --- | --- | --- |
| OpenAI CUA (Operator) | OSWorld **38.1%**; WebArena 58.1%; WebVoyager 87% | [2025-01-23](https://openai.com/index/computer-using-agent/) | [Vendor] |
| UI-TARS-2 | OSWorld **47.5**, WindowsAgentArena **50.6**, AndroidWorld 73.3, Online-Mind2Web 88.2 | [2025-09](https://arxiv.org/abs/2509.02544) | [Vendor/paper] |
| Agent S3 + Behavior Best-of-N | **69.9%** (N=10, 100 steps), later **72.6%** (above human) | [2025-10, v2 2026-02](https://arxiv.org/abs/2510.02250); [LessWrong](https://www.lesswrong.com/posts/rft2jio6CzvmsvTua/behavior-best-of-n-achieves-near-human-performance-on) | [paper] |
| GPT-5.4 (native computer use) | OSWorld-Verified **75.0%** | 2026-03-05 ([secondary](https://winbuzzer.com/2026/03/06/openai-launches-gpt-54-with-computer-use-and-finance-tools-xcxwbn/)) | [Vendor] |
| BenchLM aggregator top | Qwen3.8 Max **86.1%**; Claude Fable 5 / Mythos 5 85%; Gemini 3.6 Flash 83%; GPT-5.5 78.7%; **GPT-5.2 47.3%**; GPT-5.4 nano 39%; **no Gemma entry** | [updated 2026-10-09](https://benchlm.ai/benchmarks/osworld-verified) | [Aggregator] |

Note on the GPT comparison: the strongest OpenAI model Kai's free Gateway key
can call is **gpt-5.2**, listed at 47.3% on OSWorld-Verified. That score is
GPT-5.2 in OpenAI's own harness, not in Kai's.

**OSWorld 2.0** (2026-06-26; 108 long workflows, median ~1.6 h per task for
humans): the best reported is **20.6%** binary completion (Claude Opus 4.8,
500-step cap), with **>250 steps per task** on average and ~318 tool calls for
Opus 4.7 against ~30 in OSWorld 1.0 ([OSWorld 2.0](https://osworld-v2.xlang.ai/),
accessed 2026-10-09) [paper]. Steps per task, not seconds per step, is what
grows.

**Windows Agent Arena** (154 tasks; human **74.5%**; original Navi agent
**19.5%**, [2024-09](https://arxiv.org/abs/2409.08264)). Later results:

- UFO2: **30.5%** (o1) against Operator's **20.8%**
  ([UFO2](https://arxiv.org/html/2504.14603v2), 2025-04).
- Agent S2: **+52.8% relative** over Navi ([2025-04](https://arxiv.org/abs/2504.00906)).
- UI-TARS-2: **50.6** (2025-09).

I found no maintained 2026 WAA leaderboard.

**Time per task** is rarely reported. Google's Online-Mind2Web chart (~225 s)
and OSWorld-Human's "tens of minutes" for top agents are the only figures found.

### 3.8 Memory, plan caching and auto-skills

- **Agent Workflow Memory:** reusable workflows learned from past trajectories
  raised success by **24.6%** (Mind2Web) and **51.1%** (WebArena) relative, and
  cut steps on WebArena ([arXiv 2409.07429](https://arxiv.org/abs/2409.07429),
  2024-09) [paper].
- **For Kai:** replaying a successful run's deterministic prefix (open_url /
  open_app / shortcut / low-risk shell) costs **zero model calls**, so it is
  the best case for speed. The risk is replaying a stale or wrong plan, so match
  strictly and hand over to the model with a fresh full observation once the
  prefix is done. Kai's skills (`metadata.kai` steps) already do this by hand.

### 3.9 Planner/executor splits and speculative execution

- Agent S2/S3, UFO2's HostAgent/AppAgent split and GTA1's judge all add
  separate model calls. OSWorld-Human shows those calls dominate latency. **For
  speed, a split pays off only if the executor is much cheaper than Gemma**
  (deterministic code, a cached plan, or a tiny model) and re-planning is rare.
- **Speculative Actions** (Columbia, ICLR 2026): a fast model guesses the next
  actions, and they run ahead of time, kept only if the slow model agrees. Up
  to **55%** next-action prediction accuracy and "substantial latency
  reductions" ([arXiv 2510.04371](https://arxiv.org/abs/2510.04371)) [paper].
  On a desktop this is safe only for **side-effect-free** actions (scroll, read
  the tree, open a menu).
- **Pre-compiled policy trees (AAPT):** while idle, build a small tree of
  guarded, pre-authorised actions, then fire a branch the moment its guard
  matches. Success inside a short decision window rose **0.50 → 0.79**
  ([arXiv 2607.28399](https://arxiv.org/abs/2607.28399), 2026-07-30) [paper].
  Kai's version: a multi-action reply with guards ("then press Enter if the
  field shows X").
- **AOI** (2026-06): keyframes between steps turned into persistent text gave
  +17 to +48 pp on dynamic tasks. On Gemini 3 Flash, extra images *hurt*
  through "image-token dilution" ([arXiv 2606.29472](https://arxiv.org/abs/2606.29472)).
  For Gemma, more images per step is not free accuracy either.

---

## 4. Vercel AI Gateway

**What it is:** one OpenAI-compatible endpoint, `https://ai-gateway.vercel.sh/v1`,
with `/chat/completions` (streaming, image attachments, tool calling, structured
outputs), `/models`, `/embeddings` and the Decisions API (Jev)
([docs](https://vercel.com/docs/ai-gateway/sdks-and-apis/openai-chat-completions),
updated 2026-10-07).

**Pricing:**

- **No markup** on tokens: you pay the provider's list price
  ([pricing](https://vercel.com/docs/ai-gateway/pricing), updated 2026-09-08).
- The free tier is a **subset of models** with **lower per-model rate limits**
  (429 when exceeded). Buying credits moves the team to the paid tier and ends
  the monthly free credit.
- The pricing page does not state the free amount. Community threads say
  **$5/month** ([Vercel community](https://community.vercel.com/t/inquiry-about-reset-date-for-my-free-5-ai-gateway-credits/41272)).
- BYOK requires purchased credits.

**Gemma 4 26B on the Gateway:** providers novita, parasail, vertex and
gmicloud; **$0.13 / M input, $0.40 / M output**; text + image input. The page
says structured output is supported but "detailed capability metadata has not
been reported" ([model page](https://vercel.com/ai-gateway/models/gemma-4-26b-a4b-it),
accessed 2026-10-09). Whether `response_format: json_schema` is enforced
depends on which provider serves the request. **Verify before relying on it**,
and pin a provider with `providerOptions.gateway.only` (free per request).

**Latency overhead:**

- Independent: **+164 ms** median TTFT on gpt-5.4 (938 vs 774 ms direct) and
  **+193 ms** on claude-sonnet-4-6, from 43 GCP regions, 16,344 measurements
  ([Edgee](https://www.edgee.ai/blog/posts/i-benchmarked-six-ai-gateways-including-ours),
  2026-08-10) [Indep., but Edgee sells a competing gateway].
- A community report saw 300–700 ms extra on Gemini via Vertex, not confirmed
  ([Vercel community](https://community.vercel.com/t/latency-of-ai-gateway-in-aisdk/47213)).
- Kai saw ~0.8–2.0 s total for tiny text requests and ~2.1 s TTFB for Gemma 26B
  (first call) [Kai, single samples].

### Decision

**(a) GPT comparison models: YES, route them through the Gateway.**

- One OpenAI-compatible client (practically the same code as `VllmClient`)
  reaches `gpt-5.2`, `gpt-5`, `gpt-5-mini`, `gpt-4.1-mini` and `o4-mini` with
  one key. That makes a **same-harness** head-to-head possible today: Kai's
  Set-of-Marks prompt, Kai's tasks, Kai's checks.
- Overhead (~0.16–0.2 s TTFT) is applied to every comparison model alike, but
  it should be **reported**, not hidden.
- OpenAI's **native** computer-use loop (Responses API `computer` tool / code
  mode) needs a direct OpenAI key and newer models (gpt-5.4+ is refused on the
  free Gateway tier). Label it as a separate comparison.

**(b) A tiny fast router/verifier on the per-step critical path: NO, not
through the free Gateway.**

- The cheapest chat models measured 0.8–2.0 s for a tiny request [Kai]. That is
  slower than a Gemma step should be on the Droplet, so it would make Kai
  slower.
- Use a verifier **only off the critical path**: concurrently with the next
  observe/decide, or after a batch, or for the final "is the task done?" check.
- The right model for that job is Jev (§5), which needs paid credits.

**(c) Gemma 26B through the Gateway as a third fallback: OPTIONAL.** It is
useful when there is no Droplet and no Gemini key ($0.13/$0.40 per M is
cheap). Expect ~2 s round trips and provider variability. Keep the order
Droplet → Gemini API → Gateway, and show in the status bar which one answered.

---

## 5. Jev (TypeSafe AI "System One"), as a cheap verifier and router

**What it is** (confirmed by the owner: "JEV" = `typesafe-ai/jev` on the
Gateway):

- A **decision-only** model. You send a text `state` and typed `questions`. It
  returns typed answers with probabilities: **Noul/Boolean** (`P(true)`),
  **Choice** (one option, with a confidence and a distribution) and **Score**
  (against a rubric).
- It does not generate prose. Several independent questions are answered in
  one parallel pass ([Vercel: What is Jev](https://vercel.com/i/what-is-jev),
  2026-09-19, updated 2026-10-07;
  [InfoQ](https://www.infoq.com/news/2026/10/typesafe-ai-jev-released/),
  2026-10-01).
- Endpoints:
  - `POST https://ai-gateway.vercel.sh/v1/evaluate` (Decision API, `boolean`
    type).
  - `POST https://ai-gateway.vercel.sh/typesafe/v1/systemone` (TypeSafe shape,
    `noul` type).
  - AI SDK `experimental_decide` (formerly `experimental_evaluate`)
    ([Vercel changelog](https://vercel.com/changelog/ai-gateway-now-supports-typesafe-clients-and-http-api-for-jev),
    2026-09-21; [TypeSafe API docs](https://vercel.com/docs/ai-gateway/sdks-and-apis/typesafe),
    2026-10-05).
- Type "evaluation", **text-only input** (images need a text description
  first), **32K context**, **$0.042 per M input tokens, output free**, providers
  typesafe-ai and digitalocean ([model page](https://vercel.com/ai-gateway/models/jev),
  accessed 2026-10-09).
- **Decision fallbacks:** in one request, rerun the whole decision on another
  model when it is unsure.
  - Conditions: `confidenceBelow` for Choice/Score, `probabilityBetween` for
    Boolean/Noul, and `any`/`all`/`atLeast` to combine them.
  - The two stages run **one after the other**, so a triggered request pays
    both latencies and both costs. Headers `x-ai-gateway-decision-fallback-*`
    say when a fallback ran ([decision fallbacks](https://vercel.com/docs/ai-gateway/models-and-providers/decision-fallbacks),
    2026-10-08).

**Latency:**

| Figure | Source | Label |
| --- | --- | --- |
| 70–500 ms end to end, "~100 ms typical" (measured from the US West Coast) | TypeSafe via [InfoQ](https://www.infoq.com/news/2026/10/typesafe-ai-jev-released/) and [jev-kit](https://github.com/FlorianRiquelme/jev-kit) | [Vendor] |
| Median 76 ms, upper quartile 270 ms (from 12,759 launch tweets) | OpenChamber via InfoQ | [Indep., self-reported by users] |
| 504 / 315 / 272 ms (3 calls, EU server); bench p50 **809 ms**, p95 **1,172 ms** (15 fixtures, ~955 input tokens per call, 2026-09-20) | [jev-kit](https://github.com/FlorianRiquelme/jev-kit) | [Indep.] |
| "sub-15 ms" claims on blogs | various | **Discount**: they contradict TypeSafe's own range and have no primary source |
| **Kai** | free-tier key refused with **403** on 2026-10-09 | [Kai]: **not measured** |

**Accuracy** [Indep., small n]:

- jev-kit on a PR-triage task: 81.7% overall. One boolean field ("needs
  human") was only **46.7%**.
- Answers with confidence ≥ 0.7 were 100% correct, covering 60% of cases
  ([jev-kit](https://github.com/FlorianRiquelme/jev-kit)).
- TypeSafe's own docs flag weak counting, arithmetic, date comparison, and
  accuracy loss on **large, noisy state** ([InfoQ](https://www.infoq.com/news/2026/10/typesafe-ai-jev-released/)).
  Raw accessibility trees are large, noisy state.

**Cost for Kai:** a 3,000-token tree diff + expect + task costs about
3,000 × $0.042/M ≈ **$0.00013 per call**. Cost is negligible; latency and
accuracy are the questions.

**How Kai could use it** (one request, several questions, state = task + last
action + `expect` + a **compact tree diff**, not the full tree):

| Question | Type | Use | Guard |
| --- | --- | --- | --- |
| "Did the last action's expected result happen?" | noul | Replace the screenshot on the next step when P > 0.9 and the tree is rich → tree-only step | Below the threshold, take the screenshot as today |
| "Is the task complete?" | noul | Early `done` suggestion, or stop asking for more steps after a batch | Never end a run on Jev alone; the model or the task check confirms |
| "Is the next proposed action risky?" | choice none/low/high | Can only **raise** risk (adds to Kai's deterministic classifier, never lowers it) | Password fields and destructive shell stay high no matter what |
| "Does the next step need the screenshot?" | noul | Tree-only routing | Force a screenshot every K steps |

**Placement:** run Jev **in parallel** with the next observe. Its ~0.3–0.8 s
then overlaps with Kai's ~200 ms observe plus the Gemma call, so it can skip
*future* screenshots and calls without adding latency to the current one. A
sequential Jev call on every step would add 0.3–0.8 s and lose.

**Verdict:**

- Promising as an **advisory** verifier and router, built behind the
  `Verifier` port with an LLM-verifier fallback (for example
  `gemini-2.5-flash-lite` or `gpt-5-nano` via the Gateway, which the free tier
  allows but at 0.8–2 s).
- It must not decide approvals.
- It needs paid Gateway credits before it can be measured. Adopt it only if a
  bench run shows fewer screenshots or calls at equal success.

---

## 6. Ranked changes for Kai

Ranked by expected speed gain, then accuracy risk. "Expected" effects are
estimates from the evidence above, **not Kai measurements**. Each must be
confirmed with `kai bench` on the Droplet.

"In progress" means another agent's worktree at commit `ee0a7a2` was editing
matching files when this was written (2026-10-09). The coordinator confirms
what actually landed.

| # | Change | Expected speed effect | Expected accuracy effect | Evidence | This session |
| --- | --- | --- | --- | --- | --- |
| 1 | **Multi-action steps** (`then` ≤ 3 follow-ups, each re-checked against the tree, batch stops on surprise or any non-low risk; one screenshot after) | **−20% to −50% model calls** on typing/form/search tasks | Neutral to slightly positive if guarded (UFO2: same or better success) | OSWorld-Human (Calc 13.6→5.9 steps), UFO2 (−44–51% steps), OpenAI/Anthropic/Gemini all batch | **In progress** (`reply.rs`: `then`, `need_screen`, `MAX_THEN = 3`) |
| 2 | **Accessibility-first actions** (UIA Invoke/Value/Toggle/ExpandCollapse; AT-SPI Action/EditableText; mouse as fallback) | Removes pointer travel, paste and settle waits; enables tree-only verification | **Positive** on forms and menus (UFO2 API: o1 16.3→24.5%, steps 16.0→6.6) | §3.3 | **Seam in place** (`Actuator::a11y_act`); OS implementation status: check with coordinator |
| 3 | **Streaming decode + early JSON parse + TTFT/decode split in traces** | Small direct gain (act when the object closes, cancel trailing tokens); large **diagnostic** value (tells prefill from decode) | None | §1.5 | **In progress** (`client/stream.rs`) |
| 4 | **Tree-only steps** (rich tree + checkable `expect` + no `need_screen`; forced screenshot every K steps) | Saves JPEG encode (41–52 ms [Kai]), ~128 KB upload, vision encoder, ~264 prompt tokens per skipped frame | Small risk where trees are partial; guard with fallbacks | §3.4, Hermes PR (47 ms vs 248–478 ms) | **Seam in place** (`observe_tree_only`); `need_screen` in progress |
| 5 | **Frame sizing to the Gemma processor** (resize once to ~1056×576 at 280; draw marks at that size; 560 for dense screens or retries) | Fewer pixels to encode and upload (~43% fewer than 1280×720); no double resample | Positive (sharper mark digits); 560 helps dense text (InfographicVQA 82.8→92.0 at 1120) | §1.2 | Not seen in worktrees: candidate |
| 6 | **Plan/skill cache (auto-skills)**: strict match, replay the deterministic prefix, then a fresh full observation | **Zero model calls** for the cached prefix | Positive when matching is strict (AWM +24.6% / +51.1% relative) | §3.8 | **In progress** (`plan_cache.rs`) |
| 7 | **Gemini fallback tuning**: `thinkingLevel: "minimal"` (documented "off" for Gemma); try `responseJsonSchema` with JSON-mode fallback | Fewer thinking tokens; fewer repair retries | Positive (parse 50%→99% reported for 31B, anecdotal) | §1.3, §1.4 | One-line candidates |
| 8 | **Zoomed crop after a missed click** (grid mode or no screen change) | One extra call, only on misses; saves the failure → 31B → retry loop | **Positive** on small targets (ScreenSeekeR 18.9→48.1% on ScreenSpot-Pro) | §3.6 | **Seam in place** (`Perceptor::zoom`) |
| 9 | **OpenAI-compatible backend via the Gateway** (GPT comparison in the same harness; optional Gemma third fallback) | Neutral for Kai's own speed (adds ~0.16–0.2 s to models routed there) | n/a (comparison) | §4 | **In progress** (`client/openai.rs`) |
| 10 | **Jev verifier** (parallel, advisory: expect held / done / need screen / raise-only risk), with an LLM-verifier fallback | Positive only if run concurrently and it lets steps skip screenshots or calls | Neutral to positive if raise-only and thresholded; **unmeasured** (403 on free tier) | §5 | **In progress** (`client/jev.rs`, `client/llm_verifier.rs`) |
| 11 | **vLLM tuning on the Droplet**: keep prefix caching and async scheduling; small `max-num-seqs` (≥ 4 for sub-agents); `max_soft_tokens` set on purpose; try **MTP** (31B first; 26B-A4B likely small at batch 1); FP8 KV only if memory is short | MTP: Google says up to 3× (vendor); independent +13% for 26B-A4B on Apple Silicon | Lossless by design | §1.5 | Needs the Droplet; not run here |
| 12 | **Element-list trimming** (shorter labels, drop non-interactive/off-screen, cap count) | Fewer prompt tokens (the list is ~2–3× the image's tokens) | Neutral if interactive elements are kept | §1.2 | Partly in progress (`prompt.rs`/`element.rs` edits) |
| 13 | **Screen-diff crops** (send only the changed region after a small, expected change) | Fewer image tokens on small updates | Risk of losing context; weaker than tree-only steps | §3.4 | Later |
| 14 | **Planner/executor split** | Pays off only with a cheap executor (cached plan or deterministic code); otherwise adds calls | Can help long tasks | §3.9, OSWorld-Human | Later (plan cache covers the cheap half) |
| 15 | **Speculative / pre-authorised actions** (guarded side-effect-free follow-ups, AAPT-style) | Removes decode from the critical path for transient UI | Research-grade | §3.9 | Later |

**Biggest likely wins, in one line each:**

1. Cut steps: multi-action, accessibility-first, plan cache.
2. Cut tokens per step: tree-only steps, right-sized frames, a trimmed list.
3. Measure TTFT vs decode on the real Droplet before tuning vLLM further.

Everything else is second order until those numbers exist.
