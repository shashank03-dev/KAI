# Research: the next foundation for Kai — verification, executable memory, background control, and CAPTCHAs

Written 2026-10-12. Builds on [RESEARCH.md](RESEARCH.md) (speed inside the
harness), [ROADMAP.md](ROADMAP.md) (training plan) and [MEMORY.md](MEMORY.md)
(experience memory, first live test showed no gain). This document does not
repeat them. It covers what the strongest 2026 builders and papers do that Kai
does not yet do, and maps each to Kai's code.

Labels as in RESEARCH.md: **[Paper]**, **[Vendor]**, **[Indep.]**,
**[Practitioner]** (an X post / blog from people running it in production),
**[Est.]**. Nothing here was measured in this repo.

---

## 0. The short version

1. **The 2026 consensus: the model is no longer the bottleneck; the harness
   is.** a16z talked to operators running millions of computer-use tasks a month
   and found buyers "rarely" choose by model; what they pay for is
   verification, escalation, error handling and caching
   ([Amble et al., X article, 2026-08](https://x.com/seema_amble/status/2086833033097969848)) [Practitioner].
2. **The biggest single published harness gain this year is structured,
   verifier-gated progress state**, not a better model: StructAgent took
   Qwen3.5-27B from **31.6% → 62.2%** and Qwen3.5-9B from **27.0% → 46.9%** on
   OSWorld-Verified ([arXiv 2607.11388](https://arxiv.org/abs/2607.11388)) [Paper].
   Kai's "false done" failures are exactly what it fixes. **Build this first.**
3. **Memory that works is executable, not advisory.** EchoPath replays
   validated runs as parameterised programs with visual re-aiming:
   **~96% fewer tokens, ~60% less time** at ~91% success on 159 OSWorld-Verified
   replays ([arXiv 2609.16635](https://arxiv.org/abs/2609.16635)) [Paper].
   The production version of the same idea: "run once with the agent, cache as
   deterministic code, the model only comes back when something breaks"
   (a16z, [Stagehand v4 caching](https://x.com/Stagehanddev/status/2090886334688932150)).
   Kai's text lessons are the weakest form of memory; its plan cache is the
   seed of the strongest.
4. **Text lessons and full-screenshot memory can hurt.** Full-image memory
   *increased* hidden-operation blindness by 11.7 pp; storing only
   action-local crops (AGMem) gave +6.8 pp over it
   ([arXiv 2606.14106](https://arxiv.org/abs/2606.14106)) [Paper]. Evolving skill
   libraries help only conditionally (+5.7 to +18.6 pp, domain-dependent, with
   "revision churn") ([arXiv 2609.04869](https://arxiv.org/abs/2609.04869)) [Paper].
   This matches Kai's own first live memory test (no gain).
5. **"Computer-Use 2.0" = background, window-scoped, multi-agent driving.**
   Codex (April 2026), Cua Driver, Hermes all moved to agents that operate one
   window in the background with their own cursor while the human keeps working,
   and an action ladder: accessibility action → pixel click → brief raise
   ([trycua, X article, 2026-07](https://x.com/trycua/status/2077440069460181426)) [Practitioner].
   Kai already has the hardest part (a11y actions that work on covered windows).
6. **CAPTCHAs: do not build a solver. Build a hand-off.** See §6 for why
   (it's circumvention of a site's access control, and behavioural detection
   makes it a losing race anyway: one study flagged 7/7 commercial browser
   agents from movement alone). Kai is a desktop app with the human sitting
   right there, which makes the hand-off cheap and is the pattern every
   serious product uses.

---

## 1. Where the field is (Oct 2026)

| Signal | Value | Label |
| --- | --- | --- |
| OSWorld-Verified best a year ago → now | 42% → **85%** (Claude Fable 5), humans ~72% | [Aggregator] via a16z, llm-stats.com June 2026 |
| OSWorld 2.0 (long tasks, ~318 tool calls avg, median human 1.6 h) | Claude Fable 5.1 77.9% (partial, modified run), Simular 73.0%, GPT-6 Astra 72.6% | [Vendor]/[Aggregator] [steel.dev](https://leaderboard.steel.dev/leaderboards/osworld-2/) |
| Best open-source harness on OSWorld-Verified | StructAgent + MiniMax-M3 **78.9%** | [Paper] |
| Hard expert apps (KiCad, 25 tasks, 200 steps, 7 frontier models) | best **6/25**; blank-canvas tasks **0** for every model | [Practitioner] Cua-Bench |
| New apps vs. self-reported (OpenComputer) | small open models collapse (EvoCUA-8B 46.1% → 10.9%) | [Indep.] (already in ROADMAP §2.1) |
| Agent vs human speed in production | 2–3 min human task takes an agent 8–10 min | [Practitioner] a16z |
| Cost | $3–15 / agent-hour depending on harness (screenshot frequency, context, how much is offloaded to code) | [Practitioner] a16z |

**Reading for Kai:** frontier generalists are now above human on short
desktop tasks. A small local model wins in Kai only on *Kai's own repeated
tasks*, and only if the harness (a) never claims done without evidence and
(b) turns every success into something that no longer needs the model.

---

## 2. Verification-gated progress state (highest priority)

### What StructAgent does ([2607.11388](https://arxiv.org/html/2607.11388)) [Paper]

- **State is a ledger, not a transcript.** `VerificationRecord { required_outcomes,
  failed_paths, completed_strategies, slots, global_facts }`. Each `Outcome`
  has id, description, evidence hint, dependencies, app tag, optional
  `VerifySpec`, and status `pending | verified | reverted`.
  Planner/actor output goes to a timeline but **cannot change outcome state**.
- **Two verifiers.** A per-action one compares before/after screenshots and
  returns `complete | continue | no_effect | off_track | stuck`. A milestone
  verifier runs when the actor returns control and when DONE is claimed; it can
  request **deterministic probes**: `file_grep`, `url_match`, `a11y_match`,
  `calc_verify`, `writer_verify`, `impress_verify`, `shell_command` (read-only
  allowlist). **A determinate probe overrides the visual judgement.**
- **DONE gate.** Structured verification raised DONE recall 0.67 → 0.81; on 100
  disagreements the structured verifier was right 63% vs 31% for visual-only.
- **Recovery is diagnosed**, not retried: missing evidence / conflict with a
  verified requirement / repeated failure / environment blocked → replan,
  retry, extra check, repair environment, or **escalate to human**.
- **Memory is advice only and can never mark an outcome complete.**

Related: HiViG ([2606.11078](https://arxiv.org/abs/2606.11078)) [Paper]
condenses history into a **macro-action history** and checks the proposed
click coordinates against the screenshot before acting: +5.8 pp (Qwen3-VL-32B),
+9.0 pp (Gemini-3-Flash) over the best critic baseline. Counter-evidence:
OS-Oracle found an *untrained* GPT-4o pre-action critic often **degraded**
performance ([2512.16295](https://arxiv.org/abs/2512.16295)) [Paper] — critics
must be cheap and specific, not generic.

### What Kai has today

`expect` per step, dHash no-progress detection, an optional text verifier (off:
p50 803 ms), the bench's shell `check`. No per-run outcome ledger, no DONE gate
at run time, last-3-steps history.

### Proposal for Kai

1. **`Outcomes` in the plan step.** At step 0 (same call that already plans),
   ask the model for 1–4 required outcomes, each with an optional probe from a
   fixed menu: `file_exists(path)`, `file_contains(path, text)`,
   `a11y_has(role, name|value)`, `window_title(regex)`, `url_is(regex)`,
   `shell_ro(cmd)` (allowlist: `ls`, `stat`, `grep`, `cat`, `test`, `wc`,
   `xdotool getwindowname`, PowerShell `Get-*`/`Test-Path`). Lives in
   `kai-core` (OS-free); probes run through a new `ports::Probe` implemented in
   `kai-os`.
2. **DONE gate.** `done` is accepted only when every outcome with a probe is
   `verified`. If a probe fails, the step is rejected with "not done: <outcome>
   failed <probe>" and a full frame is forced. Probe-less outcomes fall back to
   today's behaviour. This targets Kai's measured false-done failures
   (`email-draft`, `spreadsheet-budget`, `csv-total`, `dark-mode`).
3. **Per-action tri-state** (cheap, no model): from AT-SPI/UIA events and the
   tree diff, label every action `confirmed | unverifiable | no_op` (Cua Driver
   reports exactly this). `no_op` replaces "two identical dHashes" as the
   primary no-progress signal; dHash stays as the fallback.
4. **Macro-action history.** Replace "last 3 steps" with a ≤ 60-word running
   summary of verified outcomes + last 2 raw steps. Stable, cache-friendly,
   and it is the part HiViG's ablation credits for less short-sighted planning.
5. **Diagnosed recovery** in `failure.rs`: map the failure taxonomy that
   already exists to an intervention (replan / careful model / repair env /
   ask user) instead of the current uniform "stop and ask after 2".

Measure with `kai bench`: pass rate, false-done count, steps. [Est.] This is
1–2 weeks of work and the only item here with a paper showing a +20–30 pp
harness-only gain.

---

## 3. Memory: from hints to executable procedures

### What the evidence says

| Approach | Result | Source |
| --- | --- | --- |
| Executable, parameterised replay with visual re-aiming (EchoPath) | 91–93% success on replays; **−96% tokens, −60% time** vs Synapse; target re-aim 95% accepted at 0.6–1.6× scale, median 0 px error; state gate rejected 78% of incompatible starts | [2609.16635](https://arxiv.org/html/2609.16635) [Paper] |
| Human demo → skill with workflow rules + completion checks (Work2Skill) | +3.1 to +7.5 on 286 OSWorld-Verified tasks | [2610.12984](https://arxiv.org/abs/2610.12984) [Paper] |
| Online-evolving skill library | +5.7 to +18.6 pp, conditional, revision churn | [2609.04869](https://arxiv.org/abs/2609.04869) [Paper] |
| Full-screenshot visual memory | **worse** on hidden-operation and grounding failures | [2606.14106](https://arxiv.org/abs/2606.14106) [Paper] |
| Action-local crops (AGMem) | +6.8 pp over full-image memory; hidden-op blindness −26% | same |
| Learn from failures too (ReasoningBank) + memory-aware test-time scaling | +4.6 to +8.3 pp (web/SWE); WebArena-Shopping 49.7 → 55.1 with k = 5 | [Google Research](https://research.google/blog/reasoningbank-enabling-agents-to-learn-from-experience/) [Paper] |
| Compile recurring intents into specialists instead of retrieving text | 32/36 wins when a specialist fires | [2608.08995](https://arxiv.org/abs/2608.08995) [Paper, position] |
| Production: run once → cache as code → model only on break | "cost per run falls over a workflow's lifetime"; one operator: 15–20M portal interactions/month with agents as self-healing fallback | a16z [Practitioner] |

### Design for Kai (four memory types, one journal)

Following the most complete practitioner design I found
([alphabatcher, "Agent Memory That Improves Between Runs", X, 2026-09](https://x.com/alphabatcher/status/2095503115130646825))
and the papers above:

```
trace (already recorded) ──► events.jsonl (append-only, scrubbed)
                                   │
                         consolidation job (after run / idle, own budget,
                          read-only; proposes diffs with evidence ids)
                                   │
          ┌───────────────┬────────┴─────────┬────────────────────┐
     procedures        app facts         lessons             preferences
   (executable,      ("in LibreOffice   (today's avoid/     (user-sourced,
    EchoPath-style)   Calc, sheet ops    prefer, scoped to    authority = user)
                      are under Sheet›") a screen signature)
          │                  │                 │                    │
          └──── promotion gate: must beat current memory on a paired bench A/B ────┘
```

1. **Procedures (the big one).** Generalise `plan_cache.rs` from "replay the
   leading deterministic prefix" to **the whole verified run**:
   - Store per step: action, target *signature* (role, name, a11y path) **and a
     small crop** of the target (+ wider context crop), and the outcome probe
     that held after it.
   - Mark **fixed** (clicks, keys, navigation) vs **flexible** slots (typed
     text, file names, numbers taken from the task). Rebind flexible slots from
     the new task; reject any change to a fixed slot.
   - Replay: resolve the target by a11y signature first (Kai's strength), then
     by crop template-matching (EchoPath: NCC, threshold 0.78, margin 0.02),
     else **hand that one step to the model** with the procedure as a hint.
     Structural mismatch (wrong app, outcome probe fails) → abandon replay,
     normal loop.
   - Lifecycle: `candidate → active → quarantined → retired`; promote only
     after the run's outcomes all verified (§2) — so §2 is a prerequisite.
   - Stop at anything approval-gated, exactly as the plan cache does now.
   This is what turns "Kai did it once" into "Kai does it in 300 ms with no
   model call", which is the only way a 4B-active model beats a frontier
   model on repeated personal tasks.
2. **App facts / hidden operations.** The failure the papers single out
   (hidden-operation blindness: the needed command is in a menu or shortcut not
   on screen) is the one text memory *should* fix. Store "to do X in app Y:
   menu path / shortcut", learned when a run succeeded via a menu or key. Scope
   to app, not task words.
3. **Lessons (existing).** Keep, but recall at the **decision point**: key on
   app + a coarse screen signature (window title + top a11y roles), not on task
   words. MEMORY.md's first live test already found task-word scope too loose.
4. **Visual memory, if any, is crops only.** Never put old full screenshots in
   the prompt.
5. **Consolidation off the hot path** ("dreaming", Anthropic's term for a
   background job that proposes memory diffs from batches of sessions, now a
   research-preview API). Kai's `reflect.rs` is in-band and single-run; move it
   to an idle-time job that sees N runs, groups recurring failures, and
   proposes procedures/facts with evidence ids. The user approves in
   Settings › Memory.
6. **Memory must pass an exam.** `kai bench --memory --repeat 2` already
   exists; turn it into a gate: a new memory version is activated only if it
   wins ≥ 4 more paired tasks than it loses and false-done/stale errors don't
   rise. Credit a lesson only when the trace shows it was shown *and* the
   outcome changed (access count alone strengthens false memories).
7. **Treat memory as untrusted data.** A web page can inject text that the
   reflection turns into a lesson that runs forever. Keep memory in a quoted
   evidence block with ids, never in the system prompt, and never let it touch
   approvals (already true; keep it true for procedures too).

---

## 4. Control layer: "Computer-Use 2.0"

From Cua's AI Engineer World's Fair write-up [Practitioner] and Codex /
Hermes releases:

- **Window-scoped, background sessions.** Each agent gets a session id, its own
  synthetic cursor, a window set, and a recording; the human keeps using the
  machine. Action **ladder**: a11y action → process-targeted pixel event →
  raise window for a few ms then restore focus. Kai's a11y-first path already
  works on covered windows on Linux and Windows; the missing pieces are
  (a) not stealing focus on the mouse fallback, (b) per-window capture instead
  of whole-screen, (c) a session id per sub-agent so *Team* screen subtasks can
  run concurrently in different windows instead of taking turns.
- **Desktop as a tool inside a general agent.** Expose Kai's eyes/hands over
  MCP/CLI (`kai-os` is already the right boundary) so Claude Code / Codex /
  local agents can call "list windows / window state / act". When the GUI
  stalls, the agent reads the config file or log instead of taking another
  screenshot. Kai already has `shell`; make probes (§2) and the terminal
  first-class evidence, not just actions.
- **Regression matrix for the driver.** Cua runs 500+ behavioural checks across
  toolkits (GTK, Qt, Electron, Tauri, web views) that independently observe
  the state change. Kai's `bench/` tests tasks; add tiny per-toolkit fixture
  apps that assert "this a11y action really changed the widget" so a driver
  change can't silently fall back to the mouse.
- **Further out:** pixel-native action models trained on video (Standard
  Intelligence FDM-1: 11M hours of screen recording, 30 FPS
  [Vendor] [si.inc](https://si.inc/posts/fdm1/)) suggest the
  screenshot-per-step loop is not permanent. Not actionable for Kai now.

---

## 5. Test-time compute for hard tasks

- **Behavior Best-of-N** (Simular Agent S3): run N attempts, a judge picks the
  best trajectory; 69.9% → 72.6% OSWorld, past the human line
  ([Simular](https://simular.ai/articles/simulars-computer-use-agent-outperforms-humans)) [Vendor/Paper].
  StructAgent: 1 → 3 rollouts improved every domain [Paper].
- **For Kai:** impossible on the user's live desktop (side effects). Possible
  for (a) terminal-only subtasks in throwaway dirs, (b) the bench / training
  data generation in VMs (ROADMAP Phase 1), (c) *planning*-level BoN: sample 3
  plans, pick by a cheap judge, execute one. Use the verified outcome ledger as
  the judge's input.

---

## 6. CAPTCHAs

### What I will and won't recommend

I'm not going to help design an automated CAPTCHA solver for Kai.
A CAPTCHA is a site's explicit "no automated software here" control; solving
it programmatically is circumventing that control (and usually the site's
terms), and the same component is what spam and credential-stuffing tools
need. It's also a poor engineering bet:

- Agents are bad at the hard ones: best agent **40%** vs humans 93% on Open
  CaptchaWorld (20 types; failures on spatial manipulation, counting,
  sequential motor actions) ([NeurIPS 2025, 2505.24878](https://arxiv.org/abs/2505.24878)) [Paper].
- Detection has moved past the puzzle: a UC Davis study (reported secondhand)
  classified **7/7** commercial browser agents from movement features alone
  (F1 0.9994), and Cloudflare's "Precursor" (July 2026) scores behaviour across
  the whole session rather than at a checkpoint
  ([TinyFish summary](https://current.tinyfish.ai/issue/36/market-pulse/article/28250/the-way-they-move),
  [Gigazine](https://gigazine.net/gsc_news/en/20260714-cloudflare-precursor/)) [Indep., secondary].
  Solving the image wouldn't get you through anyway.

### What to build instead (this is what the serious products do)

1. **Detect → pause → hand off.** Recognise challenge pages (reCAPTCHA /
   hCaptcha / Turnstile iframes and titles in the a11y tree, "verify you are
   human" text, 2FA/login prompts) as a new failure class
   `HumanVerification` in `failure.rs`. Kai stops acting, raises the window,
   shows "Kai needs you for this step — press Continue when done", and resumes
   with a fresh full observation. Kai is a desktop app, so the human is
   physically there — this costs seconds. Same pattern: Browserbase live view,
   Hermes "take over", OpenClaw's handoff proposal (which explicitly excludes
   solving), Auth-Your-Agent `request_takeover` [Practitioner].
2. **Use the user's own logged-in browser profile.** Kai drives the user's
   real Firefox/Chrome, so challenges are rare to begin with; don't launch fresh
   profiles for web tasks. Remember "this site challenges; ask early" as an
   app fact (§3.2) so Kai asks *before* starting the run, not mid-flow.
3. **Make the gate model-proof.** Agents have been talked into bypassing
   CAPTCHA and confirmation prompts ("it's for accessibility")
   ([arXiv 2507.05445](https://arxiv.org/abs/2507.05445)) [Paper]. The pause
   must be enforced by the harness (like password fields today), not left to
   the model's judgement.
4. **For any hosted/remote browsing Kai adds later:** sign requests with
   **Web Bot Auth** (HTTP Message Signatures, Ed25519, IETF draft) and apply
   for Cloudflare's *signed agents* list
   ([Cloudflare, 2025-08](https://blog.cloudflare.com/signed-agents/)). That
   is the sanctioned route to being let in as an agent rather than disguised
   as a human. Prefer official APIs / MCP connectors where they exist.

---

## 7. Ranked plan for Kai

| # | Change | Why | Size [Est.] | Measure |
| --- | --- | --- | --- | --- |
| 1 | Outcome ledger + read-only probes + DONE gate (§2.1–2.2) | +20–30 pp class result in StructAgent; Kai's top failure is false done | 1–2 wk | bench pass rate, false-done count |
| 2 | Per-action `confirmed/unverifiable/no_op` from a11y events (§2.3) | cheaper, more precise than dHash; feeds #1 | 3–5 d | no-progress precision, steps |
| 3 | Human-verification detection + hand-off (§6) | real-world web tasks stall here today | 3–5 d | runs that resume vs abort |
| 4 | Executable procedures replacing the prefix plan cache (§3.1) | −90% tokens / −60% time on repeats in EchoPath; the "cache as code" production pattern | 2–3 wk | repeat-run model calls, latency, success |
| 5 | Re-scope lessons to app+screen; add app facts for hidden operations (§3.2–3.3) | fixes the loose scope found in the live test | 1 wk | `--memory --repeat 2` A/B |
| 6 | Memory exam gate + offline consolidation (§3.5–3.6) | stops bad lessons from spreading | 1 wk | gate log |
| 7 | Macro-action history (§2.4) | less myopic planning, stable prefix | 2–3 d | steps per task |
| 8 | Background window-scoped control + MCP surface (§4) | user keeps working; Team screen subtasks in parallel | 3–4 wk | concurrent tasks, focus-steal count |
| 9 | Plan-level best-of-3 for long tasks (§5) | cheap test-time compute without side effects | 3–5 d | long-task pass rate |

Order matters: #1 is the prerequisite for #4 (only verified runs become
procedures) and for honest memory credit (#6).

---

## 8. Sources (accessed 2026-10-12)

X posts/articles (read via the public fxtwitter API; treated as practitioner
claims, not verified):

- Amble, et al. (a16z), "Can Agents Use a Computer Yet? We've Got the Data", 2026-08-10 — https://x.com/seema_amble/status/2086833033097969848
- Cua (Bonacci, DuPont, Wendt), "Computer-Use 2.0 at AI Engineer World's Fair", 2026-07-13 — https://x.com/trycua/status/2077440069460181426
- alphabatcher, "Agent Memory That Improves Between Runs", 2026-09-03 — https://x.com/alphabatcher/status/2095503115130646825
- Charly Poly (Browserbase), "How autonomous does your browser agent need to be?", 2026-05-28 — https://x.com/whereischarly/status/2060048412368654663
- Stagehand v4 caching / self-healing — https://x.com/Stagehanddev/status/2090886334688932150, https://x.com/Stagehanddev/status/2086849338089857082
- Cua Driver for Windows — https://x.com/trycua/status/2059688960838828391
- Browser Use, Browser Harness JS — https://x.com/gregpr07/status/2046082887641104608

Papers:

- StructAgent — https://arxiv.org/abs/2607.11388
- EchoPath — https://arxiv.org/abs/2609.16635
- Naive Visual Memory is Not Enough (AGMem) — https://arxiv.org/abs/2606.14106
- HiViG critic — https://arxiv.org/abs/2606.11078
- OS-Oracle — https://arxiv.org/abs/2512.16295
- From Interaction Traces to Persistent Skills — https://arxiv.org/abs/2609.04869
- Work2Skill — https://arxiv.org/abs/2610.12984
- Muscle Memory for Agents — https://arxiv.org/abs/2608.08995
- Tactile — https://arxiv.org/abs/2607.14443
- LUMOS — https://arxiv.org/abs/2606.30697
- ReasoningBank — https://research.google/blog/reasoningbank-enabling-agents-to-learn-from-experience/
- Open CaptchaWorld — https://arxiv.org/abs/2505.24878
- Systematization of CUA vulnerabilities — https://arxiv.org/abs/2507.05445

Other:

- OSWorld 2.0 leaderboard — https://leaderboard.steel.dev/leaderboards/osworld-2/
- Simular Agent S3 / bBoN — https://simular.ai/articles/simulars-computer-use-agent-outperforms-humans
- Standard Intelligence FDM-1 — https://si.inc/posts/fdm1/
- OpenAI, Codex for (almost) everything — https://openai.com/index/codex-for-almost-everything/
- Cloudflare signed agents — https://blog.cloudflare.com/signed-agents/
- Cloudflare Precursor coverage — https://gigazine.net/gsc_news/en/20260714-cloudflare-precursor/
- UC Davis agent-detection study (secondary) — https://current.tinyfish.ai/issue/36/market-pulse/article/28250/the-way-they-move
- Human takeover patterns — https://hermes-agent.nousresearch.com/docs/user-guide/features/bot-screen, https://github.com/openclaw/openclaw/issues/142506
