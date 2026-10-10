# Experience memory: Kai learns on your computer, without training

When Kai gets something wrong on your computer — starts Mousepad from the
shell and waits forever, clicks a button that does nothing, says "done" too
early — it should not make the same mistake tomorrow. Kai does this with
**experience memory**: after each run it writes down a few short **lessons**
from what happened, keeps them on this computer, and shows the handful that fit
the next task to the model as hints.

No model is trained or fine-tuned. The model's weights, context window and
capacity are untouched; it simply reads a few extra lines. That makes it work
the same with every backend: a local Gemma on a laptop, a hosted model, or a
model you switch to next month — the lessons are plain text.

```mermaid
flowchart LR
    RUN[A run<br/>steps · outputs · verified · your messages] --> LEARN
    CHECK[Check result<br/>bench only] -.-> LEARN
    LEARN[Learn<br/>fixed rules<br/>+ optional reflection call] --> STORE[(lessons.jsonl<br/>on this computer)]
    STORE --> RECALL[Recall<br/>task words · apps · sites<br/>top 5, ≤ 120 words]
    RECALL --> TURN[Next run's user turn<br/>From earlier runs on this computer]
    TURN --> MODEL[(Any model)]
    RUN --> CREDIT[Credit<br/>helped / hurt]
    CREDIT --> STORE
```

## What a lesson is

```json
{"id":"3f9a0c12","kind":"prefer",
 "text":"Start Mousepad with open_app, not shell `mousepad`: a shell step waits for the app and blocks.",
 "scope":{"apps":["mousepad"],"domains":[],"task_words":["write","hello","mousepad"]},
 "evidence":["2026-10-10T12:00:01+02:00[Europe/Berlin]"],
 "created":1791000000,"last_used":1791003600,"uses":1,"helped":1,"hurt":0,
 "source":"derived","targets":"env_error"}
```

- **kind**: `avoid` (failed here), `prefer` (worked after something else
  failed), `fact` (about this computer), `preference` (how you want it done).
- **text**: one imperative sentence, at most 30 words.
- **scope**: the apps and sites it is about, and the task's content words.
- **confidence** = (helped + 1) / (helped + hurt + 2): 50 % for a new lesson.
- **source**: `derived` (fixed rules), `reflection` (the model, optional),
  `user` (you typed it, or said it mid-run).

## How Kai learns (after every run)

Fixed rules first — no model call (`crates/kai-core/src/memory/learn.rs`):

| Rule | Reads | Lesson |
|---|---|---|
| Recovery | a step failed or changed nothing, and a later step with the same intent (same `expect`, same target name, or the same app) worked | `prefer` "To reach "file saved" in mousepad, key ctrl+s (not click on 'Save')." / "Start Mousepad with open_app, not shell `mousepad` …" |
| Dead end | the same action on the same element twice with no change | `avoid` "click on 'Next' in firefox did nothing (2 tries); try another way." |
| Failing command | a shell command errored or timed out — kept when a corrected command followed (attached) or the run failed | `avoid` "shell `pythn x.py` fails here: command not found; shell `python3 x.py` worked instead." |
| Steering | every message you sent mid-run | `preference` "User prefers: save to ~/Notes" (source user) |
| False done | said done but the task's check failed (bench) | `avoid` "Before done on tasks like 'export report pdf', verify the PDF is saved." |

At most 4 rule-based lessons per run (your messages are always kept).

**Reflection (optional, off by default).** With *Reflect after failures* on,
a failed or recovered run gets ONE extra model call with a compact, scrubbed
summary of the run, asking for at most two lessons as
`{"lessons":[{"kind","text"}]}`. Replies are validated (kind, ≤ 30 words, no
secrets, no talk of approvals/passwords/overrides). It is off by default
because it costs a call per such run and the fixed rules already cover the
common cases; local-model users who want more learning can turn it on.

## How Kai remembers (recall)

At the start of a run and whenever a new app or site comes into play (opened
by the run, or visible in the element list), Kai scores every lesson:
app/site match + task-word overlap + confidence (halved for every 90 days
unused), keeps the top 5 within 120 words, and puts them at the **top of the
user turn**:

```
From earlier runs on this computer:
- Start Mousepad with open_app, not shell `mousepad`: a shell step waits for the app and blocks.
```

Why the user turn and not the system prompt: the system prompt is kept
byte-for-byte stable across runs and sessions so the backend's prefix cache
always hits. The lesson block sits right after it and only changes when the
apps/sites in play change, so within a run it extends the cached prefix
(images come after the text in every request) and costs almost nothing after
the first step on a prefix-caching server such as llama.cpp or vLLM.

