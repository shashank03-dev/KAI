# Local open-weight VLM inference for computer-use agents (Apple Silicon first, Linux second) and model-agnostic action protocols, as of October 2026

Research date: 2026-10-17. All sources were accessed on that date unless noted otherwise. Kai's own pinned runtime decision from 2026-10-10 is in `docs/LOCAL_MODELS.md` in this repo.

## Runtimes: model support, speed, structured output, caching, embeddability

### Takeaway
On a Mac today, llama.cpp `llama-server` (Metal) is the most compatible single runtime. It has vision through `--mmproj`, an OpenAI-compatible API, `/v1/responses`, Anthropic `/v1/messages`, `json_schema`, grammars, `--jinja` tool calling, prompt caching and slot save/restore. MLX (mlx-vlm, or the vllm-mlx server) is modestly faster at decode for small VLMs, by roughly 5 to 25% in vendor tests. MLX's lead grows on MoE text models, and Apple's M5 Neural Accelerators favor MLX for prefill. I found no independent, dated benchmark of image prefill latency for grounding models (UI-TARS, Holo, Fara) on any M-series chip.

### Cited Findings
**llama.cpp (llama-server, mtmd)**
- Kai pins llama.cpp release b11429 (2026-10-05). Its `docs/multimodal.md` lists Gemma 4 E2B, E4B, 26B-A4B and 31B GGUF with mmproj as supported. The release ships a macOS arm64 Metal build (`llama-b11429-bin-macos-arm64.tar.gz`, 11,971,406 B), plus Linux and Windows CPU, Vulkan and CUDA builds. GGUF repos exist for Qwen3-VL-8B/4B (official Qwen) and UI-TARS-1.5-7B (mradermacher). See repo `docs/LOCAL_MODELS.md` and `docs/MACOS.md`. Original sources: [llama.cpp releases](https://github.com/ggml-org/llama.cpp/releases) and [multimodal.md @ b11429](https://raw.githubusercontent.com/ggml-org/llama.cpp/b11429/docs/multimodal.md).
- Server features on master as of 2026-10:
  - Tool calling: OpenAI-style function calling with `--jinja`. Some models' native tool-call formats are supported, with a generic fallback. `--chat-template-file` overrides the template.
  - Structured output: `response_format` supports `json_object` and `json_schema`. GBNF grammars work per request (`grammar`, `json_schema`) and server-wide (`--grammar`, `--json-schema`).
  - Images: from URL, base64 or `file://` (with `--media-path`). Multimodal is still marked "experimental".
  - Caching: `cache_prompt` (shared-prefix KV reuse) is on by default and the docs warn it is not bit-exact. `--cache-reuse N` enables chunk reuse through KV shifting. Slot save/restore is available through `--slot-save-path`.
  - Batching: continuous batching with `-np` parallel slots.
  - Endpoints: `/v1/responses` (converted internally to chat completions) and Anthropic `/v1/messages` with SSE plus `count_tokens`.

  Source: [llama-server README (master)](https://raw.githubusercontent.com/ggml-org/llama.cpp/master/tools/server/README.md).

**MLX family (mlx-vlm, mlx-lm, vllm-mlx, LM Studio mlx-engine, Ollama MLX runner)**
- mlx-vlm has an OpenAI-compatible server (`mlx_vlm.server`) with streaming. A third-party doc mirror lists Qwen3-VL, Qwen3.5 and Qwen3-Omni as supported, with a thinking budget. I could not reach the official repo or PyPI page to confirm the version or date. Sources: [mlx-vlm docs mirror (Mintlify)](https://www.mintlify.com/yocxy2/mlx-vlm/introduction) and [mlx-vlm 0.4.4 package index](https://simple-repository.app.cern.ch/project/mlx-vlm). One secondary article lists Gemma 4 as supported: [pyshine](https://pyshine.com/MLX-VLM-Vision-Language-Models-Apple-Silicon/).
- vllm-mlx (waybarrios) is an "OpenAI and Anthropic compatible" server for Apple Silicon:
  - native MLX with continuous batching and multimodal models
  - JSON-Schema `response_format`
  - 19 tool-call parsers (OpenAI, Qwen, DeepSeek, Gemma and others)
  - an image prefix cache keyed on a SHA-256 of decoded pixels, which caches embeddings plus KV state with LRU eviction

  A v0.5.0 release (reportedly 2026-09-17) made structured output reject constraints it cannot enforce. Sources: [GitHub waybarrios/vllm-mlx](https://github.com/waybarrios/vllm-mlx), [project page](https://waybarrios.com/projects/vllm-mlx/) and [paper arXiv 2601.19139](https://arxiv.org/html/2601.19139v1). Its "28x vision latency" claim is unverified.
- LM Studio runs both llama.cpp and its own MIT-licensed `mlx-engine` (mlx-lm + Outlines + mlx-vlm), and exposes an OpenAI-compatible endpoint headlessly through `lms server`. mlx-engine 0.4.2 added continuous batching. Sources: [Dan MacKinlay notes](https://danmackinlay.name/notebook/local_llm_mac.html) and [yage.ai (2026-03-31)](https://yage.ai/share/mlx-apple-silicon-en-20260331.html). The LM Studio app itself is closed source, so Kai cannot bundle it (repo `docs/LOCAL_MODELS.md`).
- Ollama v0.19.0 (announced 2026-03-30, preview) routes by file format: GGUF goes to llama.cpp Metal and safetensors goes to MLX. The MLX runner launched with six architectures: Gemma 3, GLM-4 MoE Lite, Llama, Qwen 3, Qwen 3.5 and Qwen 3.5 MoE. Source: [yage.ai](https://yage.ai/share/mlx-apple-silicon-en-20260331.html).
- Secondary changelogs, not verified against the official notes:
  - Ollama v0.20.0 (April 2026) added Gemma 4.
  - v0.21.0 added MLX Gemma 4 mixed precision.
  - v0.23.1 added Gemma 4 MTP speculative decoding on Apple Silicon.

  Source: [fazm.ai Ollama notes](https://fazm.ai/t/ollama-release-notes-2026). [PromptQuorum](https://www.promptquorum.com/power-local-llm/ollama-vision-models-review) also claims a new multimodal engine around May 2026, and says Qwen 3.6 vision was not yet in Ollama.
- There is an Ollama wrapper overhead: raw llama.cpp Metal ran at 89.4 tok/s against 43.5 tok/s through Ollama on an M4 Max 128 GB (model not stated). Source: GitHub ollama #14861, via [yage.ai](https://yage.ai/share/mlx-apple-silicon-en-20260331.html).

**Other servers**
- vLLM is used for Fara-7B: `vllm serve microsoft/Fara-7B --dtype auto` (bf16), needing vllm ≥0.10.0 and transformers ≥4.53.3. Fara-7B was tested on A6000, A100 and H100. SGLang and Docker Model Runner are also documented. Source: [Fara-7B model card](https://huggingface.co/microsoft/Fara-7B).
- Holo2 uses vLLM: `vllm serve Hcompany/Holo2-4B`, bf16, max length 65,536, `--reasoning-parser deepseek_r1`, thinking on by default and switchable through chat-template args. The 30B-A3B model needs TP=2. Source: [H Company hub: deploy with vLLM](https://hub.hcompany.ai/models/Holo2/deploy-with-vLLM-on-NVIDIA-GPUs).

### Inferences
- For a Rust app, both servers can run as a sidecar on `127.0.0.1`. llama.cpp is the only option that is a single prebuilt binary for every OS with no Python. MLX options are Python, except LM Studio (closed) and Ollama (a system service). A Swift/MLX embed is possible on Mac (MLX Swift exists) but is not cross-platform.
- An MLX backend could be added behind a switch on macOS only, through vllm-mlx or mlx-vlm server. It would speak the same OpenAI API, so Kai's `OpenAiClient` would not need to change. The gain is measurable but small for 2 to 8B VLMs in the only head-to-head found (about 5% decode). It is largest on M5 (prefill) and on MoE models.
- llama.cpp's `/v1/responses` and `/v1/messages` mean a local model can be driven through OpenAI-Responses- or Anthropic-shaped clients. Whether `computer`-type tools are translated was not verified, and probably they are not.

### Gaps
- No independent, dated measurement of image prefill or TTFT for screenshots at 1280×800 or larger, for Qwen2.5-VL-7B, UI-TARS-1.5-7B, Holo2 or Fara-7B, on M1 through M5. Kai should measure this itself with `kai bench`.
- No Linux GPU numbers were found here (vLLM, SGLang or LMDeploy tok/s for these VLMs). LMDeploy and ExLlama vision support was not researched because of the tool-call budget.
- Two possibly relevant papers surfaced but were not read: [SiliconBench arXiv 2609.19169](https://arxiv.org/pdf/2609.19169) and [BaseRT arXiv 2607.00501](https://arxiv.org/pdf/2607.00501).
- mlx-vlm's official version and date for Gemma 4 support could not be confirmed.

## MLX vs llama.cpp Metal head-to-head (dated)

### Takeaway
On a small VLM (Qwen3-VL-2B 4-bit, M4 Max, March 2026), mlx-vlm and llama.cpp are nearly tied for vision decode (about 217 to 239 tok/s). mlx-vlm has 7 to 17% lower time-to-output up to 448×448. For text, MLX leads raw llama.cpp by roughly 1.4 to 1.8x on MoE models, but llama.cpp can win on prefill for short prompts and at very long context.

### Cited Findings
**Vision benchmark** (vendor RunAnywhere/MetalRT, 2026-03-13): M4 Max 64 GB, macOS 26.3, Qwen3-VL-2B-Instruct 4-bit, one COCO image, greedy decoding, 5 runs each, KV cleared between runs. Engine versions were not stated, and llama.cpp was measured through its server. Source: [RunAnywhere blog](https://www.runanywhere.ai/blog/metalrt-vlm-fastest-vision-language-model-apple-silicon).

| Image | Vision decode tok/s (MetalRT / mlx-vlm / llama.cpp) | Time-to-output ms (MetalRT / mlx-vlm / llama.cpp) |
|---|---|---|
| 224×224 | 285.4 / 239.1 / 231.0 | 92.4 / 118.5 / 141.9 |
| 448×448 | 279.3 / 226.7 / 226.8 | 171.8 / 204.1 / 219.5 |
| 448×896 | 264.7 / 217.3 / 215.3 | 346.1 / 334.7 / 362.7 |

At 448×896, mlx-vlm beat MetalRT on time-to-output, which contradicts the vendor's "100% win" claim.

**Text results** (summarized in [yage.ai, 2026-03-31](https://yage.ai/share/mlx-apple-silicon-en-20260331.html)):
- Qwen3-Coder-30B-A3B on M4 Pro 64 GB: MLX about 130 tok/s against Ollama 43 tok/s.
- The article estimates MLX at 1.4 to 1.8x raw llama.cpp, and 1.4 to 1.6x on dense models.
- One analysis says the MLX lead closes above about 27B, because memory bandwidth becomes the limit.
- Prefill caveat: on an M1 Max with a ~650-token prompt, effective throughput was MLX 13 against GGUF 20 tok/s, with MLX spending 94% of its time in prefill.
- Long context: on M3 Ultra at 30K+ tokens, MLX token generation was about 50% slower than llama.cpp with flash attention (mlx-lm #763).

**M5:** Apple ML Research (January 2026, MLX, Qwen3-14B-4bit) measured M5 against M4 at 4.06x faster TTFT and 1.19x faster generation, thanks to the per-GPU-core Neural Accelerators. Source: via [yage.ai](https://yage.ai/share/mlx-apple-silicon-en-20260331.html).

**Memory:** MLX used 7 to 13% less memory than GGUF on large MoE models (same source).

### Inferences
- A computer-use loop is prefill-heavy: one screenshot of about 1 to 2.5K vision tokens, plus history, produces a short action. So image prefill and TTFT matter more than decode tok/s. On M1 to M4 the two runtimes are roughly at parity for this workload. On M5, MLX's Neural Accelerator prefill advantage could be large, but that is untested on VLMs.

### Gaps
- No dated head-to-head on 7B grounding VLMs, at real screenshot sizes, on M1, M2 or M3, or across RAM sizes.
- No independent (non-vendor) vision head-to-head.

## Quantization impact on grounding accuracy

### Takeaway
I found no published study of GGUF or MLX 4-bit quantization against ScreenSpot accuracy for UI-TARS, Qwen-VL, Holo or Fara. Adjacent evidence says average scores hide many per-instance flips, so Kai must measure this itself.

### Cited Findings
- 4-bit GPTQ and AWQ on LLaVA-1.5 cost only about 0.9 to 1.5% of aggregate score, but changed about 19 to 21% of individual predictions (the "Deviation Ratio"). Source: [arXiv 2503.06794](https://arxiv.org/pdf/2503.06794).
- Qwen2.5-VL-7B NF4 lost 3 points against FP16 on a blur-robustness test. Source: [arXiv 2603.26769](https://arxiv.org/pdf/2603.26769).
- A GGUF sweep on a text coder model (Qwen2.5-Coder-7B) measured fidelity against Q8_0 at Q2_K 21.5%, Q4_K_M 56.6% and Q6_K 79.7%. This is not a GUI task. Source: [InventiveHQ](https://inventivehq.com/blog/gguf-quantization-quality-benchmark).

### Inferences
- Keep the mmproj vision projector at F16 or BF16. Prefer Q5_K_M or Q8_0 language weights for grounding models when RAM allows, and A/B against bf16 on Kai's own bench before shipping Q4.

### Gaps
- ScreenSpot-V2 and ScreenSpot-Pro accuracy at each quantization level for any computer-use model.

## Action protocols: frontier tool schemas, open-model native formats, MCP, adapters

### Takeaway
Every vendor uses a different schema:
- OpenAI: a batched `actions[]` array in pixels.
- Anthropic: a 17-member `computer_toolset_20260801` in pixels.
- Gemini: `computer_use` with environments and 0–999 normalized coordinates.
- Open models use a fourth set: UI-TARS text `Thought/Action` strings, Qwen `<tool_call>` JSON with a 0–1000 grid, Fara `computer_use` JSON in pixels at 1428×896.

The core primitives are the same everywhere (click, double-click, drag, move, scroll, type, key, wait, screenshot), plus navigate and terminate. So the model-agnostic design is one internal action enum plus per-model adapters for coordinate space and syntax. MCP fits the tool/host side but is not a model-side output format.

### Cited Findings
**OpenAI Responses `computer` tool (current docs):**
- Declared as `tools:[{type:"computer"}]`.
- The model returns a `computer_call` with an ordered `actions` array. Actions are `click` (button, x, y, keys), `double_click`, `drag`, `move`, `scroll`, `keypress`, `type` (text), `wait` and `screenshot`. The first turn may be screenshot-only.
- The client replies with `computer_call_output` holding a `computer_screenshot` `image_url` data URL (`detail:"original"`) and chains through `previous_response_id`. Pending safety checks need human acknowledgement.
- The docs' examples use `gpt-6.1-sol` and now recommend code execution for GPT-6 Astra, keeping `computer` "as an alternative". GA status is not stated.

Source: [OpenAI computer use guide](https://developers.openai.com/api/docs/guides/tools-computer-use).

**Anthropic:**
- `computer_toolset_20260801` is GA on the Claude API and Google Cloud, with no beta header. It has 17 member tools, dispatched on `(toolset_name, name)`: `screenshot`, `zoom` (region), `left_click`, `right_click`, `middle_click`, `double_click`, `triple_click` (coordinate plus optional modifier text), `left_click_drag`, `mouse_move`, `left_mouse_down`, `left_mouse_up`, `cursor_position`, `scroll` (direction, amount), `type`, `key` (with `repeat`), `hold_key` and `wait`.
- Coordinates are in the pixel space of the returned screenshot. The docs recommend 1024×768 to 1280×800 and avoiding anything above 1920×1080.
- Screenshot limits are 2576 px on the long edge and about 3.75 MP for newer models, or 1568 px and 1.15 MP for older ones. Retina 2x screens need downscaling.
- The older `computer_20251124` and `computer_20250124` versions need a beta header and are rejected on Claude 5.5+ through the first-party API.

Source: [Anthropic computer use tool docs](https://platform.claude.com/docs/en/agents-and-tools/tool-use/computer-use-tool).

**Gemini:**
- The tool is `{"type":"computer_use","environment":"browser"|"desktop"|"mobile"}`, with `excluded_predefined_functions`, `disabled_safety_policies` and `enable_prompt_injection_detection` options.
- Predefined functions: click, double/triple/middle/right click, mouse_down/up, move, type (text, press_enter), drag_and_drop, wait, press_key, key_down/up, hotkey, take_screenshot, scroll (direction, magnitude_in_pixels) and, in the browser only, navigate, go_back and go_forward.
- Each call carries an `intent` string. Coordinates are integers 0–999, normalized to a 1000×1000 grid.
- A `safety_decision` (`require_confirmation` or `blocked`) needs a `safety_acknowledgement`. Custom functions can be mixed in, such as `yield_to_user`.
- Models are gemini-4-argon, gemini-3.8-flash and others. The feature is in Preview. Page updated 2026-10-13.

Source: [Gemini computer use docs](https://ai.google.dev/gemini-api/docs/computer-use).

**Fara-7B** (Microsoft, MIT, 2025-11-24, based on Qwen2.5-VL-7B, 128k context):
- Output is chain-of-thought followed by a `computer_use` tool call `{"name","arguments"}`.
- Actions: key, type, mouse_move, left_click, scroll (pixels, positive means up), visit_url, web_search, history_back, pause_and_memorize_fact, wait and terminate (success or failure).
- Pixel `coordinate` [x,y] at a 1428×896 screen. It uses the ChatML template with a required system prompt that stops at critical points.
- Scores: WebVoyager 73.5, Online-Mind2Web 34.1 and WebTailBench 38.4. On the same benchmarks UI-TARS-1.5-7B scored 66.4, 31.3 and 19.5.
- Fara1.5 now exists in 4B, 9B and 27B sizes.

Source: [Fara-7B model card](https://huggingface.co/microsoft/Fara-7B).

**UI-TARS (v1 and 1.5):** a custom text format, `Thought: …` then `Action: click(start_box='<|box_start|>(x,y)<|box_end|>')`. Actions are click, left_double, right_single, drag, hotkey, type, scroll, wait, finished and call_user. It is not JSON function calling. UI-TARS-1.5 is Qwen2.5-VL based and Apache-2.0. Sources: [UI-TARS README_v1](https://cdn.jsdelivr.net/gh/bytedance/UI-TARS@main/README_v1.md) and [lilting.ch](https://lilting.ch/articles/ui-tars-gui-agent).

**Qwen3-VL:**
- The cookbooks have the model emit `<tool_call>{"name":"mobile_use" | "computer_use","arguments":{"action":…,"coordinate":[x,y]}}</tool_call>`.
- Coordinates are on a relative 0–1000 grid. Secondary sources conflict on whether the range is 0–999 or 0–1000.

Sources: [DeepWiki Qwen3-VL mobile agent](https://deepwiki.com/QwenLM/Qwen3-VL/5.3-mobile-agent-and-function-calling) and [DeepWiki grounding](https://deepwiki.com/QwenLM/Qwen3-VL/5.2-spatial-understanding-and-2d-grounding).

**Holo2** (H Company; 4B, 8B, 30B-A3B, and a 235B-A22B localization leader): a thinking model served through vLLM. The license is reportedly CC-BY-NC-4.0 for 30B-A3B, per a third party. The exact action format was not retrieved. Sources: [H Company blog](https://huggingface.co/blog/Hcompany/introducing-holo2-235b-a22b) and [LLM Reference](https://www.llmreference.com/model/holo-2-30b-a3b).

**MCP servers for computer control** (open source, 2026):
- [Open Computer Use](https://www.producthunt.com/products/open-computer-use): MIT, macOS/Linux/Windows. Tools include list_apps, get_app_state, click, drag, scroll, type_text, press_key, set_value and perform_secondary_action.
- [Windows-MCP](https://github.com/CursorTouch/Windows-MCP).
- [computer-use-linux](https://github.com/agent-sh/computer-use-linux): Rust; AT-SPI, GNOME Shell, Wayland portals and ydotool.
- [macos-control-mcp](https://github.com/PeterHdd/macos-control-mcp): screenshots, OCR, click and type.
- [mcp-desktop](https://github.com/mocha06/mcp-desktop): split into a read-only screenshot server and a control server.
- Cua Driver: CLI, MCP or SDK on all three OSes.

Overview: [pinggy roundup](https://pinggy.io/blog/best_computer_use_mcp_servers/).

### Inferences
**Adapter design.** The adapter layer has three parts: (1) coordinate mapping, (2) output parsing, and (3) history and screenshot formatting.

Coordinate spaces:
- Pixels of the sent image: Anthropic, OpenAI, Fara.
- 0–999 normalized: Gemini.
- 0–1000 normalized: Qwen3-VL.
- Absolute pixels after Qwen2.5-VL smart-resize: UI-TARS-1.5. This is per the UI-TARS repo, not re-verified here.

Output syntaxes:
- JSON tool calls: OpenAI, Anthropic, Gemini, Fara, Qwen.
- Regex over Thought/Action text: UI-TARS.

**Kai implications:**
- Kai's existing internal action schema (`docs/CUA_NATIVE_PROTOCOL.md`) is the right integration point. Each native format becomes a parser plus a coordinate transform.
- Keep native per-model prompts byte-stable, because fine-tuned models degrade when given foreign schemas.
- MCP is best used to *expose* Kai's hands to other agents (Kai already has `kai mcp`), not as a model protocol.

**Structured output trade-off.** Grammar or JSON-schema constraints, which llama.cpp and vllm-mlx both support, make generic models reliable. They can, however, conflict with a fine-tuned model's native format, such as UI-TARS free-text. So constrain only generic models (Gemma, plain Qwen) and parse native output for computer-use fine-tunes.

### Gaps
- UI-TARS-2, OpenCUA (OpenCUA-7B/32B uses pyautogui-code output, from memory, not verified here) and GLM-4.5V/4.6V grounding formats were not retrieved within the budget.
- No source was found on how Agent S, Bytebot or OpenCUA structure their adapters.

## Cloud providers serving open VLMs as a fallback

### Takeaway
Fireworks, Together, DeepInfra and OpenRouter host Gemma 4 and Qwen3-VL (Together lists Gemma 4 31B; the others list 26B-A4B). Groq's listed models show no Qwen3-VL or Gemma 4. No source showed UI-TARS, Holo or Fara on any major serverless host, so self-hosting with vLLM is the path for those.

### Cited Findings
- Providers and models, all from third-party aggregators, so verify on each provider's site:
  - Gemma 4 26B A4B: Fireworks, OpenRouter and DeepInfra.
  - Gemma 4 31B: Together.
  - Qwen3-VL-235B-A22B-Instruct: DeepInfra and Fireworks.
  - Qwen3 VL 8B Instruct (and others): OpenRouter.
  - Price spread example: Gemma 4 26B input cost $0.90 on Fireworks against $0.09 on OpenRouter.

  Sources: [pricepertoken Fireworks vs OpenRouter](https://pricepertoken.com/endpoints/compare/fireworks-vs-openrouter), [DeepInfra Qwen pricing guide](https://deepinfra.com/blog/qwen-api-pricing-2026-guide), [OpenRouter Qwen](https://openrouter.ai/qwen) and [pricepertoken OpenRouter vs Together](https://pricepertoken.com/endpoints/compare/openrouter-vs-together).
- Groq's model list as cited: gpt-oss-20B/120B, Llama 3.3 70B, Llama 4 Scout, Qwen3 32B and Kimi K2. Llama 4 Scout is natively multimodal. Source: [beri.net comparison](https://www.beri.net/article/together-vs-fireworks-vs-groq-open-weight-inference-tool-call-accuracy-2026).
- UI-TARS-1.5-7B is listed on the ifq.ai aggregator: [api.ifq.ai](https://api.ifq.ai/bytedance/ui-tars-1.5-7b/providers). Holo2-8B is on Featherless: [featherless.ai](https://featherless.ai/models/Hcompany/Holo2-8B). That is a sign that smaller hosts carry these models.
- Kai already has a Gemini backend for `gemma-4-26b-a4b-it` (repo `CLAUDE.md`).

### Inferences
- All of these providers are OpenAI-compatible. Kai's `OpenAiClient`, with a base URL and model from config, covers them without new code. Gemma 4 through the Gemini API, or OpenRouter, is the cheapest fallback that matches the default local model.

### Gaps
- No measured tokens/s or TTFT for image inputs on Groq, Together, Fireworks or DeepInfra.
- Whether each provider supports `json_schema` or tool calling together with image input, for these models.
