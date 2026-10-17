# Architecture of leading computer-use agents (as of 2026-10-17)

Scope: implementation language and UI stack, perception, action, models, macOS/Linux locality, published results, and which patterns correlate with the best scores. Focus on macOS (Apple Silicon) and Linux desktop agents running open-weight models. Research done 2026-10-17. Many 2026 benchmark numbers come from vendor posts or aggregators and could not be checked against the official OSWorld workbook. Each one is marked accordingly.

## Q1. Per-agent architecture: language, UI stack, perception, action, models, OS support, results

### Takeaway
Two families have emerged. (a) **Closed frontier models exposed as a tool** (Anthropic computer toolset, Gemini computer use, OpenAI CUA / Codex computer use) that take screenshots plus coordinate actions, with hosts in Python/TS reference loops, Docker/Xvfb, or a native macOS app reading the AX tree. (b) **Open frameworks**, almost all **Python** (Agent S, OpenCUA, UFO, browser-use, Open Interpreter) or **TypeScript/Electron** (UI-TARS-desktop / Agent TARS), with Cua moving its core to **Rust**, adding a **Swift** macOS app and a **Tauri** app for Linux/Windows. Open-weight models (UI-TARS-1.5, OpenCUA-72B, Fara-7B, Holo4, Ferret-UI Lite) are almost all **Qwen-VL derivatives**. They act from screenshots alone and output pixel or normalized coordinates.

### Cited Findings

