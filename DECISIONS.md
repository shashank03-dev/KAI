# DECISIONS.md

Every assumption, every researched fact that changed the plan, and every place
the build deviates from `KAI-BUILD-PROMPT.md` and why. Append-only; newest notes
at the bottom of each section.

## Fixed decisions (resolved)

- **Primary OS:** **Linux** (with Windows as the equal first-class target). The
  original prompt offered `macOS | Windows`; the repo owner's instruction for
  this build is explicit: *"we will primarily make it for Linux and Windows."*
  So Kai targets **Linux and Windows** as the two shipping platforms. macOS code
  paths are kept compiling where cheap (they share the `enigo`/`keyring`/`image`
  stack) but are **not** a release target and are not claimed to be flawless.
  This is a deliberate deviation from the prompt's "primary = demo laptop macOS"
  framing, taken at the owner's direction.
- **Gemma 4 models:**
  - Gemini API ids: `gemma-4-26b-a4b-it` (MoE, ~4B active, default/fast) and
    `gemma-4-31b-it` (dense fallback, harder screens).
  - vLLM / Hugging Face ids: `google/gemma-4-26B-A4B-it` and
    `google/gemma-4-31B-it`. Source: the official vLLM recipe
    (recipes.vllm.ai/Google/gemma-4-26B-A4B-it).
  - **vLLM version:** `0.25.0` or later is required for Gemma 4 with images.
    (The recipe's nightly-only features — MTP speculative decoding — are not
    used.) Pinned in `cloud-init.yaml`.
- **Language:** Rust 2024, one Cargo workspace. ✔
- **UI:** GPUI + Ely GPUI Components, pinned to commit
  `b0dbfd429c17819f6c143fd365852244d497514a` (not a branch). gpui is the
  `gpui-pre`/`gpui-pre-platform` crates.io snapshot at `=0.3.8` — the exact
  snapshot Ely itself pins (see Ely AGENTS.md, 2026-10-07 decision). We follow
  Ely's pin so the two gpui crates are the same build.
- **License:** MIT (`LICENSE`). ✔
- **Team name / region:** Team `Kai` / DigitalOcean region **`nyc2`**. The owner
  asked for `blr1`; as of the availability page (verified 2026-08-10) H100 GPU
  Droplets are offered only in **NYC2, AMS3, TOR1** — Bangalore (`blr1`) has no
  H100. We default to `nyc2` (nearest-to-US judges / lowest latency for the
  demo) and make the region a variable in `up.sh` so it can move to `ams3`/`tor1`.
  `up.sh` still runs `doctl compute region list` / `size list` and refuses to
  proceed if the chosen region lacks the GPU size, so a stale assumption fails
  loudly instead of silently.

## Phase 0 — learned before writing

### Gemini API (Gemma 4) — trust the team's prior lessons, plus:

- Ask for JSON with `responseMimeType: "application/json"` and put the schema in
  the prompt. **Do not use forced function calling** (it hung in the team's
  testing). We follow this.
- `thinkingConfig`: for Gemma 4 on the Gemini API, thinking is controlled by
  `thinkingLevel`. The team's note said "minimal"; Google's current Gemma-on-
  Gemini docs document `"high"` to *enable* thinking and report that some Gemma
  builds reject turning it off over REST. **Decision:** send
  `thinkingConfig: { thinkingLevel: "low" }` (lowest latency level the API
  accepts for Gemma 4). If a request is rejected for the thinking field, the
  `GeminiClient` **retries once without `thinkingConfig`** and remembers that for
  the rest of the process (a `OnceCell<bool>` "thinking supported" flag). This
  matches the prompt's "retry without it and remember" instruction.
- Endpoint: `POST v1beta/models/{model}:generateContent`, images as
  `inlineData { mimeType, data(base64) }`, `systemInstruction` for the stable
  system prompt. Validate and clamp every field that comes back.

### vLLM (DigitalOcean backend)

- OpenAI-compatible `/v1/chat/completions` with `image_url` parts
  (`data:image/jpeg;base64,...`). `--api-key` from a generated secret.
