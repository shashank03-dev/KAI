# Continuation prompt — paste this into the new session

---

You are continuing work on **Kai**, in the repo `shashank03-dev/KAI`, branch
`main`. Read these first, in this order, before doing anything:
`README.md`, `DECISIONS.md`, `docs/HARNESS.md`, `docs/DESIGN.md`,
`docs/SKILLS.md`, then skim `crates/*/src`. They describe what already exists
and what was not finished. Do not redo work that is done and tested.

**What Kai is:** an open-source desktop app (Rust, GPUI + Ely GPUI Components,
pinned) for **Linux and Windows only** that lets Gemma 4 operate the computer:
it sees the screen (screenshot + accessibility tree → numbered Set-of-Marks),
picks one action per step, clicks/types/runs shell commands, asks before risky
actions, and can be stopped instantly. Gemma 4 runs on our DigitalOcean GPU
Droplet via vLLM, with the Gemini API as fallback. The harness (`kai-core`,
`kai-os`) and the app (`kai-app`) are one product: the CLI `kai` exposes the
same harness in a terminal, and the desktop app is a richer front end over it.

**Rules for this session**
- Use sub-agents for parallel work: the Agent tool with `model: "opus"` and
  `effort: "medium"`. Split by crate or concern so agents don't edit the same
  files at once; you integrate, build, test and commit.
- Commit as the repo owner with **no** `Co-Authored-By` or Claude lines.
  Do not open a PR unless asked.
- **One push per batch of work, and only when I say "push".** Commit
  locally as you go, but do not push in between: CI runs only on pushes to
  `main` (Linux + Windows, ~25 min) and Actions minutes cost money. Run the
  full local check first (fmt, clippy, test, skill validate), then push
  everything to `main` in one go. Screenshots run only by hand from the
  Actions tab.
- English only in the UI. Linux + Windows only (no macOS work).
- Keep every claim honest: README and the Benchmarks page show only numbers
  that were actually measured, with how they were measured.
- Secrets (Vercel AI Gateway key, Gemini key, OpenAI key, HF token, DO token)
  come from environment variables or the OS keychain. Never write them to
  files, logs, traces or commits.