**Anthropic: computer use tool / Claude in Chrome**
- The current tool version is `computer_toolset_20260801`, GA on the Claude API and Google Cloud with no beta header. It supports Claude Fable 5/5.1, Mythos 5/5.1, Opus 5/5.5, Sonnet 5/5.5, Opus 4.8 and Haiku 5.5. Earlier versions are `computer_20251124` (beta, Opus 4.5–4.7, Sonnet 4.6) and `computer_20250124`, which 5.5+ models reject. — [Claude docs, computer use tool](https://platform.claude.com/docs/en/agents-and-tools/tool-use/computer-use-tool)
- It has 17 member actions: `screenshot`, `zoom` (region → full-resolution crop), left/right/middle/double/triple click, drag, mouse_move, mouse down/up, cursor_position, scroll, type, key, hold_key and wait. Each member can be withheld via `configs`. — [Claude docs](https://platform.claude.com/docs/en/agents-and-tools/tool-use/computer-use-tool)
- Perception is screenshots only, at up to 2576 px long edge and about 3.75 MP (4784 visual tokens) for new models, or 1568 px / about 1.15 MP for older ones. The API does not downscale, so the client must resize and rescale coordinates. Recommended resolution is 1024x768 / 1280x720 for desktops and 1280x800 / 1366x768 for web. **On macOS Retina, screenshots are 2x, so downscale by 2 or halve the coordinates.** — [Claude docs](https://platform.claude.com/docs/en/agents-and-tools/tool-use/computer-use-tool)
- The reference implementation is a Docker container with Xvfb (virtual X11), Mutter WM, Tint2 panel, Firefox and LibreOffice, plus tool implementations and an agent loop (`anthropics/anthropic-quickstarts/computer-use-demo`). — [Claude docs](https://platform.claude.com/docs/en/agents-and-tools/tool-use/computer-use-tool)
- Speed and cost notes from the docs:
  - A screenshot costs about 1,000–1,800 input tokens.
  - Batch actions (several `tool_use` blocks per turn) cut round trips.
  - Prune screenshots in batches to keep prompt caching effective (for example keep the last 3 and prune every 25 turns).
  - Add about 0.5 s waits after clicks.
  - The default reference loop caps at 10 iterations.

  — [Claude docs](https://platform.claude.com/docs/en/agents-and-tools/tool-use/computer-use-tool)
- Claude in Chrome is a browser extension with a side panel. Unlike computer use, it is confined to the browser and reads page structure directly. It integrates with Claude Code to read the DOM, console and network. It is protected by safety classifiers but "still risky". — [Claude support](https://support.claude.com/en/articles/12012173-computer-use); [contextstudios glossary](https://www.contextstudios.ai/fr/glossaire/claude-in-chrome) (secondary)
- A "Quick Mode" in which the model emits a compact single-letter command language that the extension parses is reported to be about 3x faster. This is a single third-party source and unconfirmed. — [pasqualepillitteri.it](https://pasqualepillitteri.it/en/news/346/claude-chrome-quick-mode-fast-browsing-guide)

**OpenAI: Operator / CUA, ChatGPT agent, Codex computer use**
- CUA (Jan 2025) works from screenshots plus a virtual mouse and keyboard in a VM, which OpenAI calls "one general action space". Results: 38.1% OSWorld, 58.1% WebArena, 87% WebVoyager. It asks for confirmation on logins and CAPTCHAs. — [OpenAI, Computer-Using Agent](https://openai.com/index/computer-using-agent/)
- **Codex app computer use (2026-04-16, "Codex for (almost) everything", desktop v26.415):**
  - It runs in the background with its own cursor, and several agents can work in parallel on different apps without taking over the desktop.
  - It is **macOS only** at launch.
  - It reads the macOS **AX tree** (accessibility hierarchy) of open apps as model context, not only screenshots.

  — [Daniel Vaughan's Codex notes](https://codex.danielvaughan.com/2026/04/17/codex-app-computer-use-macos-background-gui-automation/); [mjtsai.com 2026-04-17](https://mjtsai.com/blog/2026/04/17/); [testingcatalog](https://www.testingcatalog.com/openai-codex-transformed-into-superapp-with-computer-use/)
- GPT-5.5 is reported at 78.7% on OSWorld-Verified. This is a vendor-reported figure relayed by secondary sources. — [theplanettools.ai](https://theplanettools.ai/blog/gemini-3-5-flash-native-computer-use-three-way-tie-2026)

**Google: Gemini Computer Use / Project Mariner**
- `gemini-2.5-computer-use-preview-10-2025` (Oct 2025) is "primarily optimized for web browsers" with "promise for mobile". It is claimed to lead Online-Mind2Web and WebVoyager "with lower latency", with evaluations partly run by Browserbase. — [Google blog](https://blog.google/technology/google-deepmind/gemini-computer-use-model/)
- It uses a fixed action vocabulary and outputs **coordinates normalized to a 0–1000 grid** whatever the viewport size, so the client scales them to pixels. The client executes actions, typically through Playwright or a cloud browser. — [Steel docs](https://docs.steel.dev/cookbook/gemini-computer-use) (third-party integration guide)
- On 2026-06-24, computer use became a built-in tool of **Gemini 3.5 Flash**, replacing the separate 2.5 model. It covers browser, apps and desktop. Reported results are 78.4% OSWorld-Verified (up from 65.1% for Gemini 3 Flash), at $1.50 / $9 per M tokens. These are vendor figures via secondary sources. One source says it is a public preview. — [basic-tutorials.com](https://basic-tutorials.com/news/gemini-3-5-flash-gets-computer-use-googles-ki-steuert-jetzt-browser-apps-und-desktop-selbst/); [basic-tutorials.de](https://basic-tutorials.de/news/gemini-3-5-flash-bekommt-computer-use-googles-ki-steuert-jetzt-browser-apps-und-desktop-selbst/); [digitalapplied](https://www.digitalapplied.com/blog/gemini-3-5-flash-computer-use-agent-automation-2026)
- Conflict: Simular's comparison page cites a "Gemini 3.8 Flash" model card at 59.0% on OSWorld-2.0 and says Google has no OSWorld-Verified leaderboard submission. — [simular.ai](https://www.simular.ai/alternatives/gemini-computer-use-alternatives)

**ByteDance: UI-TARS-desktop / Agent TARS / UI-TARS models**
- The repo is a pnpm monorepo. The Agent TARS CLI needs Node.js ≥ 22 and also has a Web UI.
- It has local-computer, remote-computer and browser operators (local and remote).
- Agent TARS browser control can be GUI (visual), DOM or hybrid. UI-TARS Desktop itself works from screenshots.
- Models: UI-TARS-1.5 (7B on Hugging Face), Seed-1.5-VL/1.6, Doubao (`doubao-1-5-thinking-vision-pro`), and Claude as an example.
- Listed OS support is Windows/macOS/Browser. Linux is not mentioned.
- Releases: UI-TARS Desktop v0.1.0 (2025-04-17) and v0.2.0 (2025-06-12); Agent TARS CLI v0.3.0 (2025-11-05).

  — [GitHub bytedance/UI-TARS-desktop](https://github.com/bytedance/UI-TARS-desktop)
- The desktop app is Electron, inside a pnpm/Turborepo monorepo. — [pyshine](https://pyshine.com/UI-TARS-Desktop-ByteDance-Multimodal-AI-Agent/); [agentconn](https://agentconn.com/agents/ui-tars-desktop) (secondary; the Electron version varies between sources)
- UI-TARS-1.5 OSWorld: 24.6 (50 steps) and 22.7 (15 steps), against Claude's 22.0 / 14.9 at that time (2025). — [a2a-mcp.org](https://a2a-mcp.org/blog/what-is-ui-tars.md) (secondary)
- The UI-TARS-2 report (arXiv 2509.02544, Sep 2025) describes:
  - a data flywheel
  - stabilized multi-turn RL
  - a **hybrid GUI environment with file system and terminal**
  - a unified sandbox for large-scale rollouts
  - a **hybrid action space** (GUI actions plus SDK functions: terminal commands and MCP calls)

  RL added +10.5% on OSWorld and +8.7% on AndroidWorld. It also scored 45.3 on Terminal Bench and 68.7 on SWE-Bench Verified. — [arXiv 2509.02544](https://arxiv.org/pdf/2509.02544); [themoonlight review](https://www.themoonlight.io/en/review/ui-tars-2-technical-report-advancing-gui-agent-with-multi-turn-reinforcement-learning)

**Simular: Agent S2 / S2.5 / S3**
- It is **Python** (`pip install gui-agents`) and runs on Linux, macOS and Windows, single monitor only. — [GitHub simular-ai/Agent-S](https://github.com/simular-ai/Agent-S)
- Architecture:
  - The main agent (`AgentS3`) has an optional reflection agent.
  - The grounding agent (`OSWorldACI`) converts actions into executable Python using a **separate grounding model**.
  - An optional **code agent** (`--enable_local_env`, `call_code_agent`) runs Python/Bash, with a 30 s Bash timeout.
  - Actions are Python strings executed via `exec`, with `pyautogui` imported.
  - Perception is screenshots only, keeping 8 image turns by default.

  — [GitHub Agent-S](https://github.com/simular-ai/Agent-S)
- Models:
  - Main model: OpenAI, Anthropic, Gemini, OpenRouter, Azure or **vLLM**, with GPT-5 (2025-08-07) recommended.
  - Grounding model (required): **UI-TARS-1.5-7B** recommended, at 1920x1080 grounding dimensions (1000x1000 for UI-TARS-72B).

  — [GitHub Agent-S](https://github.com/simular-ai/Agent-S)
- Results:
  - 2025-10-02: S3 scored 62.6% alone and 69.9% with bBoN (100-step).
  - 2025-12-15: S3 scored 66% alone and **72.6% with bBoN**, "first to surpass human-level", against a human baseline of about 72%.
  - WindowsAgentArena: 50.2% → 56.6% with best-of-3.
  - AndroidWorld: 68.1% → 71.6%.
  - 2026-08-28: Simular's hosted agent "Sai" reported 73% on **OSWorld 2.0**, a different benchmark.

  — [GitHub Agent-S](https://github.com/simular-ai/Agent-S); [PyPI gui-agents](https://pypi.org/project/gui-agents); [Simular blog](https://www.simular.ai/zh/articles/agent-s3)

**XLang: OpenCUA**
- OpenCUA (arXiv 2508.09123, NeurIPS 2025) is an open framework with annotation tools, the **AgentNet** dataset (3 OSes, 200+ apps/sites), and models.
- OpenCUA-72B reached 45.0% on OSWorld-Verified, open-source SOTA at release and ranked #1 among open models in Oct 2025. It scores 60.8% on ScreenSpot-Pro and 37.3–37.4% on UI-Vision.

  — [arXiv 2508.09123](https://arxiv.org/abs/2508.09123); [opencua.xlang.ai](https://opencua.xlang.ai/); [GitHub xlang-ai/OpenCUA](https://github.com/xlang-ai/OpenCUA)

**Cua (trycua)**
- Languages: a **Rust** core for the `cua` SDK/CLI, **Python** (SDK, `cua-sandbox`, `cua-bench`), **TypeScript** (`@trycua/cua`), **Swift** (Swift SDK and the macOS Spaces app), Kotlin bindings, and a **Tauri app for Linux/Windows** in the Spaces core. — [GitHub trycua/cua](https://github.com/trycua/cua)
- Components:
  - **Lume** creates and manages macOS and Linux VMs on Apple Silicon via Apple's **Virtualization.framework**.
  - **lumier** is a Docker-compatible interface to Lume.
  - **Cua Spaces** is a desktop app for agent desktops (local macOS VMs, Linux/Omarchy images, or the user's AWS/GCP/Modal).
  - **Cua Driver** operates native apps and browsers via CLI, MCP or typed SDKs on macOS, Windows and Linux.
  - `cua-spacesd` is an in-sandbox gRPC daemon on port 3211 that handles processes, files, screenshots, input and streaming.
  - Linux sandboxes run as **gVisor** containers.

  — [GitHub trycua/cua](https://github.com/trycua/cua)
- `cua-computer-server`, `cua-agent` and `cua-som` are now **deprecated**. The agent role moved to "a coding agent using the Cua Driver MCP" or `sandbox.agents()`. — [GitHub trycua/cua](https://github.com/trycua/cua)
- Models: bring your own. **CUA-S1** is a family of small specialized models for bounded decisions; its first profile, CUA-S1-FORMS, scores form-filling choices from structured UI elements. An optional `cua-perception` extension adds OmniParser and PP-OCR (AGPL-3.0). — [GitHub trycua/cua](https://github.com/trycua/cua)

**browser-use**
- It is **Python** (≥ 3.11), with TypeScript projects as well.
- It controls a local browser, Browser Use Cloud, or any remote browser over **CDP**.
- Models: OpenAI, Anthropic and Google wrappers, a `ChatBrowserUse` gateway, its own **BU2 (`bu-2-0`)** model, and local models via Ollama.
- It implements all 31 actions of Claude's browser toolset and includes Bash.
- The page gives no latency figures.

  — [GitHub browser-use](https://github.com/browser-use/browser-use)

**Microsoft: UFO2 / UFO3 Galaxy, Fara-7B**
- UFO is Python (3.10/3.11).
- **UFO²** (v2.0.0, Apr 2025, LTS) runs on Windows only:
  - A HostAgent coordinates AppAgents.
  - **Hybrid control detection** combines visual and UIA signals.
  - Actions mix GUI and API: native UIA, Win32 and WinCOM.
  - **Speculative multi-action batching cuts LLM calls by 51%.**
- **UFO³ Galaxy** (Nov 2025): a ConstellationAgent breaks tasks into a DAG and a TaskOrchestrator runs it. It covers Windows, Linux and Android, using MCP servers for tools.
- Models: OpenAI and Azure, with guides for Qwen, Gemini and Claude.

  — [GitHub microsoft/UFO](https://github.com/microsoft/UFO)
- **Fara-7B** (2025-11-24) is built on Qwen2.5-VL-7B and is screenshot-only (no AX tree or HTML).
  - Input: the last 3 screenshots plus action history. Output: click coordinates or keystrokes.
  - WebVoyager: 73.5%, against GPT-4o SoM agent at 65.1% and UI-TARS-1.5-7B at 66.4%.
  - Trained on 145k synthetic trajectories from a Magentic-One-style Orchestrator + WebSurfer.
  - It pauses at "critical points" (checkout, payment, login).
  - It targets on-device use, including NPUs.

  — [Microsoft Azure AI Labs](https://labs.ai.azure.com/projects/fara-7b/); [VentureBeat](https://venturebeat.com/ai/microsofts-fara-7b-is-a-computer-use-ai-agent-that-rivals-gpt-4o-and-works); [The Decoder](https://the-decoder.com/microsoft-unveils-fara-7b-a-compact-model-for-running-ai-driven-computer-control-locally/)

**Open Interpreter (OS mode)**
- It has a local code interpreter plus "OS Mode", which controls the OS visually with mouse and keyboard, and a Computer API (screenshots, browser). The source is an architecture write-up from Feb 2024. — [sean.lyn.ch](https://sean.lyn.ch/how-its-built-open-interpreter/); [Open Interpreter docs](https://docs.openinterpreter.com/guides/demos)

**H Company: Holo / Surfer**
- **Holo4** (2026-09-28) comes in two sizes:
  - Holo4-27B (dense, built on Qwen3.8-27B, weights CC BY-NC 4.0)
  - Holo4-35B-A3B (MoE, built on Qwen3.6-35B-A3B, **Apache 2.0**)

  Reported scores: OSWorld 85.2% (27B) and 80.8% (35B-A3B); OSWorld 2.0 61.7% (27B) and 30.9% (MoE). One summary gives about $0.08 per task. H itself warns the harnesses differ between compared systems. Trajectories are published at trajectories.hcompany.ai and on Hugging Face. — [aiweekly](https://aiweekly.co/alerts/h-company-ships-holo4-agents-27b-hits-617-on-osworld-20); [gradually.ai Holo4-35B-A3B](https://www.gradually.ai/en/ai-models/holo4-35b-a3b/); [featherless (HF mirror)](https://featherless.ai/models/Hcompany/Holo4-35B-A3B); [ai-tldr](https://ai-tldr.dev/releases/sam-witteveen-holo4-oct06/) (all secondary)
- Holo 3 was released in 2026. — [Wikipedia: H (company)](https://en.wikipedia.org/wiki/H_(company))

**Perplexity Comet**
- It is a Chromium-based browser. A backend model plans and issues commands, the sidebar is loaded from a Perplexity page, and **custom Chrome extensions** drive the page, relaying structured messages. — [Zenity Labs reversing](https://labs.zenity.io/p/perplexity-comet-a-reversing-story); [dev.to](https://dev.to/dhrn/the-untold-story-of-comet-browser-1k3e); [SiliconANGLE 2025-07-09](https://siliconangle.com/2025/07/09/perplexity-introduces-comet-browser-ai-powered-automation-tools/)

**Manus (now Meta)**
- The cloud agent runs in a 24/7 cloud sandbox VM. The E2B / Firecracker microVM claim comes from a third-party page and is unconfirmed. — [Manus blog](https://manus.im/blog/manus-sandbox); [fast.io](https://www.fast.io/resources/manus-ai-e2b-integration.md)
- **"My Computer"** (Mar 2026) ships in the Manus Desktop app for **macOS and Windows**. It runs a CLI in the user's terminal to read and edit files and launch and control apps. It is not sandboxed, and relies on "Allow Once" / "Always Allow" approval gates. — [GIGAZINE 2026-03-17](https://gigazine.net/gsc_news/en/20260317-manus-my-computer); [pulse2](https://pulse2.com/manus-ai-platform-launches-my-computer-to-turn-local-machines-into-autonomous-agents/amp/); [learnagentic](https://learnagentic.substack.com/p/manus-escaped-the-cloud-heres-why)

**Apple-related**
- **Ferret-UI Lite** (arXiv 2509.26539) is a **3B on-device GUI agent** for mobile, web and desktop.
  - Grounding: ScreenSpot-V2 91.6%, ScreenSpot-Pro 53.3%, OSWorld-G 61.2%.
  - Navigation: AndroidWorld 28.0%, OSWorld 19.8%.
  - It uses **inference-time cropping and zooming**, SFT and RL.
  - It is weak on long multi-step tasks (secondary source).

  — [Apple ML Research](https://machinelearning.apple.com/research/ferret-ui); [9to5Mac](https://9to5mac.com/?p=1040206); [abit.ee](https://abit.ee/et/tehisintellekt/apple-ferret-ui-lite-ai-agent-iphone-on-device-ai-gui-agent-artificial-intelligence-mobile-interface-et)
- A report says Apple is building a local agent to operate iPhone apps (2026-08-02). — [3DNews](https://3dnews.ru/1137224/apple-sozdayot-lokalnogo-iiagenta-dlya-iphone-kotoriy-smoget-upravlyat-prilogeniyami-za-polzovatelya)

**Leaderboard context (OSWorld-Verified, 2026)**
- BenchLM (2026-08-22) lists Qwen3.8 Max at 86.1%, then Claude Mythos 5 and Claude Fable 5 at 85%.
- Yutori (2026-08-07) lists an "Intelligence-Indeed Agent" at 90.2% (a system, not a model) and claude-fable-5 at 86.0%.
- The human baseline is about 72.4%.
- OSWorld 2.0 is not comparable to OSWorld-Verified.
- Scores reflect the whole model plus agent system.

— [BenchLM](https://www.benchlm.ai/benchmarks/osworld-verified); [Yutori](https://yutori.com/leaderboards/osworld-verified.md); [Steel leaderboard](https://leaderboard.steel.dev/leaderboards/osworld/)

### Inferences
- Language choice follows purpose. Python dominates research harnesses because of pyautogui, the vLLM/HF ecosystems and OSWorld tooling. TypeScript/Electron is used where a web UI and browser operators matter (ByteDance). Native or Rust stacks appear when a product needs a fast local runtime and VM control: Cua's Rust core with Swift for Virtualization.framework, and OpenAI's native macOS Codex app reading the AX tree. No major agent found uses GPUI. A Rust desktop app like Kai is unusual; the closest peer is Cua (Rust + Tauri + Swift).
- The open-weight model landscape for a local agent in Oct 2026 has three tiers. Holo4-35B-A3B (Apache 2.0, MoE with 3B active, a good fit for Apple Silicon or consumer GPUs) is the strongest permissively licensed open model reported. UI-TARS-1.5-7B and Fara-7B are the 7B screenshot-only options. OpenCUA-72B is too large for most local machines. These are vendor numbers.
- Every open model listed outputs coordinates from screenshots. Gemini and Agent S grounding make DPI/resolution rescaling a first-class concern (normalized 0–1000 for Gemini; a fixed grounding resolution for UI-TARS).

### Gaps
- **Project Mariner**: found no 2026 primary source on its current architecture (cloud VM browser, Chrome extension). It appears to have been folded into Gemini agent mode / Gemini 3.5 computer use, but this is not verified.
- **ChatGPT agent** (Jul 2025: visual browser + text browser + terminal on a cloud VM): this search did not retrieve a primary source, so the component list is unverified here.
- **OpenAI Atlas** browser and **Agent S3 judge internals** (how bBoN compares trajectories with "behavior narratives"): not fetched from the paper (arXiv 2510.02250).
- No primary source confirmed browser-use's DOM serialization details, Open Interpreter's 2026 status, or the Electron version of UI-TARS-desktop.
- Holo4 numbers come only from secondary sources, not hcompany.ai or the HF model cards.
- No notable new 2026 open-source desktop agent written in Rust/Tauri turned up beyond Cua Spaces.

## Q2. Which architectural patterns correlate with the best measured results?

### Takeaway
The top open-framework results come from **(1) a planner + specialized grounder split**, **(2) test-time scaling with a judge over multiple full rollouts (bBoN)**, and **(3) a hybrid action space (GUI plus code/terminal/MCP)**. Frontier closed models now reach about 78–86% OSWorld-Verified as single models. End-to-end RL-trained native models (UI-TARS-2, Holo4) are closing the gap.

### Cited Findings
- Agent S3: a GPT-5 planner with a UI-TARS-1.5-7B grounder and an optional code agent scored 66% alone. **bBoN raised it to 72.6%** on OSWorld. Best-of-3 added +6.4 points on WAA and +3.5 on AndroidWorld. — [GitHub Agent-S](https://github.com/simular-ai/Agent-S)
- The earlier SOTA before S3 was "GTA1 w/ GPT-5" at 63.4%, also a planner + grounder split. — [GitHub Agent-S](https://github.com/simular-ai/Agent-S)
- bBoN runs several parallel attempts and a judge picks the best. — [habr](https://habr.com/ru/post/953246); [LessWrong](https://www.lesswrong.com/posts/rft2jio6CzvmsvTua/behavior-best-of-n-achieves-near-human-performance-on)
- UI-TARS-2: multi-turn RL plus a hybrid GUI/terminal/MCP action space gave +10.5% on OSWorld. — [arXiv 2509.02544](https://arxiv.org/pdf/2509.02544)
- UFO²: hybrid GUI + native API actions plus speculative multi-action batching (−51% LLM calls). — [GitHub UFO](https://github.com/microsoft/UFO)
- CUA's OSWorld score improves with more allowed steps (test-time scaling). — [OpenAI CUA](https://openai.com/index/computer-using-agent/)
- Ferret-UI Lite: inference-time crop/zoom lets a 3B model reach 53.3% on ScreenSpot-Pro. Anthropic also ships a `zoom` action. — [Apple ML Research](https://machinelearning.apple.com/research/ferret-ui); [Claude docs](https://platform.claude.com/docs/en/agents-and-tools/tool-use/computer-use-tool)
- Holo4 is a single RL-trained model with a reported 85.2% OSWorld (vendor figure, harness caveats). — [aiweekly](https://aiweekly.co/alerts/h-company-ships-holo4-agents-27b-hits-617-on-osworld-20)

### Inferences
- For a local open-weight agent, the cheapest proven gains are:
  - a separate small grounding model (or zoom/crop re-grounding)
  - code/shell actions next to GUI actions
  - multi-action batching per model call

  bBoN multiplies compute by N and needs isolated parallel environments (VMs), so it fits benchmarks and cloud better than a live single desktop.
- Pure Set-of-Marks / AX-tree prompting is not what the top OSWorld entries use. They use screenshot plus learned grounding. Shipping products do still use AX/UIA trees for speed and robustness: Codex on macOS, UFO² on Windows, Claude in Chrome via the DOM.

### Gaps
- No controlled ablation was found that isolates the planner/grounder split from model strength in 2026. Leaderboards mix harnesses and step budgets.
- The rollout count and the judge's cost for bBoN on OSWorld were not confirmed.

## Q3. macOS vs Linux handling

### Takeaway
macOS is handled either natively through the **AX tree plus a background cursor** (Codex) or through **Virtualization.framework VMs** (Cua Lume). Linux is handled through **Xvfb/Docker** (Anthropic reference) or **gVisor containers** (Cua). Retina 2x scaling is the main macOS pitfall that vendors document.

### Cited Findings
- Codex computer use is macOS-first and reads the accessibility (AX) hierarchy of any open app. It runs in the background without taking over the user's desktop. — [mjtsai.com](https://mjtsai.com/blog/2026/04/17/); [Daniel Vaughan](https://codex.danielvaughan.com/2026/04/17/codex-app-computer-use-macos-background-gui-automation/)
- Cua Lume runs macOS/Linux VMs on Apple Silicon via Virtualization.framework, with a Swift macOS Spaces app. Linux sandboxes use gVisor, and Linux/Windows get a Tauri app. — [GitHub trycua/cua](https://github.com/trycua/cua)
- Anthropic: the Linux reference is Docker + Xvfb + Mutter + Tint2. On macOS Retina, screenshots must be downscaled by 2 or coordinates halved. — [Claude docs](https://platform.claude.com/docs/en/agents-and-tools/tool-use/computer-use-tool)
- Agent S runs on Linux, macOS and Windows via pyautogui, single monitor only. — [GitHub Agent-S](https://github.com/simular-ai/Agent-S)
- UI-TARS-desktop lists Windows/macOS (no Linux). UFO³ adds Linux via MCP servers. Manus My Computer supports macOS and Windows only. — [UI-TARS-desktop](https://github.com/bytedance/UI-TARS-desktop); [UFO](https://github.com/microsoft/UFO); [GIGAZINE](https://gigazine.net/gsc_news/en/20260317-manus-my-computer)

### Inferences
- Linux desktop support is the weakest area among consumer agents. Several (UI-TARS-desktop, Manus, Codex) skip it. Linux is mostly a *sandbox/benchmark* target (OSWorld, Xvfb). A Linux-native desktop agent using AT-SPI is a gap in the market.

### Gaps
- None of the sources detail how agents handle macOS TCC permissions (Screen Recording / Accessibility prompts) or Wayland screen capture on Linux. This needs OS documentation research.

## Q4. Speed techniques

### Takeaway
The levers are fewer model calls (batching or multi-action outputs), smaller images (resolution caps, pruning), prompt caching, structured trees instead of pixels where available, and small or MoE grounding models.

### Cited Findings
- Anthropic: batch actions per turn, screenshot-pruning cadence to preserve prompt caching, a recommended 1024x768–1280x800 resolution, and about 1–1.8k tokens per screenshot. — [Claude docs](https://platform.claude.com/docs/en/agents-and-tools/tool-use/computer-use-tool)
- UFO²: speculative multi-action execution cuts LLM calls by 51%. — [GitHub UFO](https://github.com/microsoft/UFO)
- Gemini 2.5 CU is claimed to have "lower latency" than alternatives on browser benchmarks (Browserbase environment). No absolute numbers were published in the sources found. — [Google blog](https://blog.google/technology/google-deepmind/gemini-computer-use-model/)
- Fara-7B uses only the last 3 screenshots of context. — [The Decoder](https://the-decoder.com/microsoft-unveils-fara-7b-a-compact-model-for-running-ai-driven-computer-control-locally/)
- Agent S keeps 8 image turns by default. — [GitHub Agent-S](https://github.com/simular-ai/Agent-S)
- Holo4-35B-A3B is MoE with about 3B active parameters, at a reported $0.08 per task for the 27B. — [gradually.ai](https://www.gradually.ai/en/ai-models/holo4-35b-a3b/); [ai-tldr](https://ai-tldr.dev/releases/sam-witteveen-holo4-oct06/)
- Claude in Chrome "Quick Mode" is a compact command language, claimed to be about 3x faster (unconfirmed). — [pasqualepillitteri.it](https://pasqualepillitteri.it/en/news/346/claude-chrome-quick-mode-fast-browsing-guide)
- CUA-S1 small models score bounded decisions over structured elements instead of generating free-form text. — [GitHub trycua/cua](https://github.com/trycua/cua)

### Inferences
- For Kai (Gemma 4 by default, local), the most transferable speed ideas are:
  - keep the screenshot history short (about 3)
  - cap resolution and rescale coordinates
  - batch multiple actions per model call
  - keep the system prompt byte-stable for prefix caching
  - use the AX/AT-SPI/UIA tree when it is present, and fall back to pixels plus grounding

### Gaps
- No source gave measured per-step latency (seconds per step) for any agent. Treat all "faster" claims as unquantified.
