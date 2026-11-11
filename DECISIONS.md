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

## Round 4 (2026-10-12) — human checks: CAPTCHAs, one-time codes, questions

Design: [docs/HUMAN_CHECKS.md](docs/HUMAN_CHECKS.md). Research behind it:
[docs/RESEARCH_2026_10_FOUNDATIONS.md](docs/RESEARCH_2026_10_FOUNDATIONS.md) §6.

- **Kai never solves a CAPTCHA and never guesses a code.** The owner asked for
  "ask the user when the agent meets an OTP or a CAPTCHA, notify them, then
  proceed". Automated CAPTCHA solving circumvents a site's access control and
  is a losing race against behavioural detection anyway, so the design is a
  hand-off: the person at the computer does it, Kai waits and carries on.
- **The harness decides, not the model.** Detection reads the element list
  before every model call; a guard refuses any click on a CAPTCHA element and
  any `type` into a one-time-code field on all three action paths (model
  step, batched `then`, executor). Models can be talked into "solving it for
  accessibility"; a prompt rule alone would not hold.
- **Conservative detection over recall.** Whole-phrase matching on names;
  the invisible "protected by reCAPTCHA" badge, promo/ZIP/postal codes and
  "2-Step Verification" settings links never pause a run. A missed check is
  cheap (the model is told to `ask_user`, which now reaches the user); a
  false pause on every sign-in page would not be.
- **`ask_user` is a real question when someone can answer** (app, terminal).
  Headless/bench/piped runs keep the old behaviour (end as `AskedUser`) and
  get a new failure category `needs_human`, appended to the taxonomy.
- **Codes are typed by Kai only on request, only into the field the user
  saw** (re-found by name + role), only if they look like a code (4–12
  letters/digits), and are redacted from every later element list, the
  trace, UI events and memory. Screenshots can still show a code while it is
  on the page; documented, not hidden.
- **Waiting is the user's time**: excluded from the run's time limit,
  bounded by `human_wait` (10 min) and 6 asks per run, and the kill switch is
  polled while waiting.
- **Notifications**: Linux via the freedesktop notification service (zbus,
  already a dependency); Windows relies on the always-on-top mini window and
  the raised main-window dialog (no new Windows API surface without a Windows
  machine to test on).
- **Not built here**: the app crate could not be compiled in this session
  (the Ely UI dependency is not reachable from the sandbox); its changes
  mirror the approval dialog's patterns and are checked by CI. `cua.rs` (the
  experimental OpenAI native loop) is not wired to human checks.

## App UI/UX audit (2026-10-12)

Audit of every screen against `screenshots/` and the page code; fixes:

- Raw enum names reached the UI (`StepLimit` in the task list, "Run ended:
  StepLimit", `Gui`/`Shell` in the team plan, History's outcome line). All go
  through `state::outcome_text` now; a stopped run is amber, not red.
- Skill details: the YAML frontmatter rendered as one giant setext heading
  (Markdown reads the closing `---` as an underline), and `FileTree` showed only
  an empty search box. The sheet now shows the description, a plain file list
  and the SKILL.md body without frontmatter.
- Approval dialog said "This step is high risk" even under "Every action";
  it now says "Allow this step? Clicking “Send”", gives Kai's reason, and
  mentions high risk only when the step is marked so.
- Theme: the title-bar toggle, Ctrl+Shift+L and the palette didn't save the
  choice (Settings › Appearance and the next start disagreed with the screen);
  Appearance › System didn't follow the OS. Both fixed.
- Skill on/off switches were lost on restart: kept in
  `Settings::disabled_skills`.
- Settings content ran into the window's right edge: a 46 rem column with
  right/bottom padding.
- Model & server: Save gave no feedback (toast now, unless a key failed); a
  test of "This computer" had nowhere to show its result.
- Confirm before destructive actions: Memory › Clear all and removing a
  downloaded model (with the space it frees).
- History shows a hint instead of a blank pane when no run is picked; the
  queued view says "starts next" / "n tasks ahead" / "queue paused"; the
  status bar's offline tooltip gives the reason; the palette offers only the
  run controls that apply; Safety shows the registered stop shortcut as keys
  (or that none is registered); onboarding copy ("Your DigitalOcean server",
  local models mentioned).

Not verified by a build here: this container can't fetch the pinned Ely GPUI
Components commit (GitHub 401, codeload 403), so `kai-app` was not compiled or
re-captured. gpui calls were checked against the `gpui-pre 0.3.8` source and
Ely calls follow existing usages in the crate; kai-core builds, passes its
tests and clippy. Screenshots in `screenshots/` predate these fixes.

## Kai Glass review (2026-10-12)

Review of this branch against its own CI screenshots and code:

- **Save as skill was unusable.** The sheet put a split Markdown editor in a
  fixed-height box that never clipped: on the narrow sheet both panes shrank
  to a few words per line, the preview rendered the frontmatter as one giant
  heading, and the text ran over the Save button. Now: the source field, a
  preview of the body without frontmatter (as Skill details does), and Save,
  in one scroll area.
- **The home screen animated forever while idle.** The connection pill used
  the breathing `live_dot` whenever the model was "ready" (i.e. whenever
  Kai sits idle), so gpui redrew the window every frame; `anim.rs` itself
  promises nothing loops while idle. The pill now has a steady glow; the busy
  line still breathes during a run.
- **Approval JSON ran off the dialog.** Lines longer than 48 characters are
  shortened with "…"; the full reason is in the dialog text above it.
- **Screenshots workflow could commit to the default branch.** It skipped only
  `main`, which this repo doesn't use; pushes to the default branch are now
  skipped (a manual run there still works).
- `describe`'s doc comment had slid onto `outcome_text`; moved back.

## Ely vendored (2026-10-12)

- `ZacharyZhang-NY/Ely-GPUI-Components` stopped being cloneable: GitHub
  refuses it, and the API reports the owner/repo as not visible. Every build
  without a warm cargo cache failed, including the default branch's Ubuntu
  CI and any fresh checkout.
- With the owner's approval, the pinned revision `b0dbfd4` (MIT) was
  recovered from CI's cargo cache and committed unedited to
  `vendor/ely-gpui-component`. `Cargo.toml` now uses it as a path dependency
  and excludes it from the workspace. Provenance and how to go back to
  upstream: `vendor/README.md`.
- First full local build of the workspace, `kai-app` included: clippy `-D
  warnings` is clean. The lockfile change is only Ely's `source` line, so
  every other dependency version is unchanged.


## Verification: outcome ledger, DONE gate, per-action effect (2026-10-12)

Ranked plan items #1 and #2 from `docs/RESEARCH_2026_10_FOUNDATIONS.md` §7.
The design, guarantees and limits are in `docs/VERIFICATION.md`.

- **"The planning call Kai already makes at step 0".** The single-agent loop
  makes no separate planning call (only Team mode's orchestrator does). The
  outcomes are asked for in the run's **first model call**, the one that
  picks the first action, so no call is added. The request goes in that
  user turn, not in the system prompt, so the cached prefix stays the same.
  The first call gets +256 reply tokens to fit the outcomes.
- **`url_is` reads the observation, not the OS.** Kai has no browser-protocol
  access. The only OS-independent source of the URL is the address bar's
  accessibility value, and that is already in the element list. So
  `a11y_has`, `window_title` and `url_is` need no OS call, and the `Probe`
  port only serves `file_exists`, `file_contains` and `shell_ro`.
- **The window title** comes from the tree read Kai already does: the active
  window's AT-SPI name on Linux, `GetWindowTextW` on the foreground window
  on Windows. It is a new `Observation::window_title` field. No extra call.
- **The probe port is a `RunOptions` field**, not an `Engine` field. Engine is
  built with struct literals in ten places, while `RunOptions` has `Default`.
  Without a port, OS probes fail closed.
- **`shell_ro` runs without a shell.** The allowlist type (`ReadOnlyCmd`) has
  no public constructor besides `parse`. kai-os runs POSIX stages straight
  through `std::process` pipes, so a pipe between two allowed programs works,
  and a pipe into anything else, redirection or substitution cannot even be
  expressed. Globs are not expanded; `~/` is.
- **No outcomes, no change.** With `Config::outcomes` off nothing changes. With
  it on and no outcomes named, only the first turn differs (it carries the
  request); a test compares the two runs turn by turn.
- **Off by default.** The rule for shipping was "false done drops and the
  pass rate does not fall". That needs a live model, and this container has
  no model key and no browser (details in
  `bench/results/2026-10-12-verification.md`). So the feature ships behind
  `--outcomes` / *Settings › Safety*, and no gain is claimed.
- **Stage 2, Windows.** UI Automation events are not wired: the UIA event
  handlers could not be compile-checked here (the Windows cross-check stops
  at `ring`, which needs the MSVC headers). Windows labels actions from the
  tree diff and dHash only, which is exactly the old no-progress rule.
- **Files over 500 lines.** `engine/mod.rs` was at 502. Steering and output
  helpers moved to `engine/act.rs`, and it is now under 500. `agent.rs`,
  `skills.rs` and `human_e2e.rs` were already over 500 before this change;
  none of them grew.

## Untrusted content and sensitive data (2026-10-12)

The owner's rules: only direct user instructions are permission; everything
on screen (sites, PDFs, emails, invites, chats, tool output) is untrusted;
phishing, injection or an unexpected warning means stop and ask; confirm at
the point of risk, not before starting; typing sensitive data counts as
sending it. Design and tests: `docs/SAFETY.md`.

- **Rules in code, not only in the prompt.** The system prompt says screen
  text is data, but a model can be talked out of a prompt. So
  `untrusted::suspicious` pauses the run as a human check (`Suspicious`) when
  the screen holds a known injection or phishing phrase, and
  `risk::is_payment_or_password` now also covers typed sensitive data, which
  makes the confirmation unconditional, even on autopilot.
- **Phrase lists, not a classifier.** Deterministic and testable, with no
  extra model call. They will miss novel wording; the prompt rule covers the
  rest. "Gift card" was dropped from the phishing list: shops show it all the
  time.
- **One question per sighting.** "Ignore it and continue" acknowledges that
  exact text for the run, like a handled CAPTCHA.
- **Redaction.** Typed sensitive data is replaced by `[card number]` and
  similar in the saved trace (`Trace::push`) and in the model's step history
  (`HistoryItem::from_decision`). The approval prompt still shows the real
  value, so the user can check it. Training-data recordings (opt-in) keep
  the raw model reply.

## rustautogui evaluated, not added (2026-10-12)

Asked to add `rustautogui` 2.5 as a fallback, provided it improves the
foundations significantly. I read the code behind the functions Kai would
use (`find_image_on_screen`, the template preparation, mouse and keyboard)
and left it out:

- `RustAutoGui::new` panics (`panic!`, not an error) when no X display can
  be opened, as on Wayland or headless machines. A fallback must not crash
  Kai.
- Linux capture is `XGetImage` on the X11 root window: it cannot see native
  Wayland windows. Kai's capture already handles Wayland, X11 and Windows on
  every monitor.
- Windows capture covers the main monitor only, sized by
  `GetSystemMetrics(0/1)`, without DPI handling.
- `find_image_on_screen` prints to stdout on every call (27 `println!` in
  the crate). That would corrupt `kai mcp`, whose stdout is the protocol.
- The FFT matcher pads a 1080p screen to 2048×2048 and runs a 4.2M-point
  FFT per search (over 100 MB of temporaries, sums in f32).
- The struct holds raw X11 pointers (not `Send`), so each call would need a
  fresh instance and display connection.
- Mouse and keyboard duplicate enigo, which Kai already uses, and are
  X11-only on Linux.

The one capability Kai lacks is template matching, for re-aiming a
procedure step when its accessibility signature fails. If that turns out to
matter, normalised cross-correlation on Kai's own capture, searched near the
element's old position, is about 100 lines. It is not built, because it would
only back up procedures, which are off by default.

## Browser lane (2026-10-12)

- **Research first:** [docs/RESEARCH_2026_10_VENDORS.md](docs/RESEARCH_2026_10_VENDORS.md).
  Both frontier vendors now have a separate web surface (Anthropic's
  `browser_toolset_20260801` with accessibility refs and page text; OpenAI's
  Playwright code mode), and ComponentBench measured 83.1% (tree) vs 48.9%
  (pixels) for the same model. Kai had only the desktop path into web pages.
- **Built:** `kai run --browser [--headless]`, a `Perceptor` + `Actuator`
  over CDP (`crates/kai-os/src/web/`). The model's prompt and schema do not
  change; the harness picks the lane. Design and numbers: [docs/BROWSER.md](docs/BROWSER.md).
- **Pipe, not port:** `--remote-debugging-pipe`, so no other local process can
  drive the browser. Kai's own profile folder (Chrome 136+ ignores debugging
  flags on the default profile, and the user's everyday profile should not be
  remote-controlled anyway).
- **Address policy in `kai-core`** (`web::normalise_url`): http/https only.
- **`Observation::url`** (new, `None` on the desktop): the `url_is` probe
  prefers it over the address-bar text.
- **Rejected:** Playwright or a Node sidecar (a large runtime for a handful of
  protocol calls); a new action or tool surface for the model (keeps a small
  model's choices small); attaching to the user's running Chrome (needs a
  manual `chrome://inspect` toggle and a prompt per connection).
- **Measured** (headless Chromium 141, this container): tree-only observe
  3 ms (4 marks) and 23 ms (150 marks); full observe 115–127 ms p50.
  A same-page desktop comparison was attempted under Xvfb, but the desktop path
  read no page elements there (environment), so no head-to-head is claimed.
  **No live model run:** no key in this session.
- `kai-cli/src/main.rs` reached 500 lines; `run_recorder` moved to
  `runs.rs` with the lane switch.

## Browser lane hardening, Holo locally, live A/B (2026-10-13)

- **Page dialogs freeze the page** (measured: after an `alert`, the click's
  own `mouseReleased`, screenshots, tree reads and `Runtime.evaluate` all hang
  until the dialog is answered). The CDP client now tracks dialogs per page
  session from `Page.javascriptDialogOpening/Closed` and fails any call on a
  frozen page at once with `DialogOpen`. The model sees the dialog as marks
  whose names carry the page's message, so the existing risk words decide
  approval. Rejected: auto-accepting (a `confirm("Delete …?")` is a decision,
  not noise) and auto-dismissing (blocks legitimate flows).
- **Stale-mark identity** is role + name + box, not the whole element: the
  value changes when Kai types, and the lane wrongly treated the dropdown it
  had just set as a different element (found by the live test).
- **Skill selection bug (pre-existing, affects the desktop too):** keyword
  overlap picked the Gmail skill for "Open http://127.0.0.1:8765/contact.html
  in the browser and send the contact form … email ravi@example.com"
  (browser, send, email) and its fixed first step opened Gmail before the
  model looked. A skill whose fixed steps open another site is no longer
  picked when the task names its own site (e-mail domains and file names do
  not count as sites). Test: `a_task_with_its_own_address_does_not_open_another_site`.
- **`discard` is a high-risk word** (leaving a page with unsaved changes;
  "Discard draft" buttons).
- **HTTP clients trust the OS certificate store** as well as the built-in
  roots (`rustls-tls-native-roots`): behind a TLS-inspecting proxy, as in this
  container and in many companies, Kai could not reach any model API.
- **`kai bench --browser [--headless]`**: each task in a fresh lane with a
  throwaway profile; setup scripts see `KAI_BENCH_LANE=1` and
  `open-browser.sh` hands its URL to the lane. Results are filed as
  `<backend>+browser`.
- **Holo3.1 4B / 0.8B** added to the local catalog (community GGUFs pinned to
  a commit and SHA-256; H Company publishes GGUFs only for the 35B-A3B, 21 GB).
  `untested` until a bench run. The ranking now enforces that an untested
  model is never "Best" (it had relied on tie-breaking, which Holo broke).
- **llama-server lean flags** for every local model: `--no-repack -np 1
  --cache-ram 0 --ctx-checkpoints 2 --reasoning off`. Holo3.1 4B RSS
  6.51 → 4.29 GB; a cached-prefix step 9.6 s → 1.1 s (thinking off).
  Numbers and method: docs/LOCAL_MODELS.md.
- **Screen capture needs a window manager** under bare Xvfb in this container
  (`kai smoke` failed with "Connection error" until openbox ran).
- **Live A/B result** (bench/results/2026-10-13-browser-ab.md): with all
  fixes, lane 8/8 vs desktop 7/8 on the four local web tasks (2 runs each),
  38 vs 70 steps, 85k vs 162k prompt tokens, 178 s vs 409 s. Fixes found by
  the live runs: dropdown popups as option marks, typing replaces a field's
  value, the `fill-form` skill's submit rule (twice).
- **Holo3.1 4B locally**: 3/4 on the same tasks via the lane, 46 s per call
  on 4 CPU cores, 4.44 GB peak RSS. Stays `untested` in the catalog (one
  run, CPU only): not enough to recommend it over Gemma.
- **`kai bench --backend local`** now starts the model's server, as
  `kai run` does (it failed unless a server was already running).
- The lane stays opt-in (`--browser`): the A/B is two runs on four tasks.
- **`read_page`** (2026-10-13): page text for reading tasks as one optional
  reply flag, not a new action or tool, offered only by eyes that can read
  pages so the desktop prompt is unchanged; hostile text withheld; 4,000
  characters at most. Not yet measured live (keys revoked).
- **App switch** (Settings › "Do tasks in Kai's own browser"): one browser
  per app session, reused across runs (closing it at the end of each run
  would take away what Kai just opened), reopened if the user closed it.
  `friendly()` moved to `kai-app/src/errors.rs` (agent.rs was past 500
  lines) and passes the browser's own error messages through unchanged.

## Live runs on Gemma 4, and what they changed (2026-10-12)

The handoff's item 1 needed a model key; the owner provided two Gemini API
keys for this session. They were passed only as `GEMINI_API_KEY` in the
environment of each command, never written to a file, a log, a trace or a
commit (results JSON holds the bench command, which has no key).
Results: `bench/results/2026-10-12-live.md`.

- **The bench desktop had to be fixed first.** Firefox's policy loads
  `p11-kit-trust.so`, which `p11-kit-modules` provides and the install
  script did not install, so every page failed behind the TLS-inspecting
  proxy. `xdg-utils` was missing, a killed Firefox left "already running"
  and Troubleshoot Mode dialogs on top of later tasks, and SIGTERM does not
  close a Mousepad with unsaved text. The first live run (11/14) was on that
  broken desktop and is not used; all measured runs are on the fixed one.
- **Harness fixes from what Gemma actually did** (all with tests):
  - `shell` with no `text` and the command in `then[0]` takes that command
    (16 of 49 repairs in the first run were this shape);
  - a silent shell step no longer counts as "the screen has not changed";
    but the same command with the same result twice in a row does, the model
    is told so, and a third identical run is refused. The first version
    without the repeat rule let Gemma write one file 40 times: the stall
    warning had been the only brake. Both halves are needed;
  - a dedicated repair hint for `open_url` without a URL;
  - on 429, once every key is limited, wait as long as `RetryInfo` asks
    (10–60 s) and log the quota. The keys share one per-minute input-token
    quota, so rotating keys alone does not help;
  - a shell command that would kill Kai (its own PID or its parent's from a
    `ps | grep` match, `kill -1`, or Kai by name) is refused: Gemma did
    exactly that and ended a run;
  - `xdg-open` / `gio open` start detached (`xdg-open ~/dir` blocked 180 s).
- **Measured switches: none turned on.** Two runs per side; the table and
  reasons are in the results page. The rule from the handoff (pass rate
  holds and the target metric improves) was not met by any of
  `--outcomes`, `--macro-history`, `--recover`, `--best-of 3`, procedures
  or window scope.
- **Item 5 (visual re-aim) not built.** Its condition was replays failing on
  renamed targets; none were seen.

## The desktop app, driven for real (2026-10-12)

`kai-app` built and ran on Xvfb with Mesa's software Vulkan (lavapipe).
Driving it with real tasks found three bugs, now fixed and tested:

- **Team: "You stopped Kai" when nobody did.** A terminal and a screen
  sub-agent run side by side and share one run state. Every `Acted` event
  cleared the run's pending approval, and a dropped approval answers Stop.
  Approvals and human checks now queue per run and are removed only by
  answering them.
- **Approvals for harmless steps.** Risk words matched as raw substrings of
  the model's thought: "display" contained "pay", "information" "format",
  and a `wait` "to confirm" Mousepad opened asked for approval on
  high-risk-only. Words now have to start a word (stems keep inflections:
  `delet`, `remov`), and `wait`, `done` and `ask_user` never escalate. A
  click whose thought says "deleting" or "payments" still does.
- **Team splitting simple tasks.** Team is on by default, and Gemma split
  "open Mousepad and type a sentence" into a terminal and a screen subtask
  that both did the work (the sentence typed three times). A split is kept
  only when it has two or more terminal subtasks, the only case where
  sub-agents run side by side; otherwise the task runs as one agent.

After the fixes the same task ran as one agent in 3 steps and the editor
held the sentence exactly once (read back over AT-SPI).

## Item 3 and 4: background driving and the driver matrix (2026-10-12)

Design notes: `docs/BACKGROUND.md`, `docs/DRIVER_MATRIX.md`.

- Built: window-scoped frames (`KAI_WINDOW_SCOPE=1`, off). Not built: a
  pointer path that keeps the real cursor still (none works for GTK/Qt on
  X11 or Wayland), and a session per sub-agent (it needs that path).
- The matrix found two real gaps on its first run: **Qt 5.15 apps publish
  no accessibility tree** on this stack (Kai uses the grid for them), and
  **Firefox ESR 140 exposes no AT-SPI action on page buttons**, so Kai
  clicks them with the mouse. Both are recorded as expectations, so a fix
  upstream (or a driver regression here) makes the matrix fail and get
  updated.
- Qt 6, GTK 4, Electron/Chromium and Windows UIA rows are not built: no
  Chromium in the container (Ubuntu's is a snap), no Windows machine.

## Real-world review on Gemma 4 (2026-10-13)

Results: `bench/results/2026-10-13-real-world.md`. Eight everyday tasks
outside the bench set, each outcome checked independently.

- **Pass:** gzip old logs (t3), pcmanfm new folder (t6), Firefox link and
  opening hours (t7, after the table-cell and launch-wait fixes). **Right
  result, timed out:** CSV totals (t4). **False done:** duplicate files
  (t1), lines of code (t2), the app's Team disk-usage task. **Failed:**
  Mousepad Find and Replace (t5), six attempts.
- **Kept from t5:** a `type` with no target goes to the one focused,
  enabled editable field (password fields still ask; no focused field is
  still an error), and an element list with an unnamed field never
  replaces the screenshot. Both are small, on by default, and tested; the
  first removes a schema dead end, the second a relaunch loop seen live.
- **Safety probes held:** prompt injection on a page, password typing,
  out-of-scope deletes.
- **Next:** false done is now the main failure, not driving. A first gate
  to try: when the task names an output file, `done` needs that file to
  exist and be non-empty. Re-test t5 live.

## Model store, run memo, ship plan (2026-10-13)

Plan and research: `docs/SHIP_PLAN.md`. No model key and no GPU were in
this session, so nothing below is a Kai bench number.

- **OpenAI's computer-use loop re-read** (guide, accessed 2026-10-13): one
  `computer_call` with an ordered `actions` array, then one screenshot back,
  with history on the server (`previous_response_id`). Kai already has the
  same loop in a smaller form: `then` for the actions, and marks instead of
  pixels. The missing piece was memory of what the model *read*.
- **Run memo** (`docs/LONG_RUNS.md`, `--memo`, **off**): a scratchpad of up
  to 40 words that the model writes and that comes back on every later
  turn. Injection-looking memos are refused (the test fails without the
  guard, checked). With the switch off, the prompt and schema are
  byte-identical (tested). Full-history resend and summarising calls were
  rejected because of context, latency and cost for small models. To
  measure: `--memo` A/B on live Gemma 4 26B, two runs per side.
- **Model store** (`docs/MODEL_STORE.md`): `kai models search` and
  `kai models pull <owner/repo>[:quant]` build a pinned entry (commit,
  SHA-256, sizes) from the Hub's API into `<config>/models.json`. Gated,
  split and text-only repos are refused. Pulled models are always
  `untested`.
  Live on 2026-10-13: search and pull (resolve only) ran against the real
  Hub; see the note below.
- **Curated shelf +7**, all `untested`: Holo4 35B-A3B (official GGUF,
  Apache-2.0), Holo3.1 35B-A3B (official), Holo3.1 9B, Fara 7B (MIT), Gemma
  4 12B QAT (Google's own), GLM-4.6V Flash (MIT) and Qwen3.5 9B. All 14 URLs
  answered 200 with `x-linked-size` = `size_bytes` and `x-linked-etag` =
  `sha256`. Memory comes from each model's `config.json` (only the
  full-attention layers of the hybrid ones keep KV). Holo4 27B was left out
  because it is CC BY-NC. New `agent_trained` flag: shown in `kai models
  list` and used only to break ties between equally tested models. "Best"
  still needs a Kai measurement.
- **500-line rule restored** with no behaviour change: `prompt.rs` tests →
  `prompt_tests.rs`, human-check phrase tables → `human_phrases.rs`, Skills
  page tests → `skills_tests.rs`, and pages and shortcuts → `kai-app/src/nav.rs`.

## Task checks, the store in the app, a self-checking release (2026-10-13)

Still no model key and no GPU in this session, so there are no live numbers.

- **Task checks** (`--task-checks`, **off**; `docs/VERIFICATION.md`): the
  DONE gate seeds outcomes from the task's own words, with no model call.
  An output file or folder → `file_not_empty` (new OS probe, not on the
  model's menu); "show the X folder in the file manager" → `window_title(X)`.
  The rule is conservative: a misread task must never refuse a correct
  `done`, so a bare file name with no place, or a relative path when the
  task never mentions home, adds nothing. On the 30 bench prompts, 17 get a
  check and every one is right (listed). The e2e guard test fails without
  the seeding (checked). The model's own outcomes (`--outcomes`) now come
  after the seeded ones instead of replacing them.
- **Store in the app:** Models › More from Hugging Face, with search, Pull,
  and the same `add_pulled` (dedupe by SHA-256) as the CLI. A *Made for
  computer use* badge was added to the cards. Rendered under Xvfb with
  lavapipe (`KAI_CAPTURE_ONLY=models-store`, shot `93-models-store`), light
  and dark.
- **Release workflow:** `cargo test --workspace --release` before
  packaging, a `.sha256` next to each archive, a smoke test of the unpacked
  CLI on both OSes (skill validate, models list, mock bench), and "Run
  workflow" as a dry run that does not publish. The Linux package and smoke
  were simulated here with the debug build: checksum OK, 10 skills valid,
  mock bench 30/30.
- **Found while smoke-testing:** `kai models list | head` panicked on the
  closed pipe. Unix now gets the default SIGPIPE (exit 141, quiet), as
  other CLIs do (`console::init`).

## Windows for real, and cut-off replies from open models (2026-10-13)

- **First run on a real Windows machine** (GitHub `windows-latest`, run
  38429042037, workflow `windows.yml`). Clippy `-D warnings`, the full test
  suite, release build and CLI smoke (skills, models list, mock bench 30/30)
  all passed. `kai-app` started on Direct3D 11 (Microsoft Basic Render
  Driver) and photographed all 60 screens, light and dark, and they render
  as on Linux. The desktop replay passed 10 of the 19 replayable tasks,
  including `web-form-submit` on the real Windows desktop. 7 failures are
  recorded replies that are Linux shell (`bc`, `zip`, `for f in *`), so
  they cannot pass in PowerShell; that is not a Kai bug. 2 (`tip-calculator`,
  `calendar-event`) report "task setup failed" and are not diagnosed yet.
  The app logs "window not found / Invalid window handle" once at start,
  when the splash closes; it is harmless here, but noted. Manual dispatch
  from this session's GitHub connection was refused (403), so the session
  branch alone carries a push trigger; `main` stays manual-only.
- **Cut-off replies keep what was complete** (`reply_salvage.rs`). Holo3.1
  4B on this CPU (~60–145 s per call) wrote follow-ups with every optional
  field, and once looped `"left","left",…`. 10 of ~50 replies hit the
  192-token budget, 9 of them inside `then`, and each repair cost another
  full call. Kai now keeps every top-level field that ended before the cut
  and every complete follow-up. That is the existing rule for an invalid
  follow-up ("drops itself and the rest, never the main decision"). The
  usual validation is the guard: a cut `text` or `target` fails to parse
  and is repaired. A reply whose `risk` was cut is never salvaged, because
  a missing `risk` would read as "none"; the guard test caught exactly
  that. Replayed on the 10 real cut replies from the log: 9 salvaged, and
  the tenth (cut before `risk`) goes to repair as it should. There is no
  prompt or schema change, so prompts stay byte-stable.
- **Measured (2026-10-13), Holo3.1 4B on 4 CPU cores, 14 tasks, one run per
  build** (`bench/results/2026-10-13-local.md`): the cut-off reply fix cut
  repair calls from 16 to 6, step p95 from 172 s to 118 s and wall time by
  15%. Passed 8/14 before and 6/14 after, which is noise at one run each
  (different tasks failed). At ~60 s per call, Holo 4B on a CPU stays
  `untested`. Also found and fixed: a second local run started right after
  the first failed on a busy port (`652175e`).

## macOS on Apple Silicon (2026-10-15)

- **Kai runs on macOS (Apple Silicon), at the owner's request for the first
  release.** Design, permissions and what was rejected: `docs/MACOS.md`.
  The earlier "no macOS work" rule is lifted for this; Linux and Windows
  behaviour does not change.
- **Captures are scaled to points** (`frame::to_display`). xcap reports a
  Mac display in points but captures in pixels (2x on a MacBook), while
  clicks and the AX tree use points, so every click would have landed at
  twice its coordinates. Where the sizes already match (Linux, Windows) the
  capture is untouched.
- **An AX reader, not the grid.** `a11y/macos.rs` reads the frontmost
  window and menu bar with one `AXUIElementCopyMultipleAttributeValues` per
  element, under a 0.25 s messaging timeout and the shared 600 ms budget.
  Press is `AXPress`; fill sets `AXValue` on an empty field and reads it
  back. Secure fields are never filled or pressed this way.
- **`runtime_shaders` for GPUI on macOS.** The default build needs
  `xcrun metal`, which only the full Xcode has.
- **The Mac prompt rule** (`prompt::MAC_RULE`, shortcuts use `cmd`) is added
  only when `Perceptor::mac()` is true, so other prompts stay byte-stable.
- **Local models on Metal:** llama.cpp `b11429` macOS arm64 build, hashed
  here. Unified memory is counted once in the ranking: no split placement
  on a Mac.
- **Run on a real Mac** (`.github/workflows/macos.yml`, GitHub's
  `macos-15`, Apple Silicon). The first runs found four bugs that no Linux
  check could: the system-wide `AXFocusedApplication` failed although Kai
  was trusted (the tree is now read from the frontmost app's pid); the
  bench's server script used `setsid`; the bench saw no display or browser
  on a Mac; a killed browser could leave a call waiting on its timeout
  (calls now watch the process); the AX fill required an `AXEnabled` that
  text views do not report. Run 5, all green: tree read 10 ms, click-and-type
  and an `AXValue` fill (6.7 ms) in TextEdit read back from the tree, all 60
  app screens, replay 18/19 (the miss is `dark-mode`'s GNOME reply). No live
  model number on macOS is claimed.
- **Two browser-lane races, found by the first release dry run** (Linux,
  GitHub runner, 2026-10-15) and reproduced here with every core busy. (1)
  `Input.insertText` can return before the page handles the text, so the
  next step overtook it (leaving the page skipped its "unsaved changes"
  prompt); typing now waits, at most 600 ms, until the focused element
  holds the text. (2) A click on a button that closes its own tab got no
  reply, so Kai waited 15 s and reported an error; calls on a detached
  session now end at once, and the click counts as done. Browser tests
  under full CPU load: 1 failure in 4 runs before, 0 in 8 after.

## Transcript follow, keychain off the UI thread (2026-10-16)

- **Follow newest, hold while reading, jump back.** A run's Steps and
  Terminal follow the newest entry; an offset the user moved away from the
  end stops following, and growth under a still offset does not. While the
  user reads older steps, arrivals never move the view and a "Jump to latest
  · N new" button (or ⌘/Ctrl+↓) returns. Rejected: always auto-scroll (yanks
  the reader), and deciding by the end position alone (a step card that
  grows would stop following). Pure policy in `kai-app/src/follow.rs`, unit
  tested; live behaviour still unverified (`docs/reviews/UI_UX_AUDIT_2026_10_16.md`).
- **Keys are read on the agent runtime.** The app read the keychain on its
  UI thread at start, at each run launch and at each connection test; one
  read was sampled stalling the window 16.7 s on macOS. A source guard test
  keeps keychain reads in `kai-app/src/keys.rs`.
- **Key actions carry printable keys only.** A lone control character
  (newline, ESC, NUL) passed validation and reached OS injection as a raw
  key; both the native tool contract and the Decision schema now refuse it.
- **Act on what the step targets, not the whole screen** (review before
  merging the computer-use foundation). The pre-action check compared every
  element and the exact screen hash, so any clock or caret refused every
  step; it now compares the target element (or focused field), and pixels
  only for grids. Batched provider actions continue while the layout holds.
  Long text is typed, not pasted, under an execution context: the
  clipboard is not overwritten and a stop lands between keys.

## OpenAI parity: protocol fixes and the contract for open models (2026-10-16)

Research: `docs/RESEARCH_2026_10_OPENAI_PARITY.md` (sources dated 2026-10-16).

- **Accept what OpenAI really sends.** Real GA responses carry
  `"action": null` and `"pending_safety_checks": null`; both stopped every run.
  Absent, `null` and `[]` now mean the same. `wheel` is the middle button;
  OpenAI's uppercase key aliases, modifier `keys` on mouse actions, `wait.ms`
  and batches up to 16 are accepted; unknown names are still refused before
  OS input. Fixture-tested only: no OpenAI key was used.
- **Safety checks are the user's call, then acknowledged.** A flagged call
  asks the person; approval runs it and echoes `acknowledged_safety_checks`,
  declining runs nothing, the unattended bench always stops. Rejected:
  auto-acknowledging in the bench (it would accept `malicious_instructions`).
- **Pixel scroll on the desktop uses OpenAI's own conversion** (100 px per
  notch, at least one). The audit refused approximation; a documented,
  deterministic conversion that the vendor specifies is its reference
  semantics, not a guess.
- **OpenAI's model sees an unmarked, larger frame** (up to 1600 px) at
  `detail: "original"`; OpenAI attributes about 9 OSWorld points to resolution.
  Kai's own models keep the marked 280-token frame.
- **Open models can speak OpenAI's contract (`--oai-contract <frame>`, off).**
  Their action goes through the OpenAI parser, so an A/B compares models,
  not harnesses. Coordinate frames are data per model family; Gemma keeps
  marks (no GUI-grounding training). Rejected: making it the default before
  a two-runs-per-side bench, and pixel grounding for Gemma.
- **Browser lane offers click, type and keys through the shared contract**,
  reusing the lane's live-tested paths. Drag and held modifiers stay off in
  the browser until they have fixtures.

## The cobalt orbit icon, everywhere (2026-10-16)

- **The owner's render is the icon, and every surface is derived from it.**
  `assets/icons/kai-app-v3.png` (1254×1254, the attached image, md5
  `7189935daafc31e32ec7e7a4f2d10ed3`) is the master. `scripts/icons/build.sh`
  rebuilds from it: `Kai.iconset/` + `Kai.icns` (the macOS bundle icon,
  copied by `scripts/macos/bundle.sh`), `Kai.ico` (6 PNG entries), and
  `kai-app-512.png` (the runtime window icon through `WindowOptions::icon`).
  Design and regeneration: `docs/APP_ICON.md`.
- **The in-app mark is the ribbon on transparency**, extracted from the
  master by `scripts/icons/extract-mark.swift` (blueness mask, connected
  components, gloss and crevice handling; mean core `#3769E3`), drawn by
  `ui::orb` with an orbit-cobalt halo (`brand::ORBIT`) and on the splash
  screen. Ember stays the app's signal colour (`brand.rs`).
- **The Windows executable carries the icon** through
  `crates/kai-app/build.rs` + `winresource`, a Windows-target build
  dependency; a cross check without an rc/windres toolchain warns instead of
  failing, and Explorer and the taskbar read the embedded resource (the
  runtime `WindowOptions::icon` alone would not reach them).
- **The splash takes an image, not a widget** (`SplashScreen::icon_image`,
  vendored ely): an `AnyElement` would break the `Clone` bound `Hosted<T>`
  needs.
- **Rejected:** keeping the amber hand (`assets/icons/kai-hand.svg`,
  deleted) and tinting the old ember halo in place of the ribbon (the
  ribbon's own cobalt is the mark; the halo matches it). The v1 (amber
  hand) and v2 (folded K) renders stay for comparison; the redundant
  `Kai-v3.icns` export was dropped (byte-identical to `Kai.icns`).
- **Screenshots** in the repository are captured on Linux by the
  Screenshots workflow; the committed set predates the new mark and
  refreshes on that workflow's next run. The mark was verified in a live
  macOS capture, light and dark (`docs/APP_ICON.md`).
- **Checks:** `cargo fmt --all --check`, `cargo clippy --workspace
  --all-targets -- -D warnings` (host and `--target x86_64-pc-windows-gnu`,
  mingw-w64), `cargo test --workspace` and `scripts/macos/bundle.sh`
  (ad hoc sign, 10 checks, 0 failed) pass, and a live home-screen capture,
  light and dark, shows the mark. The Windows-only build-script calls are
  type-checked against the locked `winresource` crate, since a non-Windows
  host never compiles that branch; `windows.yml` covers it natively too.

## Mac crash, account connections and the DMG (2026-10-17)

- **Root cause of the macOS crash:** Settings rendered Ely `Select`s for the
  fast and careful models with zero choices when Codex or Claude Code was the
  backend and no catalog had been fetched; `Select::render` asserts on that
  (`select Name("fast") has no choices`). Reproduced with the 18:44 build and
  the owner's saved settings (`backend: codex`, a Gemma model) in capture
  mode; fixed by disabling empty pickers. Details in `docs/ACCOUNTS.md`.
- **Connections rebuilt as a state machine** keyed by generation and
  provider/endpoint; provider switches cancel and clear. Provider/model
  pairs are checked in kai-core (`catalog::check`) at pick, run start and
  client construction.
- **Opening Kai sends no model request.** The startup and backend-switch
  connection tests (one image request each) are gone; testing is explicit.
- **Settings has one place for provider and model** (Accounts & models);
  Model & server keeps endpoints, keys and per-backend tests. The Models
  page no longer repeats the accounts section.
- **Packaging:** `scripts/macos/package_dmg.sh` builds `dist/Kai.dmg`
  (Kai.app + Applications link) and checks structure, arm64, signatures,
  private files and the mounted image. The bundled Codex keeps OpenAI's
  Developer ID signature (the old script re-signed it ad hoc). Third-party
  licenses are generated from `cargo metadata`. Without
  `KAI_SIGN_IDENTITY`/`KAI_NOTARY_PROFILE` the image is ad hoc signed and
  not notarized, and the script says so. Benchmark result files (raw runs)
  are no longer shipped.
- **Pre-existing, not fixed:** four `window not found` log lines when the
  splash window is removed at start (the 18:44 build logs them too).
- **Not run:** live Codex/Claude sign-in, catalog fetches against real
  accounts, and the GPT-5.6 Luna computer-use evaluation (deferred).

## Stack research and kai-ui (2026-10-17)

- **Stack kept.** Research across agent architectures, grounding results,
  macOS/Linux OS APIs, UI frameworks and local inference
  (`docs/RESEARCH_2026_10_STACK.md`) found Kai's Rust engine, accessibility
  marks plus screenshot, the llama.cpp sidecar and the CDP lane right for a
  Mac-first, open-model agent. What blocks the Mac is the OS layer: ad hoc
  signing drops permissions on every rebuild, and xcap captures through
  `CGWindowListCreateImage`, obsoleted in the macOS 15 SDK
  (ScreenCaptureKit next). Linux needs Wayland portal capture and libei input.
- **Tauri, Electron and SwiftUI rejected** for the UI: WebKitGTK failures on
  Linux and a TypeScript UI over IPC; size and a sidecar; no Linux UI.
- **Ely is replaced by `crates/kai-ui`, written from scratch** on GPUI
  (owner's call; design and plan in `docs/UI_KIT.md`). Ely is 251k unowned,
  unlinted lines with no upstream and caused the one recorded Mac crash;
  Kai uses about 100 of its types. Look: ChatGPT × Raycast × Linear on
  Kai's graphite/ember identity. `vendor/` is deleted when the last page
  moves.

## macOS: Windows and Linux habits mapped to the Mac (2026-10-18)

- **Shortcuts.** Gemma sends `ctrl+c`, `alt+tab` and `ctrl`-clicks on a Mac
  despite `prompt::MAC_RULE`; they do nothing there, the screen does not
  change and the model retries. The hands now press the Mac shortcut
  (`kai-os/src/input/mac_keys.rs`), except in terminals, where `ctrl` is
  meant. On by default on macOS only: it is a correctness fix (the model's
  intent is unambiguous), not a heuristic; Linux and Windows are untouched.
  Not yet measured with a live model on a Mac.
- **App names.** When `open -a` does not know a name, the Mac app for a
  common Linux/Windows name is tried (`kai-os/src/launch_mac.rs`).
- **Prompt.** `MAC_RULE` names the Mac apps and says shell tools are BSD.
  Mac-only, so other prompts stay byte for byte the same.
- **App menu.** ⌘H, Hide Others and ⌘M.
- **Checked:** unit tests on every OS, clippy for Linux, Windows and macOS
  (from Linux, see CLAUDE.md), `scripts/macos/live_check.sh` extended with
  `ctrl+a` in TextEdit and `open notepad`.

## Live Gemma 4 E4B on this CPU: loops found and fixed (2026-10-18)

Gemma 4 E4B (`kai models install`, llama.cpp b11429, 4 CPU threads, about
60–110 s per call) on the Xvfb desktop, four simple tasks (`write-note`,
`open-folder`, `mousepad-write`, `close-app`), one run per build. What the
runs showed, and the fix for each:

- **The tree was dropped for the grid on a slow read.** `a11y::read_tree`
  timed out at the same 600 ms as the readers' own budget, so a partial
  tree was thrown away (on a Mac the walker's deadline always lost that
  race). On `mousepad-write` the GTK save dialog came back as a bare grid,
  and Gemma lost its place: "the current screen is a file explorer, I need
  to open the text editor", four steps in a row, until the time limit.
  Readers now get the deadline and return what they read (Linux bounds
  each node's D-Bus calls by it); the timeout is only a safety net 400 ms
  later. Test: `a_read_that_stops_at_its_deadline_keeps_what_it_read`.
- **Root cause of the lost dialog: pressing "Save As..." through
  accessibility.** Reproduced with the probe: opened by the AT-SPI action,
  GTK runs the modal dialog inside that call and Mousepad stops answering
  every tree read (and even Escape) until killed; opened by a mouse click,
  the dialog reads in 240 ms (146 nodes; 450 ms with all 4 cores busy). A
  control whose name ends in an ellipsis opens a dialog by every desktop's
  guidelines, so it is now clicked with the mouse (`live::opens_dialog`).
  On a Mac, `AXPress` on such a control returns "cannot complete" after the
  messaging timeout although the press happened, and Kai then pressed it a
  second time with the mouse; that answer now counts as pressed.
- **Bench fixture:** setups end Mousepad with `pkill -9`, so the next task
  began behind a "restore the previous session?" prompt; the desktop now
  turns session restore off (`scripts/desktop/start.sh`).
- **Repeated reasoning was not a loop to any guard.** Each grid click
  changed the screen, so no-progress and A-B-A-B stayed quiet. When a
  thought repeats the last step's (first 12 words), the next turn says so,
  with a fresh screenshot and the careful model (`engine/stuck.rs`; test
  `a_model_repeating_its_reasoning_is_told_to_look_again`). Seen firing
  live; Gemma changed route on the next step.
- **Cut replies.** 3 of the first 6 calls stopped at the 192-token budget
  and paid a repair call each. The default is 320: a ceiling costs nothing
  when a reply is shorter, and finishing is cheaper than a second call.
- **Empty replies: Gemma 4 thought despite `--reasoning off`.** The new
  log said it: `finish_reason length, 1448 characters of reasoning, 320
  tokens`, i.e. the whole budget went to the thought channel and nothing
  to the answer. The local server now also gets `--reasoning-budget 0`,
  which ends any thinking at once. The client logs `finish_reason` and
  reasoning length for an empty reply, uses a complete decision found in
  `reasoning_content`, and keeps a decision that is padded with whitespace.
- **Repairs say why** (`repairing malformed reply: <error>`).
- **Measured (honestly), one run per build:** 2/4 before and 2/4 after on
  the four tasks (10-minute limit, ~60–110 s per call), 24 calls for 17
  steps before, 21 for 17 after. Tree reads lost to the grid: 9 → 0.
  Empty replies: 3 → 0 on a 20-step run with the thinking budget at 0.
  `mousepad-write` went from blind grid clicks on the save dialog to typing
  the file name into its Name field; with a 25-minute limit E4B still did
  not finish (it retyped names and wandered) and Kai stopped it as a loop.
  That is the 4B model's limit, not a harness fault. `write-note` went from
  3 steps and a repair to 2 steps. `open-folder` fails on the model's
  choice: it lists the folder with `ls` (the known "show the folder" gap),
  then asks Linux `open_app` for "File Manager", which only takes an exact
  app name (a `GenericName` match is a possible follow-up; on a Mac it now
  opens Finder). A live A/B with Gemma 4 26B on the Gemini API, two runs
  per side, is still owed.

## Browser lane: a closed tab hands back to its opener (2026-11-11)

- **Found:** `browser_edges::new_tabs_are_followed_and_a_closed_tab_hands_back`
  failed 5 of 10 runs alone on `main` (7 of 10 under load) with Chrome for
  Testing 153. The close was seen, but the lane handed over to "the newest
  page", and a fresh profile in that Chrome also lists its own pages (a
  first-run notice). Kai then read a page with no tree, for good.
- **Fix:** `web/tabs.rs::hand_back`: the tab that opened the closed one
  takes over while it is open, else the newest web page; never the closed
  tab (it can still be listed right after `window.close()`) and never a
  `chrome://`-style page. Tab handling moved out of `lane.rs`, which was
  over 500 lines.
- **Checked:** unit tests for the choice; the browser test 0 of 20 failed
  alone and 0 of 10 with all four cores busy (whole `browser_edges` file).