- **Structured outputs:** vLLM supports `response_format: { type: "json_schema",
  json_schema: {...} }` (xgrammar/guidance backend). We send Kai's action schema
  there so the vLLM reply is always schema-valid — no repair retry needed on this
  backend. (The Gemini backend has no equivalent, hence JSON-mode + one repair.)
- Serve flags we use (from the recipe, trimmed for an agent that sends one
  screenshot + one optional crop):
  `--max-model-len 16384 --gpu-memory-utilization 0.90
   --limit-mm-per-prompt.image 2 --enable-prefix-caching --async-scheduling`
  plus `--served-model-name gemma-4-26b-a4b-it` so our client uses the same id on
  both backends. Vision token budget left at the 280-token default per image.
- Prefix caching is on; our prompt is laid out stable-first (system prompt + tool
  schema) and changing-last (screenshot, element list, last 3 actions) so the
  prefix hits every step.

### Agent Skill Open Standard (agentskills.io)

- A skill = a directory with `SKILL.md` (YAML frontmatter + Markdown body),
  optional `scripts/`, `references/`, `assets/`.
- Frontmatter: `name` (required, ≤64, `[a-z0-9-]`, no leading/trailing/double
  hyphen, **must match directory name**), `description` (required, 1–1024),
  optional `license`, `compatibility` (≤500), `metadata` (string→string map),
  `allowed-tools` (space-separated, experimental).
- Progressive disclosure: only `name`+`description` at startup (~100 tokens);
  full body on activation (<5k tokens recommended, keep `SKILL.md` ≤500 lines);
  `scripts/`/`references/`/`assets/` loaded on demand.
- **Kai's spec-compatible extension:** Kai reads an optional `kai` key inside
  `metadata` holding a JSON array of deterministic `steps` (open_url, shortcut,
  type) that run **without** a model call. This lives entirely inside the spec's
  `metadata` string-map (we store it as a JSON string), so a Kai skill is still a
  valid Agent Skill for any other agent — they just ignore the extra metadata.
  Documented in `docs/SKILLS.md`.
- `kai skill validate <dir>` enforces every rule above. CI runs it on all shipped
  skills.

## What was NOT run for real in this environment (be honest)

This build was produced in a Linux CI-style container with **no GPU, no
DigitalOcean access, no macOS, and no real display server**. Therefore:

- `deploy/digitalocean/up.sh` / `smoke.sh` / `down.sh` were **written and
  shell-linted (`bash -n`, shellcheck where available)** but **not executed
  against real DigitalOcean**. The vLLM request/response shape is exercised
  against `MockClient` and a local mock HTTP server in tests, not a real H100.
- Benchmark numbers from a real Droplet are **not** committed as measured fact.
  `bench/results/` ships a **`mock` backend** run only, clearly labelled, so the
  table and plumbing are real and reproducible; the README states plainly that
  vLLM/Gemini/OpenAI numbers were not measured here and must be filled in on the
  demo laptop. **No number is claimed that was not measured.**
- The GPUI UI is built against Ely's real APIs (read from the pinned source), and
  the core/OS/CLI crates compile and test on Linux. Screenshots of every UI
  state must be captured on the demo machine; the harness/paths are in place.

## Cost reminder

A single NVIDIA H100 GPU Droplet is ~US$3.39/hour (DigitalOcean pricing page,
2026) and **bills while it exists, even idle/powered-off**. `up.sh` prints this
and reminds the operator to run `down.sh`. Only `down.sh` (destroy) stops
billing.

## Deviations log

- 2026-10-09: Primary OS switched to Linux+Windows per owner instruction (see
  above). macOS demoted to best-effort-compile, not a release target.
- 2026-10-09: Region default `nyc2` not `blr1` (no H100 in `blr1`).
- 2026-10-09: Gemini thinking set to `low` with retry-and-remember fallback,
  refining the team's "minimal" note to the levels the current API documents.

## Phase 2b — kai-os (measured on this machine)

