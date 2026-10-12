# Research: how Anthropic and OpenAI do computer use, and where Kai should differ

Written 2026-10-12. Builds on [RESEARCH.md](RESEARCH.md) (speed inside the
harness, Gemini computer use, leaderboards), [ROADMAP.md](ROADMAP.md)
(training) and [RESEARCH_2026_10_FOUNDATIONS.md](RESEARCH_2026_10_FOUNDATIONS.md)
(verification, procedures, background control, CAPTCHAs). It does not repeat
them. This document looks at the two frontier vendors' current computer-use
stacks end to end, asks which parts Kai should copy, which parts it should do
differently because it runs small open models, and ranks what to build.

Labels as before: **[Paper]**, **[Vendor]** (the vendor's own docs or claims),
**[Indep.]**, **[Practitioner]**, **[Secondary]** (a blog or aggregator
quoting someone else; not checked against the primary), **[Est.]**.

---

## 0. The short version

1. **Both vendors now ship two surfaces, not one.** Anthropic has a
   `computer_toolset_20260801` (17 pixel-level members) **and** a separate
   `browser_toolset_20260801` (31 members) in which the page is an
   accessibility tree with stable `ref_N` handles, page text and DOM-level form
   input. OpenAI has the `computer` tool **and** recommends code execution
   (Playwright/PyAutoGUI scripts) for its strongest model. Kai has one
   surface, the desktop, and reaches web pages only through the browser's
   desktop accessibility tree and screenshots. **That is Kai's biggest
   structural gap, and §5.1 closes it.**
2. **The observation and action space matters as much as the model.** With
   the same model (GPT-5 mini), success fell from **83.1%** with
   accessibility-tree observations to **48.9%** with coordinate-only control
   ([ComponentBench, arXiv 2608.18307](https://arxiv.org/abs/2608.18307), 2026-08) [Paper].
   Kai's Set-of-Marks-from-the-tree design is on the right side of that gap.
   Its web lane should be too.
3. **The vendors' answer to speed is fewer round trips, not faster pixels.**
   Both batch several actions per model turn. Anthropic says "read before you
   look": the page tree is "often cheaper" than a screenshot and gives
   actionable references. OpenAI points its best model at code execution.
   GPT-6 Astra's own number is **~40 min vs ~75 min** per OSWorld 2.0 task
   against its predecessor, with "asynchronous tool calls" listed among the
   new features ([The Batch](https://www.deeplearning.ai/the-batch/gpt-6-astra-is-a-star)) [Secondary].
   Kai already batches (`then`) and skips screenshots (tree-only steps). The
   browser lane turns most web steps into tree-only steps.
4. **Background operation is now table stakes.** Codex (2026-04-16) drives
   macOS apps "with its own cursor … in the background without taking over
   your computer" ([OpenAI on X](https://x.com/OpenAI/status/2044827932145897652)) [Vendor].
   For the web, the cleanest way to get this is the browser's own protocol:
   Chrome DevTools Protocol (CDP) input events go to the page, not to the OS,
   so there is no cursor to steal and no focus to take.
5. **Open weights caught up on the model side.** H Company's Holo4-35B-A3B
   (Apache 2.0, ~3B active, 2026-09-28) reports **80.8%** on OSWorld, and
   Holo3.1-4B/9B report **72%** on AndroidWorld [Vendor]. The 35B-A3B has the
   same shape as Gemma 4 26B-A4B. A model swap is now a real option for Kai,
   but every number is vendor-run (§4).
6. **Kai's own different approach is the right one for small models: keep the
   model's job tiny.** One JSON action, numbered marks from the tree (never
   pixels), harness-enforced safety, executable procedures. A 31-member tool
   surface is cheap for a frontier model and expensive for a 4B-active one.
   So Kai's browser lane keeps the **same 12-action schema** and changes only
   what the harness does under it.

---

## 1. Anthropic

### 1.1 The computer-use toolset (current)

From the [computer-use tool docs](https://platform.claude.com/docs/en/docs/agents-and-tools/tool-use/computer-use-tool),
accessed 2026-10-12 [Vendor]:

- **Versions.** `computer_toolset_20260801` is GA on the Claude API and
  Google Cloud, needs no beta header, and is *required* for Claude 5.5 models.
  Earlier: `computer_20251124` (Opus 4.5–4.7, Sonnet 4.6, beta header),
  `computer_20250124`, and the original `computer_20241022`.
- **17 members**, each its own tool call (no `action` field): `screenshot`,
  `zoom` (region `[x0,y0,x1,y1]` at full resolution), five click types,
  `left_click_drag`, `mouse_move`, `left_mouse_down/up`, `cursor_position`,
  `scroll`, `type`, `key` (with `repeat` 1–100), `hold_key`, `wait`.
- **Batching.** Several calls per response; run them **in order**, **stop at
  the first failure**, answer the rest with the exact text "Not executed: an
  earlier computer action in this turn failed." Ask for a human confirmation
  before each block if the app does that. Attach a screenshot to the last
  result to save a round trip.
- **Images.** The API no longer downscales. For Claude Opus 4.7 and later the
  limit is 2576 px long edge and 4784 visual tokens (`⌈w/28⌉ × ⌈h/28⌉`, about
  3.75 MP). Above 20 images per request each side is capped at 2000 px.
  Recommended screens: 1024×768 or 1280×720 for desktop, 1280×800 or 1366×768
  for web; avoid above 1920×1080. Each screenshot costs roughly **1,000–1,800
  input tokens**.
- **Coordinates** are in the pixel space of the full screenshot you return;
  zoom does not change that.
- **Prompting tips** worth copying: put the instruction text **before** the
  image in the user turn (improves click accuracy); prefer keyboard shortcuts
  for dropdowns and scrollbars; ask for a one-sentence verification after each
  step.
- **Caching.** One breakpoint after the system prompt and tools; prune old
  screenshots in batches (every ~25 turns) rather than every turn, because
  each prune invalidates the cache.
- **Safety.** Classifiers scan tool results (screenshots, page text) for
  prompt injection and steer Claude to check with the user. Anthropic still
  tells developers to run in a VM, keep secrets out, allowlist domains, and
  confirm consequential actions.

### 1.2 The browser toolset (new, and the important part for Kai)

From the [browser-use tool docs](https://platform.claude.com/docs/en/docs/agents-and-tools/tool-use/browser-use-tool),
accessed 2026-10-12 [Vendor]:

- `browser_toolset_20260801`, 31 members (27 on by default; `javascript_exec`,
  `file_upload`, `read_console`, `read_network` off by default).
- **Three views of a page:** `read_page` (the accessibility tree as text, each
  element tagged `[ref_N]`, filter `interactive`, depth limit, 50,000-character
  cap), `get_page_text` (main content as plain text), and `screenshot`/`zoom`
  (viewport only).
- **Targets** are `{"type":"ref","ref":"ref_2"}` or a coordinate. Refs are
  "accessibility-node ID or a stored selector", scoped to a tab, valid until
  navigation or a material DOM change; the executor must return a specific
  "stale ref" error and must not renumber refs it already issued.
- `find` takes a natural-language query and returns up to 20 matching refs.
- `navigate` must refuse `javascript:`, `file:`, `data:`, `chrome:` and parse
  the URL properly; a URL without a scheme is `https://`.
- Guidance: **"Read before you look"**, scope reads, use page text for
  articles, batch actions, use refs where the tree is usable and coordinates
  for canvas, video, remote desktops and cross-origin iframes. "Refs survive
  layout shifts better than coordinates."
- The executor is the developer's; Anthropic names no stack (CDP or
  Playwright are the obvious choices).

### 1.3 How Claude learned it, and the browser extension

- **Training.** Anthropic's launch write-up (2024-10) described teaching the
  model to count pixels to the target, training on a few simple programs
  (a calculator, a text editor) with no internet access, and seeing the skill
  generalise ([Maginative](https://www.maginative.com/article/anthropics-claude-ai-can-now-control-your-computer/),
  [InfoQ](https://www.infoq.com/news/2024/11/anthropic-claude-computer-use/)) [Secondary].
  Kai's design avoids pixel counting altogether (numbered marks), which is the
  sensible choice for a model that was not trained on it.
- **Claude in Chrome** (pilot 2025-08, GA 2026). Prompt-injection attack
  success was **23.6%** without mitigations and **11.2%** with them in the
  pilot; a browser-specific challenge set went **35.7% → 0%**
  ([Simon Willison quoting the pilot](https://feeds.simonwillison.net/2025/Aug/26/piloting-claude-for-chrome)) [Secondary].
  Anthropic's later post calls a ~1% success rate against an adaptive
  best-of-N attacker "meaningful risk"
  ([prompt-injection defenses](https://www.anthropic.com/research/prompt-injection-defenses)) [Vendor],
  and the GA post adds probes that screen web content before Claude acts
  ([Claude in Chrome GA](https://claude.com/resources/articles/claude-in-chrome-generally-available)) [Vendor].
  **For Kai:** open models have none of this training, so Kai's existing rule
  (screen text is data; pause on injection or phishing, `docs/SAFETY.md`)
  must cover the browser lane too, and page text is the most injectable input
  there is.

---

## 2. OpenAI

From the [computer-use guide](https://developers.openai.com/api/docs/guides/tools-computer-use)
and the [GPT-6 Astra guide](https://developers.openai.com/api/docs/guides/latest-model/gpt-6-astra.md),
accessed 2026-10-12 [Vendor], unless marked:

- **History.** Operator's Computer-Using Agent (2025-01) was "GPT-4o's vision
  capabilities with advanced reasoning through reinforcement learning",
  operating only through the visual interface: OSWorld **38.1%**, WebArena
  58.1%, WebVoyager 87% ([OpenAI](https://openai.com/index/computer-using-agent/)) [Vendor].
  Kai already implements this native loop (`crates/kai-core/src/cua.rs`).
- **Computer tool today.** `{ type: "computer" }`; a `computer_call` carries an
  ordered **`actions` array** (click, double_click, drag, move, scroll,
  keypress, type, wait, screenshot); screenshots go back as PNG with
  `detail: "original"`.
- **Code execution is the recommended mode for GPT-6 Astra.** The model
  writes Python (`pyautogui`) or JavaScript (Playwright `page`, `context`,
  `browser`) into a persistent runtime the developer hosts. Variables persist
  across calls. This collapses many UI round trips into one script.
- **Codex background computer use** (2026-04-16, macOS first): "its own
  cursor", parallel cursors per agent thread, apps are not brought to the
  front; needs Screen Recording and Accessibility permissions [Vendor]. One
  blog claims it uses private SkyLight APIs with focus-without-raise
  ([buildmvpfast](https://www.buildmvpfast.com/blog/openai-codex-background-computer-use-desktop-agent-2026)) [Secondary, unverified];
  Cua's open driver documents that approach
  ([Cua blog](https://cua.ai/blog/inside-macos-window-internals)) [Practitioner].
- **GPT-6 Astra** (2026-09-03): OSWorld 2.0 offline subset **72.6%** at
  **~40 min** per task in "latency simulations" vs 65.7% at ~75 min for
  GPT-5.6 Sol [Vendor via The Batch]. New API features listed alongside:
  asynchronous tool calls (the model keeps reasoning while a tool runs),
  mid-turn steering, reasoning-effort changes mid-conversation
  (`configuration_update`), compaction.
- **Safety.** Classifiers review reasoning and actions on every tool-using
  call and can halt work; in ChatGPT/Codex a flagged task waits for the user.
  The checks run *alongside* the model, so an action can finish before it is
  flagged [Secondary, The Batch]. Kai's approvals run *before* the action,
  which is stricter and should stay that way.

---

## 3. Where Kai already differs, on purpose

| Concern | Anthropic / OpenAI | Kai today | Why Kai's way fits open models |
| --- | --- | --- | --- |
| Grounding | Pixel coordinates the model learned (pixel counting, RL) | Numbered marks from the accessibility tree; grid fallback | A 4B-active model was never trained to count pixels; picking a number is a classification it can do |
| Action surface | 17 (computer) + 31 (browser) tool members; code mode | One JSON object, 12 actions, schema-constrained on vLLM | Fewer choices, no retries, ~192 output tokens |
| Batching | Several tool calls per turn; stop at first failure | `then` follow-ups, each re-checked against the tree | Same idea, plus a guard the vendors leave to the developer |
| Screenshots | ~1,000–1,800 tokens each, up to 2576 px | ~280 tokens (Gemma's budget), tree-only steps | Prefill dominates small-model latency |
| Safety | Model training + classifiers alongside | Harness rules before every risky action, password fields always high risk, human hand-off for CAPTCHAs | Open models lack the safety training; the harness must not depend on it |
| Memory | None in the API (developer's job) | Procedures, app facts, lessons, gated by an exam | Turns repeat tasks into zero-model-call replays |
| Web pages | Separate browser surface with refs and page text | The desktop a11y tree of whatever browser is open | **Gap** (§5.1) |
| Background | Codex: own cursor per thread | AT-SPI/UIA actions work on covered windows; mouse fallback takes the real pointer | **Partial gap** (§5.4) |

---

## 4. What the open side offers now

| Work | Result | Date | Label | For Kai |
| --- | --- | --- | --- | --- |
| **Holo4-35B-A3B** (H Company), Apache 2.0, built on Qwen3.8 | OSWorld **80.8%**, OSWorld 2.0 30.9% at $0.61/task; the 27B dense is 85.2% / 61.7% but **CC BY-NC** | 2026-09-28 | [Vendor] via [datanorth](https://datanorth.ai/news/h-company-releases-holo4), [HF](https://huggingface.co/Hcompany/Holo4-27B) | Same MoE shape as Gemma 26B-A4B; the strongest *shippable* open computer-use model. Candidate fast model on the Droplet (§5.3) |
| **Holo3.1** 0.8B / 4B / 9B / 35B-A3B | AndroidWorld 72% (4B, 9B), 79.3% (35B-A3B); NVFP4 + harness work cut step time **6.8 s → 3.3 s** on DGX Spark | 2026-06-02 | [Vendor] [HF blog](https://huggingface.co/blog/Hcompany/holo31) | The 4B is a candidate local *grounder* for screens with no tree (§5.5) |
| **CoAct-1**: an orchestrator picks a GUI operator or a programmer (Python/Bash) per subtask | OSWorld **60.8%**, WAA 52.5%; **10.15 steps** vs ~15 for GUI-only agents; biggest gains in Calc, multi-app, VS Code | ICLR 2026 | [Paper] [2508.03923](https://arxiv.org/abs/2508.03923) | Kai's `shell` already is the programmer lane; Team mode already splits terminal/screen subtasks |
| **ComponentBench**: same tasks, four observation/action spaces | GPT-5 mini **83.1%** (a11y tree) vs **48.9%** (pixels); >30 pp swings from the space alone; fastest setting still 3.7× human time | 2026-08 | [Paper] [2608.18307](https://arxiv.org/abs/2608.18307) | Strongest evidence for refs over pixels in the browser lane |
| **GUI-Actor**: an attention head picks screen patches instead of writing coordinates | ScreenSpot-Pro 44.6 (7B), 42.2 (3B); backbone can stay frozen | NeurIPS 2025 | [Paper] [2506.03143](https://arxiv.org/abs/2506.03143) | Grounding without coordinate text; relevant only for grid screens |
| **GUIPruner** (training-free token pruning) | **3.4×** fewer FLOPs, **3.3×** faster vision encoding, >94% of accuracy (Qwen2-VL-2B) | 2026-02 | [Paper] [2602.23235](https://arxiv.org/abs/2602.23235) | Not needed at Gemma's ~280 image tokens; needed if Kai moves to a native-resolution model like Holo/Qwen |
| **TRACE** (prune once, keep a compact session cache) | numbers not in the abstract | 2026-09 | [Paper] [2609.10297](https://arxiv.org/abs/2609.10297) | Same caveat |
| **Speculative Actions** | up to 55% next-action accuracy, up to 20% lower latency (not GUI) | ICLR 2026 | [Paper] [2510.04371](https://arxiv.org/abs/2510.04371) | Safe only for side-effect-free actions; small win |

---

## 5. Ranked plan: what Kai should build, and how it differs

### 5.1 A browser lane over the DevTools Protocol (build now)

**What.** When a task is about the web, Kai drives a Chromium-family browser
it launched itself, through CDP, instead of through the desktop:

- **Observe** with two protocol calls: `Accessibility.getFullAXTree` (roles,
  names, values) joined with `DOMSnapshot.captureSnapshot` (layout boxes,
  input types) on the backend node id. This gives the same numbered
  Set-of-Marks list Kai already shows, without the browser chrome's own
  controls, and with real form values. The screenshot comes from
  `Page.captureScreenshot`, which works when the window is covered.
- **Act** by mark → backend node: scroll into view, read its box, and send
  `Input.dispatchMouseEvent` / `Input.insertText` / `Input.dispatchKeyEvent`
  **to the page**. These are trusted events to the page, but they never move
  the OS pointer or take keyboard focus: background operation for the web
  with no OS tricks.
- **Verify** with the page URL itself (`url_is` reads the real URL, not the
  address-bar text the desktop tree may or may not expose) and the title.
- **Navigation policy** in the harness: `http`/`https` only, a bare host means
  `https://`, everything else refused (Anthropic's rule).

**How this differs from the vendors.** The model does not get a second tool
surface. It gets the same marks and the same 12 actions; the harness picks
the lane. A 4B-active model's prompt and schema stay byte-for-byte the same,
so prefix caching and any fine-tune keep working.

**Rejected.** (a) Attaching to the user's everyday Chrome: Chrome 136+
ignores remote-debugging switches on the default profile, and the new
`chrome://inspect` route needs a manual toggle plus a prompt per connection
([Chrome DevTools blog](https://developer.chrome.com/blog/chrome-devtools-mcp-debug-your-browser-session),
[miniasp](https://blog.miniasp.com/post/2026/08/16/chrome-remote-debugging-autoconnect-9222-cdp)).
Kai uses its own profile folder, so the user signs in to a site once there.
(b) A TCP debugging port: any local process could connect to it and drive the
browser. Kai uses `--remote-debugging-pipe` (a private pipe to the child
process) instead. (c) Playwright or a Node sidecar: a large runtime for a
handful of protocol calls. (d) Firefox: it dropped CDP for WebDriver BiDi;
a later port.

Design note, tests and measurements: [BROWSER.md](BROWSER.md).

### 5.2 Page text as a read action (next)

Anthropic's `get_page_text` exists because reading tasks ("what is the price
of …") fail when the answer is in non-interactive text and the small
screenshot is too coarse. Kai's `local-price-lookup` bench task failed in
exactly this way (README). In the browser lane, add the page's main text
(capped, marked as untrusted data) to the observation when the model asks
for it, instead of a new action. Measure on the bench's reading tasks.

### 5.3 A second open model on the Droplet: Holo4-35B-A3B (measure first)

Same active size as Gemma 4 26B-A4B, Apache 2.0, trained specifically for
computer use. Kai's `vllm` backend already sends an OpenAI-compatible
request with structured outputs, so trying it is a config change plus a prompt
check. Do not switch the default on vendor numbers: run `kai bench --backend
vllm --model Hcompany/Holo4-35B-A3B` against Gemma on the same Droplet, at
least twice per side. Note its native image size is far above Gemma's 280
tokens; if prefill dominates, that is where GUIPruner-style pruning (§4)
becomes worth building.

### 5.4 Background desktop driving (already planned)

Unchanged from [RESEARCH_2026_10_FOUNDATIONS.md](RESEARCH_2026_10_FOUNDATIONS.md) §4,
now with a smaller scope because the browser lane covers web pages: a mouse
fallback that does not move the user's pointer (X11 multi-pointer, Windows
`PostMessage` for Win32 controls), per-window capture, and one session per
sub-agent.

### 5.5 A local grounder for screens without a tree (measure first)

When the tree is empty (canvas apps, games, some Electron), Kai falls back to
a coarse grid. A small grounding model (Holo3.1-4B, or a GUI-Actor-style
head) could turn "the Save icon" into a box on CPU or a small GPU. It costs a
model call only on grid screens. Build only if the bench shows grid-screen
failures.

### 5.6 Not now

- **Code execution as the main mode** (OpenAI's recommendation for Astra):
  it relies on a frontier model writing correct scripts; Kai's `shell` covers
  the safe, verifiable part.
- **Speculative actions:** ≤ 20% latency in the paper, and only for
  side-effect-free actions. Kai's batching and tree-only steps already take
  most of that.
- **Token pruning:** see §5.3.

---

## 6. Sources (accessed 2026-10-12)

Vendor docs and posts:

- Anthropic, computer-use tool — https://platform.claude.com/docs/en/docs/agents-and-tools/tool-use/computer-use-tool
- Anthropic, browser-use tool — https://platform.claude.com/docs/en/docs/agents-and-tools/tool-use/browser-use-tool
- Anthropic, prompt-injection defenses — https://www.anthropic.com/research/prompt-injection-defenses
- Claude in Chrome GA — https://claude.com/resources/articles/claude-in-chrome-generally-available
- OpenAI, computer use guide — https://developers.openai.com/api/docs/guides/tools-computer-use
- OpenAI, GPT-6 Astra guide — https://developers.openai.com/api/docs/guides/latest-model/gpt-6-astra.md
- OpenAI, Computer-Using Agent — https://openai.com/index/computer-using-agent/
- OpenAI, Codex computer use on macOS — https://x.com/OpenAI/status/2044827932145897652
- H Company, Holo3.1 — https://huggingface.co/blog/Hcompany/holo31 ; Holo4 model card — https://huggingface.co/Hcompany/Holo4-27B
- Chrome DevTools MCP, connecting to an existing session — https://developer.chrome.com/blog/chrome-devtools-mcp-debug-your-browser-session

Papers:

- ComponentBench — https://arxiv.org/abs/2608.18307
- CoAct-1 — https://arxiv.org/abs/2508.03923
- GUI-Actor — https://arxiv.org/abs/2506.03143
- GUIPruner — https://arxiv.org/abs/2602.23235
- TRACE — https://arxiv.org/abs/2609.10297
- Speculative Actions — https://arxiv.org/abs/2510.04371
- AAPT (pre-compiled policy trees) — https://arxiv.org/abs/2607.28399

Secondary (claims not checked against a primary):

- The Batch on GPT-6 Astra — https://www.deeplearning.ai/the-batch/gpt-6-astra-is-a-star
- Simon Willison on the Claude for Chrome pilot — https://feeds.simonwillison.net/2025/Aug/26/piloting-claude-for-chrome
- Maginative / InfoQ on Claude's 2024 computer-use training — https://www.maginative.com/article/anthropics-claude-ai-can-now-control-your-computer/ , https://www.infoq.com/news/2024/11/anthropic-claude-computer-use/
- Holo4 coverage — https://datanorth.ai/news/h-company-releases-holo4
- Codex background computer use write-up — https://www.buildmvpfast.com/blog/openai-codex-background-computer-use-desktop-agent-2026
- Cua, macOS window internals — https://cua.ai/blog/inside-macos-window-internals
- Chrome 136 remote-debugging change, autoConnect — https://blog.miniasp.com/post/2026/08/16/chrome-remote-debugging-autoconnect-9222-cdp