- Keep `cargo fmt`, `cargo clippy --workspace --all-targets -D warnings` and
  `cargo test --workspace` green, plus `kai skill validate skills`. Files stay
  ≤ 500 lines (Ely's rule). Watch disk: use `CARGO_PROFILE_DEV_DEBUG=0` and
  `cargo clean -p <crate>` when space runs low.

**Ask me at the start (one question, then proceed):** what "JEV" refers to.
My working guess is **V-JEPA 2** (Meta's video world model used for
predicting the outcome of actions in planning), as a fast "decision/verifier"
model next to Gemma. If that's right, research whether it can run fast enough
(locally or on the Droplet) to predict/verify action outcomes and cut model
calls; adopt it only if it measurably helps, otherwise document why it was
dropped. If "JEV" means something else, use that.

## Goals, in order

### 1. Deep research first (write `docs/RESEARCH.md` with sources)
Research, with real sources and dates, everything that makes a computer-use
agent **faster first, accurate enough second** on Gemma 4 and Google models:
- Gemma 4 specifics: image token budgets (70/140/280/560/1120 soft tokens),
  best screenshot size, prompt format, thinking on/off cost, JSON reliability,
  26B-A4B vs 31B vs E4B/E2B trade-offs; vLLM flags for latency (prefix
  caching, chunked prefill, speculative decoding/MTP drafter, FP8 KV cache,
  `max_num_seqs`, streaming first token), structured outputs backends.
- Gemini API: Gemma via `generateContent`, latency, `thinkingLevel`, context
  caching, streaming, the Gemini computer-use model if relevant for comparison.
- What fast computer-use agents do: OpenAI CUA / Operator, Anthropic computer
  use, UI-TARS, OS-Atlas, ShowUI, OmniParser, Agent S2/S3, SeeAct, OSWorld
  and Windows Agent Arena leaderboards; grounding tricks, action batching,
  multi-action steps, skipping screenshots when the tree is enough,
  accessibility-first control (UIA patterns: Invoke/Value/Toggle instead of
  mouse), keyboard-first strategies, diff-based screen updates, caching plans.
- **Vercel AI Gateway** (key supplied as `AI_GATEWAY_API_KEY`): which models
  are free/cheap there, latency, OpenAI-compatible endpoint. Decide whether to
  route (a) the GPT comparison models and (b) a tiny fast "router/verifier"
  model through it. Use it only if it is worth it; document the decision.
- V-JEPA 2 (or whatever "JEV" turns out to be) as above.
End with a ranked list of changes with expected speed/accuracy effect.

### 2. Make the harness faster (implement the top items from research)
Candidates to evaluate and implement where they win:
- **Multi-action steps:** let the model return a short list of actions when
  the next few are certain (type + enter, open + search), verify once.
- **Accessibility-first actions:** on Windows use UIA Invoke/Value/Toggle/
  ExpandCollapse patterns; on Linux AT-SPI Action/EditableText — no mouse
  move, no screenshot round-trip, works when the window is covered.
- **Tree-only steps:** skip the image when the tree is rich and the last
  action's `expect` is checkable from the tree; send the image only when needed.
- **Streaming decode** with early JSON parse to act before the reply finishes;
  measure time-to-first-token.
- Smaller, cheaper frames (Gemma soft-token budget tuning), screen-diff crops.
- **Planner/Executor split:** Gemma plans once; a fast model (or deterministic
  code) executes routine steps; Gemma re-plans only on surprise.
- **Plan/skill cache:** reuse successful traces for similar tasks (auto-skills).
- Zoomed crop after a missed grid click (still not done).
- Measure every change with the benchmark; keep only what wins.

### 3. One-on-one benchmark vs GPT models
- Add an `openai` backend (`OpenAiClient`) using the OpenAI Responses /
  computer-use API (and/or GPT models via the Vercel AI Gateway), run through
  **the same harness, tasks, machine and checks**. Also run GPT through its own
  native computer-use loop as a second comparison, labelled separately.
- Expand `bench/tasks.yaml` to ~25 realistic tasks people actually do
  (browser search, forms, files, settings, text editor, email draft, terminal
  jobs, downloads, screenshots, calculator, spreadsheets), each with setup and
  an automatic check.
- Report per backend: success rate, seconds per step p50/p95, total time per
  task, steps per task, tokens/cost. A head-to-head table in README and on the
  Benchmarks page. Never claim a number that wasn't measured.

### 4. The desktop app as the control centre (like Codex)
The app must be fully working on Linux and Windows and feel like a control
room for agents:
- A **task list / sessions sidebar**: several runs (queued, running, done),
  each with its own live transcript, steps, frames and controls
  (pause, resume, stop, retry, steer — send a message to a running agent).
- **Chat-style steering**: talk to the agent mid-run; it folds the message into
  its next step.
- Sub-agent tree view (Ely `lists::Tree`) with live status per sub-agent.
- Terminal pane showing the shell commands Kai runs and their output
  (Ely `terminal`), plus a diff/file-change view where relevant.
- Model/backends switcher (Gemma 26B/31B, Gemini, GPT for comparison).
- Everything existing (onboarding, approvals, mini window, kill switch,
  skills, history, benchmarks, settings) keeps working.
- Tell me if you think any other desktop component would help, and add it.
- The headless container can't render GPUI (see DECISIONS.md "Screenshots");
  if you find a way (e.g. a different X server, `Xvnc`, software GL path,
  or Ely's capture approach), capture all screens in light and dark into
  `screenshots/` and review them visually; otherwise leave the harness ready.

### 5. Simulate real-world use in the container
Install a desktop stack in the container (Xvfb/Xvnc + a light WM like
openbox, Firefox or Chromium, a text editor such as gedit/mousepad, a file
manager, LibreOffice Calc if space allows). Run Kai on everyday tasks end to
end — with replay first, then with real models once keys are available — and
record timings. Fix whatever breaks. Commit traces/results.

### 6. Ship
Update README (head-to-head results, measured only), HARNESS.md, DESIGN.md,
DECISIONS.md (every assumption and deviation), CI green on Linux and Windows,
release workflow producing the Linux tarball and Windows zip.

**When done:** tell me plainly what was built, what was measured (with the
numbers), what wasn't, and what I need to do next (keys, Droplet, making the
repo public).