- **Measured observe cost** (Xvfb 1920x1080, `gtk3-widget-factory`, real AT-SPI
  tree with 81 marks, release build, 4 vCPU container, no GPU):
  capture **19–28 ms**, accessibility tree **136–198 ms**, downscale + marks +
  JPEG **41–52 ms**, JPEG **~128 KB**. Capture and the tree run in parallel, so
  observe ≈ max(capture, tree) + encode ≈ **~200 ms**. The tree read has a hard
  600 ms budget; past it Kai uses the grid. Evidence: `docs/img/set-of-marks-gtk.jpg`
  (exactly the image the model receives).
- **Mark tags sit above each box**, not inside it. The first capture put tags
  inside and hid the first letters of labels ("~~Combo~~boxentry"); moved after
  reviewing the image.
- **Grid cells are numbered** (1 = top-left, row by row) instead of the prompt's
  "D7" letters, so the model always answers with one integer `target` in both
  modes, and the 3x5 bitmap digit font needs no font file.
- **Excluding Kai's own windows from capture:** Windows uses
  `SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE)` (set by the app on its
  HWNDs). **Linux/X11 has no equivalent API**, so the app reports its window
  rectangles and `kai-os` paints over them before the frame is hashed or sent.
- **Wayland:** screen capture works through the desktop portal (one prompt per
  session); synthetic input reaches only XWayland apps. Onboarding reports this
  as "Limited" and tells the user to pick an Xorg session for full control.
  Not hidden, not faked.
- **Kill switch hotkey:** `Ctrl+Alt+Esc` by default on both OSes; if it is
  already grabbed (KDE binds it to xkill) Kai falls back to `Ctrl+Alt+End` and
  says so. Pointer in the top-left 3 px also stops. Tested: a running shell
  command is killed with its whole process group within ~150 ms.
- **Shell:** `sh -c` in its own process group (Linux), PowerShell with
  `taskkill /T` (Windows); 180 s timeout, 12 KB output cap per stream.
- **URLs:** only `http`, `https`, `mailto`; bare hosts get `https://`.
  `javascript:`/`file:` are refused.
- **Windows build** is type-checked from Linux with `x86_64-pc-windows-gnu`
  (clippy clean). The msvc target needs MSVC's C toolchain for `ring`, so CI on
  a Windows runner is the real proof; see `.github/workflows/ci.yml`.
- **Zoomed crop:** the `crop_jpeg` slot and `frame::crop` exist, but the loop
  does not yet request a crop automatically after a missed grid click. Listed
  as remaining work.

## Phase 2c — CLI, skills, benchmark (measured)

- `kai skill validate skills` — **9/9 skills pass** (6 required + `run-terminal-task`,
  `own-site-security-check`, `pc-security-checkup`).
- Security skills are deliberately **defensive and read-only**: a public
  configuration review of a site the user confirms they own (certificate,
  redirects, security headers, SPF/DMARC/CAA) and a posture check of the user's
  own PC (firewall, updates, listening services, encryption). No port scanning,
  probing or attack payloads; every fix is approved first.
- Three bench modes, labelled in every results file:
  - `mock` — recorded replies, scripted screen, dry-run hands. Plumbing only;
    its ~0 ms timings are **not** a speed claim. 10/10.
  - `replay` — recorded replies **executed for real** on a desktop (real
    capture, AT-SPI, shell), judged by each task's real check command.
    Measured on Xvfb: **5/5 terminal tasks pass; Kai's own per-step overhead
    p50 65 ms, p95 93 ms** (model time excluded — there is no model in replay).
    GUI tasks need a real model, since recorded marks won't match a live screen.
  - `vllm` / `gemini` — the real thing. **Not run here** (no GPU Droplet or
    Gemini key in this environment). Run on the demo laptop and commit.
- Shell steps first cost a flat ~51 ms from a 50 ms poll; switched to waiting
  on exit with `select!`, which took step overhead from p50 106 → 65 ms.
- Skill selection ignores stopwords, so "open the … and …" no longer picks a
  skill on filler words alone.
