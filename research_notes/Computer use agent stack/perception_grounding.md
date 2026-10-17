# Perception and grounding for computer-use agents (as of 2026-10-17)

Scope: which perception/grounding approach (screenshot coordinates, accessibility tree, Set-of-Marks, hybrid) gives the best accuracy and speed for open-weight models driving desktop software on macOS and Linux. Every number below carries its source and the source's date. Aggregator leaderboards (llm-stats, BenchLM, LLM Learner) are flagged as such: they mix self-reported scores and harnesses, and several of their model names could not be checked against a primary source in this pass.

## 1. Screenshot-only vs accessibility tree vs Set-of-Marks vs hybrid

### Takeaway
The controlled ablations found (OS-Harm 2025, EntWorld 2026) put **a11y tree + screenshot (hybrid)** at or near the top and **raw screenshot-only coordinates at the bottom for general models that lack GUI training**. SoM is competitive for some models. The leading agents of 2026 are still screenshot-only, because GUI-specialised models (OpenCUA, Holo2, UI-TARS, MAI-UI) trained on pixel grounding close that gap. For a general model such as Gemma, hybrid or SoM is the safer default. Pixel-only works well only with a model that has GUI grounding training.

### Cited Findings
- OS-Harm (arXiv 2506.14866, Jun 2025) compared the four OSWorld observation types on OSWorld's small test set. Gemini 2.5 Pro scored **25.6% hybrid (a11y + screenshot), 21.1% a11y tree only, 5.3% screenshot only**. For o4-mini, **SoM scored 18.9%** and **hybrid 18.4%** (the two best observation types for that model). — [OS-Harm](https://arxiv.org/pdf/2506.14866)
- EntWorld (arXiv 2601.17722, Jan 2026), on enterprise GUI tasks: the combination was strongest, screenshot-only was worst, and tree-only beat screenshot-only. Tree-only was weak on legacy enterprise apps with incomplete accessibility metadata, and the tree's token cost was flagged. — [EntWorld](https://arxiv.org/pdf/2601.17722)
- OSWorld-Human (arXiv 2506.16042, Jun 2025): adding the a11y tree to screenshots **raised step counts for most apps** (sharply in LibreOffice) but lowered them for GIMP and Chrome. In other words, the tree has an efficiency cost that depends on the app. — [OSWorld-Human](https://arxiv.org/pdf/2506.16042)
- RedTeamCUA (arXiv 2505.21936, May 2025): adding the a11y tree to screenshots **lowered attack success rates** (prompt injection). Benign task gains were mixed, and the authors note that trees are not always available. — [RedTeamCUA](https://arxiv.org/pdf/2505.21936)
- WindowsWorld (arXiv 2604.27776, Apr 2026) tested vision-only settings on purpose and kept a hybrid condition to measure what structural cues are worth to open-source models. — [WindowsWorld](https://arxiv.org/pdf/2604.27776)
- The official OSWorld site (osworld-v1.xlang.ai) filters verified results by **Screenshot, A11y tree, Screenshot + A11y tree, Set-of-Mark**. It also defines General model / Specialized model / Agentic framework entry types and says **OSWorld 2.0 is now available**, so v1-Verified and 2.0 numbers may not be comparable. The live table did not render to the fetch tool. — [OSWorld](http://osworld-v1.xlang.ai/)
- OpenCUA's OSWorld-Verified runs use `--observation_type screenshot` with 3 images of history, so the open SOTA of 2025 was screenshot-only. — [OpenCUA-32B card](https://huggingface.co/xlangai/OpenCUA-32B)
- The original OSWorld paper's best model reached 12.24% (2024). — [OSWorld](http://osworld-v1.xlang.ai/)

### Inferences
- For Kai (Gemma 4 by default, AT-SPI / UI Automation already in `kai-os`), the evidence supports keeping **hybrid perception (a11y-derived marks + screenshot)** as the default. Pure pixel coordinates should be a switchable path, used when a GUI-trained grounder is available.
- The a11y tree also doubles as a prompt-injection defence (RedTeamCUA), which fits Kai's "page text is data" guarantees.
- Tree cost: OSWorld-Human's step-count increase and EntWorld's token warning both argue for a **compressed/filtered tree** (interactive elements in the viewport only) rather than a raw dump.

### Gaps
- No primary source in this pass gives per-observation-type OSWorld-Verified numbers for 2026 open models, because the live leaderboard table did not load.
- A secondary blog claimed that a compressed a11y tree gave +5.1 points on OSWorld at 22% of the tokens. No primary paper was found, so it is not used.
- No macOS-specific ablation (AX API tree vs screenshot) was found. macOSWorld numbers were not retrieved.

## 2. Grounding models: scores and sizes

### Takeaway
As of 2026, open GUI grounders built on Qwen3-VL lead ScreenSpot-Pro: **MAI-UI 73.5% (Dec 2025, zoom-in), UI-Venus-1.5 69.6% (Feb 2026), Holo2-30B-A3B 66.1%**. Small models are strong too: **Holo2-4B scores 57.2% at 4B**. Base Qwen3-VL scores 38–55% depending on size and variant. Older models are far behind: UI-TARS-1.5-7B 39–61% (sources disagree), Qwen2.5-VL-7B about 29%. **No published GUI grounding score for Gemma 4 exists.**

### Cited Findings
**Holo2 (H Company; base Qwen3-VL-30B-A3B-Thinking; card fetched Oct 2026, release date not on the card)**, UI localisation table:
| Model | ScreenSpot-Pro | OSWorld-G | Showdown | GroundUI-1K | WebClick | ScreenSpot-v2 |
|---|---|---|---|---|---|---|
| Holo2-30B-A3B | 66.1 | 76.1 | 77.6 | 85.4 | 91.3 | 94.9 |
| Holo2-8B | 58.9 | 70.1 | 72.5 | 83.8 | 89.5 | 93.2 |
| Holo2-4B | 57.2 | 69.4 | 74.7 | 83.3 | 88.8 | 93.2 |
| Holo1.5-72B | 63.3 | 71.8 | 76.8 | 84.5 | 92.4 | 94.4 |
| Holo1.5-7B | 57.9 | 66.2 | 72.1 | 84.0 | 90.2 | 93.3 |
| Holo1.5-3B | 51.4 | 61.5 | 67.5 | 83.2 | 81.4 | 91.6 |
| Qwen3-VL-30B-A3B-Thinking | 49.9 | 65.8 | 71.2 | 84.2 | 89.5 | 91.8 |
| Qwen3-VL-8B-Thinking | 38.5 | 56.0 | 64.2 | 83.6 | 85.9 | 91.5 |
| Qwen3-VL-4B-Thinking | 41.4 | 56.4 | 66.6 | 84.1 | 85.8 | 90.0 |
| Qwen2.5-VL-72B | 55.6 | 62.0 | 41.0 | 85.4 | 88.3 | 93.3 |
| Qwen2.5-VL-7B | 29.0 | 40.6 | 52.0 | 80.7 | 76.5 | 85.6 |
| UI-TARS-1.5-7B | 39.0 | 61.0 | 58.0 | 84.0 | 86.1 | 94.0 |
| UI-Venus-72B | 61.9 | 70.4 | 75.6 | 75.5 | 77.0 | 95.3 |
| UI-Venus-7B | 50.8 | 58.8 | 67.3 | 82.3 | 84.4 | 94.1 |
— [Holo2-30B-A3B card](https://huggingface.co/Hcompany/Holo2-30B-A3B). The license is contradictory on the card: it says "research-only (non-commercial)", the tag reads cc-by-nc-4.0, and the text also says "Apache 2.0". Check before use.

- **OpenCUA (xlangai, paper Aug 12 2025; vLLM support Jan 17 2026)**, grounding: OpenCUA-7B ScreenSpot-Pro 50.0 / OSWorld-G 55.3 / ScreenSpot-v2 92.3 / UI-Vision 29.7. OpenCUA-32B 55.3 / 59.6 / 93.4 / 33.3. OpenCUA-72B 60.8 / 59.2 / 92.9 / 37.3. On the same card, UI-TARS-72B scores 38.1 SS-Pro, Qwen2.5-VL-32B 39.4, and Qwen2.5-VL-7B 27.6. — [OpenCUA-32B card](https://huggingface.co/xlangai/OpenCUA-32B)
- **MAI-UI (arXiv 2512.22047, Dec 26 2025; sizes 2B, 8B, 32B, 235B-A22B)**: ScreenSpot-Pro **73.5%**, MMBench-GUI L2 91.3%, OSWorld-G 70.9%, UI-Vision 49.2%, AndroidWorld 76.7%, MobileWorld 41.7% (the best variant). — [MAI-UI](https://arxiv.org/abs/2512.22047). Papers with Code labels the 73.5 entry "MAI-UI-32B (zoom-in)". — [PwC](https://paperswithcode.co/benchmark/screenspot-pro?eval=5370)
- **UI-Venus-1.5 (arXiv 2602.09082, Feb 9 2026, rev. Feb 24; 2B, 8B, 30B-A3B)**: ScreenSpot-Pro **69.6%**, VenusBench-GD 75.0%, AndroidWorld 77.6%. — [UI-Venus-1.5](https://arxiv.org/abs/2602.09082)
- **Qwen3-VL bases (third-party tables)**: POINTS-GUI-G (arXiv 2602.06391, Feb 2026) lists Qwen3-VL-8B at **49.9** and Qwen3-VL-32B at **54.9** on ScreenSpot-Pro, with its own 8B at 59.9. — [POINTS-GUI-G](https://arxiv.org/pdf/2602.06391). This conflicts with Holo2's 38.5 for Qwen3-VL-8B-*Thinking*. The variant (Instruct vs Thinking) and the harness differ.
- **GroundGUI (arXiv 2610.19100, Oct 2026 preprint)**: 8B model at **63.9%** ScreenSpot-Pro. — [alphaXiv](https://www.alphaxiv.org/abs/2610.19100)
- **Agentic/zoom grounding on ScreenSpot-Pro (Papers with Code)**: Holo2-235B-A22B (agentic) **78.5**, Holo2-30B-A3B (agentic) 75.2, MAI-UI-32B (zoom-in) 73.5. — [PwC](https://paperswithcode.co/benchmark/screenspot-pro?eval=5370)
- **UI-TARS-1.5**: an aggregator reports a ScreenSpot-Pro overall of 61.6% (probably the closed/large variant, not the 7B). — [Codesota](https://www.codesota.com/model/ui-tars-1-5). UI-TARS-1.5-7B scores 39.0 per [Holo2 card](https://huggingface.co/Hcompany/Holo2-30B-A3B).
- **GTA1 (arXiv 2507.05791, v1 Jul 8 2025, v5 Oct 3 2025)**: RL-trained grounder with click-success reward. At test time it samples several action proposals per step and a judge model picks one. The abstract claims SOTA but gives no numbers. — [GTA1](https://arxiv.org/abs/2507.05791)
- **ScreenSpot-Pro baseline**: the best model in the Jan/Apr 2025 paper scored 18.9%. ScreenSeekeR visual search reached 48.1% without training. SE-GUI scored 47.2% (7B) and 35.9% (3B). — [ScreenSpot-Pro repo](https://github.com/likaixin2000/ScreenSpot-Pro-GUI-Grounding), [paper](https://arxiv.org/abs/2504.07981v1). The official grounding leaderboard page (updated Sep 10 2026) lists the apps but did not render model rows. — [leaderboard](https://gui-agent.github.io/grounding-leaderboard/)
- **Gemma 4 (released Apr 2 2026; E2B, E4B, 12B Unified, 26B A4B MoE, 31B dense)**: Hugging Face showed GUI element detection returning native JSON `box_2d` with coordinates "**relative to a 1000x1000**" image, with no grammar constraints. **No GUI benchmark (ScreenSpot etc.) was reported.** Related scores: MMMU-Pro 76.9% (31B) / 73.8% (26B A4B); Tau2 76.9% (31B) / 68.2% (26B A4B). — [HF Gemma 4 blog](https://huggingface.co/blog/gemma4). LM Studio lists "screen and UI understanding" and "pointing" as capabilities. — [LM Studio](https://lmstudio.ai/models/gemma-4)
- **ScreenSpot-v2** has 1,273 tasks (web/desktop/mobile) and fixes about 11.32% of the original ScreenSpot. A click counts as correct if it lands inside the GT box. — [arXiv 2603.23559](https://arxiv.org/pdf/2603.23559)
- Aggregator claims that could not be verified: llm-stats lists "GPT-6 Astra" at 0.927 on ScreenSpot-Pro (Oct 2026). — [llm-stats](https://llm-stats.com/benchmarks/screenspot-pro). Benchmarklist reports "Claude Mythos Preview" at 93% with tools / 79.3% without. — [benchmarklist](https://benchmarklist.com/benchmarks/screenspot_pro/)

### Inferences
- For a local grounder on a single consumer GPU, **Holo2-4B/8B (about 57–59 SS-Pro)** and the **UI-Venus-1.5 8B / MAI-UI 8B** class are the best size-to-accuracy points found. All are Qwen3-VL derivatives, so they share Qwen's coordinate conventions.
- ScreenSpot-v2 is saturated (91–95% for nearly everything). Use **ScreenSpot-Pro, OSWorld-G and UI-Vision** to discriminate.
- Gemma 4's pointing is undocumented on benchmarks. Kai should measure it itself, for example a ScreenSpot-Pro / OSWorld-G subset through `kai bench` or a small eval harness, before deciding between "Gemma alone" and "Gemma planner + specialised grounder".

### Gaps
- Not retrieved from primary sources this pass: UI-TARS-2, Jedi-7B, UGround-V1, OS-Atlas, Aguvis, OmniParser v2, Fara-7B, GLM-4.5V, InternVL3.5, Phi-4-multimodal. From memory (unverified, do not cite without checking): Jedi-7B was about 39.5 SS-Pro, UGround-V1-7B about 31, OS-Atlas-7B about 18.9, OmniParser v2 + GPT-4o about 39.6 SS-Pro, GTA1-7B about 50 and GTA1-72B about 58 SS-Pro.
- Gemma 3 or Gemma 4 ScreenSpot numbers: none found anywhere.
- The `box_2d` axis order for Gemma 4 (likely [y0,x0,y1,x1] as in Gemini/PaliGemma) is not stated in the blog. Verify empirically.

## 3. OSWorld-Verified: top open-weight vs closed (with dates)

### Takeaway
Primary sources found for open models give **OpenCUA-72B 45.0% (100 steps, 2025)** and **Holo2-8B 39.9% / Holo2-30B-A3B 37.4% / Qwen3-VL-30B-A3B-Thinking 36.6% OSWorld** (2026 card, harness unspecified). Aggregators dated Sep–Oct 2026 claim much higher open entries (80%+) with model names that could not be confirmed here. Treat those as unverified until checked against os-world / the model's own card.

### Cited Findings
- OpenCUA card, OSWorld-Verified success rate (mean of 3 runs) at 15/50/100 steps: OpenCUA-7B 24.3/27.9/26.6. OpenCUA-32B 29.7/34.1/34.8. **OpenCUA-72B 39.0/44.9/45.0**. Comparisons: OpenAI CUA 26.0/31.3/31.4, Claude 3.7 Sonnet 27.1/35.8/35.9, Claude 4 Sonnet 31.2/43.9/41.5, UI-TARS-1.5-7B 24.5/27.3/27.4, UI-TARS-72B-DPO 24.0/25.8/27.1, **Qwen2.5-VL-72B 4.4/–/5.0**, Qwen2.5-VL-32B 3.0/–/3.9, Kimi-VL-A3B 9.7/–/10.3. — [OpenCUA-32B card](https://huggingface.co/xlangai/OpenCUA-32B)
- Holo2 navigation table: OSWorld Holo2-8B **39.9%**, Holo2-4B 37.7%, Holo2-30B-A3B 37.4%, Qwen3-VL-30B-A3B-Thinking 36.6%, Qwen3-VL-8B-Thinking 28.8%, Qwen3-VL-4B-Thinking 24.1%, Holo1.5-7B 6.4%. Also WebVoyager 83.0 / WebArena 46.3 / AndroidWorld 71.6 for Holo2-30B-A3B. The card says bolding is provisional "once final evaluations are available". — [Holo2 card](https://huggingface.co/Hcompany/Holo2-30B-A3B)
- UI-TARS-1.5-7B: 27.5% OSWorld at 100 steps (secondary). — [lilting.ch](https://lilting.ch/articles/ui-tars-gui-agent). UI-TARS-1.5 (large) 42.5%. — [Codesota](https://www.codesota.com/model/ui-tars-1-5)
- Aggregators (unverified, conflicting): BenchLM (Oct 13 2026) lists "Qwen3.8 Max" 86.1% (labelled open), "Qwen3.8-27B" 84.3%, "Holo3-35B-A3B" 82.6%, "MiMo-V2.6-Pro" 82%, and closed "Claude Fable 5" / "Claude Mythos 5" at 85%. — [BenchLM](https://benchlm.ai/benchmarks/osworld-verified). LLM Learner (Sep 2026) lists "Kimi K3" at 84.8% as the top open entry. — [LLM Learner](https://llmlearner.com/rankings/osworld-verified). llm-stats lists Qwen3.8-27B at 0.843 as the top open entry. — [llm-stats](https://llm-stats.com/benchmarks/osworld-verified). Vellum's page (Kimi K2.6 73.1%, MiniMax M3 70.1%) looks stale. — [Vellum](https://www.vellum.ai/open-llm-leaderboard)

### Inferences
- Between 2025 and 2026, open models moved from about 5% (raw Qwen2.5-VL-72B) to about 37–45% (GUI-trained 4B–72B) on OSWorld-Verified by primary sources. If the aggregator figures are right, 2026 frontier open models are near the human baseline (about 72%), but those entries are large models, not laptop-local.
- **Training matters more than size here.** Holo2-4B (37.7%) roughly equals Qwen3-VL-30B-A3B-Thinking (36.6%), and raw Qwen2.5-VL-72B scored 5%.

### Gaps
- The official OSWorld-Verified table did not render (JS-loaded), so these aggregator numbers could not be cross-checked. "Qwen3.8", "Holo3", "Kimi K3", "Claude Fable 5" and similar names need primary confirmation.
- Agent frameworks behind the top entries (Agent S3, CoAct-1, GTA1 + o3 planner, etc.) were not confirmed in this pass.
- WindowsAgentArena, macOSWorld, AndroidWorld beyond the model cards above, WebArena/WebVoyager beyond Holo2, and UI-Vision beyond OpenCUA/MAI-UI were not covered.

## 4. Coordinate conventions, resolution, Retina

### Takeaway
Three conventions coexist: **absolute pixels on a smart-resized image** (Qwen2.5-VL and its derivatives such as OpenCUA and UI-TARS), **0–1000 normalised** (Qwen3-VL and its derivatives; Gemma 4 `box_2d`), and native-screen pixels (some APIs). A wrong convention or a missed resize silently mis-clicks. On Retina/HiDPI, map model coordinates → captured image pixels → logical points.

### Cited Findings
- OpenCUA outputs absolute coordinates **on the smart-resized image, not the original**. Conversion uses `smart_resize(factor=28, min_pixels=3136, max_pixels=12845056)` and then scales to the original W/H. Example: `pyautogui.click(x=960, y=324)`. — [OpenCUA-32B card](https://huggingface.co/xlangai/OpenCUA-32B)
- Qwen3-VL changed the default from Qwen2.5-VL's absolute coordinates to **relative 0–1000**: divide by 1000 and multiply by the image W/H, with no need to compute the resized dims. — [CSDN blog](https://adg.csdn.net/694cf2fa5b9f5f31781a9f5b.html), [DeepWiki Qwen3-VL grounding](https://deepwiki.com/QwenLM/Qwen3-VL/5.2-spatial-understanding-and-2d-grounding). Some benchmarks treat it as a 0–999 grid (divide by 999). — [CutVerse](https://arxiv.org/pdf/2605.19484)
- Gemma 4: box coordinates refer to **1000x1000 relative to input dims**. Its vision encoder keeps aspect ratio with selectable budgets of **70/140/280/560/1120 image tokens**. — [HF Gemma 4 blog](https://huggingface.co/blog/gemma4)
- Benchmarks must normalise across [0,1000], absolute and native-pixel conventions before scoring. — [MedSPOT](https://arxiv.org/pdf/2603.19993)
- ScreenSpot-Pro exists because high-resolution professional screens with tiny targets break grounders (best 18.9% at release). Visual search / zoom-in (ScreenSeekeR 48.1%, MAI-UI zoom-in 73.5%, Holo2 agentic 75–78.5) recovers a lot. — [ScreenSpot-Pro](https://arxiv.org/abs/2504.07981v1), [PwC](https://paperswithcode.co/benchmark/screenspot-pro?eval=5370)

### Inferences
- Kai should keep the coordinate convention **per model in config** (absolute-on-resized with a smart_resize factor, or normalised with a 1000/999 divisor), not hard-code it, and test the round trip.
- Retina (macOS) and HiDPI (Linux) need one rule for how screenshot pixels map to input-event coordinates. CGEvent uses points while capture is in pixels (backing scale 2.0). This mapping comes from the platform convention, not from a source fetched here, so verify against `docs/MACOS.md` and Apple docs.
- On large monitors, a **crop/zoom second pass** (coarse locate → crop → re-ground) is the cheapest measured accuracy gain for small grounders.

### Gaps
- Qwen3-VL patch/merge size (believed to be 16 px patches merged 2x2 → 32 px per token) was not confirmed from the primary paper (arXiv 2511.21631).
- No quantified study of the Retina scaling error rate was found.

## 5. Latency: tokens per step, image cost, planner + grounder split

### Takeaway
Image tokens dominate the per-step cost. Gemma 4 lets you pick 70–1120 tokens per image. Qwen-family models scale tokens with pixels (smart_resize up to about 12.8 MP on OpenCUA). Planner + grounder splits (GTA1-style, Holo agentic, ScreenSeekeR) buy accuracy with extra calls. Single GUI-trained models (OpenCUA, Holo2, UI-Venus) do one call per step and are the latency-efficient choice.

### Cited Findings
- Gemma 4 image budgets: 70, 140, 280, 560, 1120 tokens, trading speed and memory for quality. — [HF Gemma 4 blog](https://huggingface.co/blog/gemma4)
- OpenCUA uses 3 screenshots of history per step, smart_resize max_pixels 12,845,056. — [OpenCUA-32B card](https://huggingface.co/xlangai/OpenCUA-32B)
- More steps do not monotonically help: OpenCUA-7B scored 27.9% at 50 steps vs 26.6% at 100, and Claude 4 Sonnet 43.9% vs 41.5%. — [OpenCUA-32B card](https://huggingface.co/xlangai/OpenCUA-32B)
- GTA1 test-time scaling samples several candidate actions concurrently per step and a judge picks one, trading compute for accuracy. — [GTA1](https://arxiv.org/abs/2507.05791)
- "Adaptive VLM routing for computer use agents" (arXiv 2603.12823, Mar 2026) studies routing steps between models to save cost. Its content was not retrieved. — [arXiv 2603.12823](https://arxiv.org/pdf/2603.12823)
- Adding an a11y tree increases steps for most OSWorld apps, and EntWorld flags its token cost. — [OSWorld-Human](https://arxiv.org/pdf/2506.16042), [EntWorld](https://arxiv.org/pdf/2601.17722)

### Inferences
- For Kai on local hardware: one screenshot at a low or medium Gemma token budget plus a filtered a11y/SoM list per step is likely the best latency/accuracy point. Escalate to a 1120-token or cropped re-ground only when a click target is ambiguous. This is an A/B for `kai bench`, not a measured fact.
- A planner (Gemma 4 26B A4B) + small grounder (Holo2-4B / UI-Venus-1.5-2B/8B) split is the most promising open recipe on paper. It adds one model call per click and needs a live A/B against hybrid a11y + Gemma alone.

### Gaps
- No primary source found with measured per-step latency (seconds) or tokens per step for open CUAs on consumer hardware.
- Image-token counts per screenshot for Qwen3-VL at 1920x1080 / 2880x1800 were not confirmed from a primary source.