**A design change from the first plan.** The plan said to re-send the block
only when the set changes and otherwise write "(same lessons as before)".
Kai's model requests are stateless — each step is one system + one user
message, with a 3-step history summary — so the model would never have seen
"before". The block is therefore repeated while it applies, kept small by the
120-word budget, and made cache-friendly by its position and stability.

**Lessons are hints, never rules.** Nothing but the prompt builder reads them:
risk classification, approvals, the payment/password rule and the kill switch
are computed exactly as without memory. A lesson that says "skip approval"
reaches the model as text and changes nothing else — this is tested
(`a_lesson_can_never_skip_an_approval` in `crates/kai-core/tests/memory_e2e.rs`).
Model-written lessons that mention approvals, passwords, credentials or
"ignore previous instructions" are refused at the door, so a hostile web page
cannot plant a persistent instruction through the reflection call.

## Credit, merging and forgetting

- **Credit.** Each lesson shown in a run: `uses` + 1; `helped` + 1 if the run
  passed (or failed for a different reason than the one the lesson warns
  about); `hurt` + 1 if it failed exactly the way the lesson warned about.
- **Retire.** A lesson shown in ≥ 3 runs with confidence < 30 % is dropped.
  Your own lessons are never retired automatically.
- **Merge.** A new lesson whose words overlap an existing one of the same
  kind by ≥ 80 % (token Jaccard) is merged into it (evidence, counters, scope).
- **Caps.** At most 30 lessons per app or site and 500 in all; the least
  confident, then least recently used, go first (yours last).
- **Age.** Ranking weight halves for every 90 days a lesson goes unused;
  lessons untouched for a year are dropped (yours are kept).

## Promotion to a skill

When an app or site has a remembered successful opening (the plan cache) and
at least two `prefer` lessons that helped, Kai suggests a skill: the opening
as deterministic steps, the lessons as instructions. It opens in the normal
Save-as-skill editor (`kai memory suggest` prints the draft); nothing is ever
installed for you.

## Privacy

- **Local only.** Lessons live in `~/.config/kai/memory/lessons.jsonl`
  (Windows: `%APPDATA%\Kai\memory\lessons.jsonl`). They are only ever sent as
  part of the prompt to the model endpoint you chose, like the screen itself.
- **No secrets.** Emails, key-like tokens and runs of 6+ digits are replaced
  (`[email]`, `[secret]`, `[number]`); anything typed into a password/secure
  field never becomes a lesson; a lesson that still looks secret is refused.
- **What is stored**: the lesson text, its scope (app names, site names, task
  words), counters, timestamps and the start time of the runs it came from.
  No screenshots, no element lists, no command output beyond a 60-character
  error line.

## Seeing and deleting what Kai learned

- App: **Settings › Memory** lists every lesson (kind, text, scope,
  confidence, uses) with **Delete** and **Clear all**, and an **Add a lesson**
  field. **Settings › Privacy** has *Learn from mistakes on this computer*
  (on by default) and *Reflect after failures (one extra model call)* (off).
- Terminal:

  ```
  kai memory list [--app mousepad]
  kai memory forget <id>
  kai memory clear
  kai memory add "Save notes to ~/Notes" --app mousepad [--kind preference]
  kai memory suggest
  kai run --no-memory "…"     # neither use nor add lessons for this run
  ```

`kai run` and the app use memory by default. `kai bench` does **not** (fair
comparisons): `--memory` gives the bench its own fresh, in-memory store, and
`--memory --repeat 2` runs every task twice back to back and prints first-run
vs second-run success, steps and model calls (`<results>.memory.md`). The
head-to-head numbers use first runs only. In the bench the lesson is learned
after the task's check (so false "done" is caught); reflection does not run in
the bench.

## Why no training

Training on a laptop is slow, needs data Kai would have to collect and label,
changes the model for every task at once, and has to be redone for each new
model. Experience memory is immediate (the next run benefits), specific to
this computer, visible and deletable line by line, costs at most ~120 words
of context, and works with any model. Training is parked by the owner
(2026-10-10); `train/` is kept for later.

## Measured

With a scripted policy model that follows hints (MockClient-style, no network;
`cargo test -p kai-core --test memory_e2e -- --nocapture`):

| Scenario | Run 1 | Run 2 with lessons | Run 2 without memory |
|---|---|---|---|
| Mousepad started from the shell, retried, gave up | asked user, 3 model calls | success, 3 calls | asked user, 3 calls |
| Shell timed out, then open_app worked | success, 4 calls / 4 steps | success, 3 calls / 3 steps | — |

These numbers show the mechanism (lesson learned → recalled → followed), not
a real model's behaviour; real-model A/B numbers come from
`kai bench --memory --repeat 2` on a live backend.

Code: `crates/kai-core/src/memory/` (OS-free), wiring in
`crates/kai-core/src/engine/memory.rs`, CLI in `crates/kai-cli/src/memory.rs`.