- No OpenAI comparison runner: no key was supplied, so per the prompt the README
  says the comparison was not run.

## Phase 3/4 — design and the app

- **English only.** The prompt suggested a Kannada title and greeting; the
  owner asked for English only, so the app is "Kai" and every string is
  English. (Ely's `I18n` also has no Kannada locale at the pinned commit.)
- **Component names mapped to Ely's real homes** at the pinned commit; full
  table in `docs/DESIGN.md` (e.g. NavigationRail → `shell::ActivityBar`,
  StatCard → `data_display::Statistic`, Result → `feedback::ResultView`).
- **Approval dialog** is composed on `overlays::Dialog` (what Ely's own
  `ToolApprovalDialog` is built on) because the run needs Stop / Skip /
  Approve and `ToolApprovalDialog` only has Run / Deny. Escape, the scrim and
  Stop all stop. The mini window shows the same three answers inline, since the
  main window is minimised during a run.
- **SystemNotification** is unfinished in Ely (`prove`); Kai uses the mini
  window's amber state and a toast instead. TrayIcon is macOS-only in Ely and
  not used.
- **Shortcut cheat-sheet** opens on `Ctrl+/` and `F1`, not `?`: a root binding
  on `?` would swallow the character typed into the composer.
- **Kai saffron accent** replaces only Ely's `accent` token: light `#B45309`
  (white label 5.0:1, on page 4.8:1), dark `#F59E0B` (dark label 7.1:1, on page
  7.1:1); a test holds both modes at WCAG AA. Status colours stay Ely's.
- **Save as skill** drafts the `SKILL.md` locally from the trace (leading
  URL/app/shortcut steps become deterministic steps, the rest becomes
  instructions); the user edits it beside a live preview and spec validation.
  It does not yet ask Gemma to polish the draft.
- **Voice input** is not shipped: no speech-to-text was implemented, so the
  composer has no voice button (as the prompt requires).
- **Zoomed crop** on a missed grid click is still not requested automatically.

## Screenshots — captured (correction, 2026-10-09)

The earlier note here blamed Xvfb/Vulkan/GLX for black frames. That was wrong.
The real cause: `kai-app` built `gpui_platform` **without its `x11` and
`wayland` features**, so gpui-pre-linux ran its headless-only platform and
never opened a window at all (`xwininfo -root -tree` showed no children). It
also means the earlier Linux builds of the app never showed a window. With the
features on (`crates/kai-app/Cargo.toml`), the app renders on TigerVNC's Xvnc
with Mesa's llvmpipe Vulkan driver, picked by wgpu with no env vars. All 25
screens/states in light and dark are in `screenshots/` (made-up run data, see
`screenshots/README.md`). Keyboard-only and mouse-only walkthroughs were still
not performed.

## Phase 5 — ship

- CI (`.github/workflows/ci.yml`) runs on Ubuntu 24.04 and Windows: fmt,
  clippy `-D warnings`, all tests, `kai skill validate skills`, mock bench.
  macOS is not a target (owner: Linux and Windows).
- Release (`release.yml`) on `v*` tags builds `kai-app` + `kai` and attaches
  `kai-linux-x86_64.tar.gz` and `kai-windows-x86_64.zip`. Unsigned; the README
  explains SmartScreen. Release `kai-app` on Windows uses the GUI subsystem.
- The repo is **private** at the time of writing; the hackathon requires it
  public — flip it before submission.
- Commits are authored by the repo owner's account with no co-author lines, at
  the owner's request.

## Round 2 (2026-10-09) — speed, comparison, control centre

### "JEV" = Jev, not V-JEPA 2
The owner confirmed "JEV" is **Jev** (`typesafe-ai/jev`), TypeSafe AI's
"System One" decision model on the Vercel AI Gateway: text-only, typed answers
(boolean/"noul" probability, choice, score), $0.042 per M input tokens. Kai
implements it as a `Verifier` (`client/jev.rs`). **It could not be measured:**
the supplied Gateway key is free tier and every Jev call returns 403 ("Free
tier users do not have access to this model"). The verifier turns itself off on
that error. It stays **off by default** until it can be measured on a paid key.
The fallback judge (`LlmVerifier`, `openai/gpt-4.1-nano`) was measured at p50
803 ms / p95 1494 ms per call (n = 10): slower than what it would save per
step, so it is also off by default.

### Vercel AI Gateway
- Used for **(a)** the GPT comparison models and **(b)** Gemma 4 26B-A4B
  itself, because no Droplet and no Gemini key were available. Not used for a
  per-step router model (latency above).
- Free-tier access measured 2026-10-09: works — `google/gemma-4-26b-a4b-it`,
  `openai/gpt-5`, `gpt-5-mini`, `gpt-5-nano`, `gpt-5.2`, `gpt-4.1(-mini/-nano)`,
  `gpt-4o(-mini)`, `o4-mini`, `google/gemini-2.5-flash(-lite)`; 403 —
  `gemma-4-31b-it`, `typesafe-ai/jev`, `gpt-5.4+`, `gemini-3+`. Rate limit
  5 requests/minute/model; the client waits out 429s and does not count the
  wait as model time (task wall time does include it — see the bench notes).
- `computer-use-preview` is **not** served by the Gateway (404), so OpenAI's
  native loop (`--backend openai-native`) needs a direct OpenAI key. It is
  implemented and tested against a mock server only. **Not run.**
- On the free tier, the careful slot (31B) is unavailable; Kai treats the 403
  as "unavailable" and stays on the fast model instead of failing the run.
- Gemma via the Gateway accepts `response_format: json_schema` but does not
  enforce it (it also wraps replies in code fences), so the schema goes into
  the prompt too and every reply is validated as before.

### Frames
Default frame size changed from "1280 px wide" to "≤ 645,120 px" (Gemma 4's
280-soft-token budget at 48x48 px per token): 1070x602 at 16:9. Measured: JPEG
148 → 117 KB with 81 marks; marks drawn at the final size stay crisp, while a
1280 frame downscaled by the server blurs the digits. Whether Gemma 4's
processor rounds to whole patches is undocumented; `KAI_FRAME_MAX_SIDE=1024`
gives an exactly patch-aligned 1024x576 if needed. `KAI_FRAME_MAX_PIXELS=0`
restores the old behaviour.

### Engine
- Multi-action steps via an optional `then` list (≤ 3) on the same JSON object,
  not an array reply, so old replies and both structured-output backends keep
  working. Follow-up fields are nullable so OpenAI strict mode can't force a
  bogus `target`.
- Tree-only steps on by default (full frame at least every 3rd step).
- Accessibility-first actions on by default; `type` through accessibility only
  fills **empty** fields (Kai's mouse path types at the caret, which equals
  "set contents" only when the field is empty).
- Plan cache used by `kai run` (and the app), **not** by `kai bench`, so
  benchmark runs never benefit from earlier runs of the same task.
- Live-run fix found with real Gemma: the history cut every command to 30
  characters, so the model saw `echo 'Hello from Kai' > ~/kai-` and re-ran the
  command 20+ times. Shell commands are now kept whole (≤ 300 chars) and a
  silent success is shown as `(no output) [exit 0 = success]`.
- Not done: sub-grid answers on the zoom crop are drawn but the prompt has no
  sub-cell answer format yet; screen-diff crops; a planner/executor split.

### App
- One run has the screen at a time; other runs queue. Pause holds between
  actions (never inside one). Steering goes into the run's queue and shows when
  Kai read it. Team runs can't be steered yet (the orchestrator gives
  sub-agents `NoSteering`).
- No file-change view: guessing touched files from command lines would be
  unreliable.
- Fixed: the global stop hotkey was never registered in the app; opening the
  shortcuts dialog crashed (double lease).

### Branches and commits
Work is pushed to both `ccr-904952eb-q0l216` (the owner's branch) and
`ccr-fc874938-ukt9kz` (this session's branch); they are identical. Commits are
authored by the repo owner's account with no co-author lines, at the owner's
request, so GitHub shows them as unverified (unsigned).

## Round 3 (2026-10-10) — live runs, head-to-head, foundation for training

### Keys and backends used
- Gemma 4 ran on the **Gemini API with two keys** supplied by the owner (env
  `GEMINI_API_KEY`, comma-separated; Kai rotates them per request and retries
  429/5xx on the next key). Keys were only ever passed as environment
  variables. Gemma 4 31B on the free tier is slow (single calls ~27 s p50) and
  often answers 500.
- GPT-5 mini and GPT-4.1 mini ran through the **Vercel AI Gateway** free tier,
  which throttled to ~1 request/min/model late in the run (60 s waits). Because
  of that, the GPT comparison uses a **14-task subset** covering every category;
  Gemma's numbers on exactly those tasks come from the same build.
- The 2-hour background limit cut Gemma 31B's full 30-task run at task 15; it
  was re-run on the 14-task subset instead.

### Kai bugs found by live runs, fixed (each made tasks fail for every model)
- Linux paste: the clipboard handle was dropped right after setting it, so text
  over 32 characters (pasted, not typed) never arrived on X11.
- "Nothing changed": a 9x8 dHash missed typing into a form field; replaced by a
  fine content hash plus the element list.
- The element list now shows text-field values (never for secure fields).
- Dropdowns: typing the option text picks it (+Enter); the same click is never
  batched twice; a click on a dropdown/menu forces a real screenshot.
- A URL already opened in the run is not reopened (a reload wiped the form).
- Shell: a background program holding the output pipe (`gio open`) hung the
  run forever; readers now get 0.5 s after exit. A shell step that just starts
  a GUI app runs it detached instead of blocking 180 s.
- Payment/password check read the model's own thought ("avoid
  password/payment") and matched "pay" inside words; now only typed text and
  the target's name, on word boundaries.
- Gemma sometimes streams only whitespace: aborted after 256 characters.
- Bench environment: Firefox restored ~15 old tabs and crashed-tab pages into
  every task; browser tasks now start clean. Firefox trusts the system CA store
  (the container's proxy CA) via p11-kit.
- Fairness: the release binary was not rebuilt between the four model runs, so
  all four ran the same build. The last three fixes above (GUI-app shell step,
  payment check, open_app needs a name) landed after the runs; the README notes
  which failures they caused.

### Measured (README has the table)
14-task subset, one run each: Gemma 4 26B **14/14** (75 steps, 104 calls),
Gemma 4 31B 11/14 (60 steps), GPT-5 mini 12/14 (212 steps), GPT-4.1 mini 8/14
(155 steps). Gemma 4 26B on all 30 tasks: 23/30. One run per model is not
statistically solid; docs/ROADMAP.md requires a 100+ task held-out set with
3 repeats before claiming a winner.

### Foundation for "Gemma beats frontier models over time"
- `docs/ROADMAP.md`: phased plan with targets and sources. GPT-6 Astra's
  published computer-use numbers are vendor-only (OpenAI system card,
  2026-09-03); Astra itself is 403 on the free Gateway key, so not measured.
- Recording: every `kai bench` task run is saved as training data under
  `bench/runs/` (gitignored); `kai run --record`; the app records only when
  Settings › Privacy › "Keep runs as training data" is on (default off, local).
- `kai data export|stats`, failure taxonomy, `kai bench variants`
  (training-only variants; never used for head-to-head numbers).
- `train/`: LoRA SFT, online GRPO with Kai as the environment and bench checks
  as rewards, eval, Droplet serving of adapters. Validated only by dry runs (no
  GPU here).
- Licensing: Gemma 4 is Apache 2.0. Google's Gemini API terms prohibit using
  the service to develop competing models, which likely covers Gemma *served
  through the Gemini API*; OpenAI's terms likewise. So the default training
  data is Gemma's own runs on the owner's Droplet; GPT/Gemini-API runs stay
  evaluation-only. Not legal advice — the owner should confirm.
