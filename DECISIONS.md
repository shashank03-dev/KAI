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
