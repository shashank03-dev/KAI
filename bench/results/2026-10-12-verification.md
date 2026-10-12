# Verification gate (outcome ledger + DONE gate + per-action effect): before / after

Date: 2026-10-12. Design: [docs/VERIFICATION.md](../../docs/VERIFICATION.md).
Raw files: [`2026-10-12-verification/`](2026-10-12-verification/). They are
kept in a subfolder so `kai bench report` does not merge them into the
head-to-head table.

## The question, and why this run cannot answer it

The question was whether false done goes down without the pass rate going
down, on a live model. **That was not measured.** This container has:

- no model key: `GEMINI_API_KEY`, `AI_GATEWAY_API_KEY` and `OPENAI_API_KEY`
  are unset, there is no keychain entry, and
  `generativelanguage.googleapis.com` answers 403 through the egress proxy;
- no browser and none of the GUI apps the GUI tasks need (`firefox`,
  `mousepad`, `pcmanfm`, LibreOffice), so even with a key the web tasks,
  `web-form-submit` (the measured false done) among them, would be skipped.

A recorded reply cannot measure the gate either. The bench's recorded
replies do not name outcomes, and writing outcomes into them by hand would
mean grading my own homework. So what was measured below is only this:
**does the change cost anything or break anything when the gate does not
fire**, on the same tasks with the same backend, before and after. The
mechanism itself (a failing probe refuses `done`, refusals are bounded, the
model cannot verify, nothing secret leaks) is proven by the tests in
`crates/kai-core/tests/verify_e2e.rs`, not by these numbers.

**Ship decision:** the feature is in the code, **off by default**
(`--outcomes`, *Settings › Safety*). No gain is claimed.

## How it was measured

- *Before*: a clean `git worktree` of `25db5e8` (this branch's base).
  *After*: this branch. The same `bench/tasks.yaml`, the same container
  (4 vCPU, Ubuntu 24.04, Xvfb `:99` at 1920x1080x24), debug builds, run one
  after the other.
- **Mock** (`kai bench --backend mock --no-record`): all 30 tasks' recorded
  replies on a scripted screen with dry-run hands. Mock timings are zero by
  design, so this checks the plumbing and the step and call counts, not
  latency.
- **Replay** (`DISPLAY=:99 kai bench --backend replay --yes --kind terminal
  --kind mixed --no-record`): the 20 terminal and mixed tasks' recorded
  replies run for real on this desktop. That means real capture, a real
  accessibility read (there is no AT-SPI bus here, so the tree is empty and
  the event counter is off), real shell, and each task's real `check`. Two
  runs before, two after with the gate off, and one after with `--outcomes`.
  No model is called (model time 0), so step time is Kai's own overhead.

## Results

| Run | Pass | False done / gated, never verified | Gate refusals | Steps per task | Model calls (per task) | Step p50 / p95 ms | Observe p50 / p95 ms | Act p50 / p95 ms |
|---|---|---|---|---|---|---|---|---|
| mock, before | 30 / 30 | — (field did not exist) | — | 2.97 | 85 (2.83) | 0 / 0 | — | — |
| mock, after | 30 / 30 | 0 / 0 | 0 | 2.97 | 85 (2.83) | 0 / 0 | — | — |
| mock, after, `--outcomes` | 30 / 30 | 0 / 0 | 0 | 2.97 | 85 (2.83) | 0 / 0 | — | — |
| replay, before #1 | 19 / 20 | — | — | 2.40 | 47 (2.35) | 110 / 251 | 106 / 128 | 3 / 141 |
| replay, before #2 | 19 / 20 | — | — | 2.40 | 47 (2.35) | 109 / 193 | 103 / 136 | 3 / 100 |
| replay, after #1 | 19 / 20 | 0 / 0 | 0 | 2.40 | 47 (2.35) | 110 / 231 | 104 / 128 | 3 / 113 |
| replay, after #2 | 19 / 20 | 0 / 0 | 0 | 2.40 | 47 (2.35) | 104 / 225 | 100 / 130 | 3 / 122 |
| replay, after, `--outcomes` | 19 / 20 | 0 / 0 | 0 | 2.40 | 47 (2.35) | 107 / 208 | 101 / 127 | 3 / 109 |

The one replay failure is the same in every run: `dark-mode`, failure
`env_error: task setup failed`. Its setup runs `gsettings set
org.gnome.desktop.interface …`, and that GNOME schema is not installed here
("No such schema").

Reading:

- **No regression.** The pass rate, the steps per task, the model calls and
  the per-task results are the same in every run.
- **No measurable overhead.** Step p50 is 104–110 ms after and 109–110 ms
  before. The p95 spread (193–251 ms) is noise between runs of the same
  binary, not a difference between builds. When the gate is off it costs
  one counter read and one label per action.
- **`--outcomes` changes nothing here,** as it should: the recorded replies
  name no outcomes, so the gate never fires.
- **False done is 0 in every run,** which says nothing about the gate:
  recorded replies only reach `done` when the recording was right.

## Guard removal (every guarantee has a test that fails without it)

[`guard-removal.py`](2026-10-12-verification/guard-removal.py) removes each
guard in turn, runs the test that holds it, and puts the guard back. It was
run against the final code:

```
gate check                       -> FAILS (guard holds)
status from reply                -> FAILS (guard holds)
refused probe drops outcome      -> FAILS (guard holds)
allowlist                        -> FAILS (guard holds)
allowlist (lib)                  -> FAILS (guard holds)
timeout                          -> FAILS (guard holds)
bound                            -> FAILS (guard holds)
bound headless                   -> FAILS (guard holds)
waiver                           -> FAILS (guard holds)
redaction                        -> FAILS (guard holds)
events                           -> FAILS (guard holds)
```

Workspace tests: 361 passed before, 388 passed after (27 new), 0 failed.
`cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D
warnings` and `kai skill validate skills` (10/10) are clean.

## To get the real answer

On a machine with a key and the GUI apps (the 2026-10-10 runs used the
Gemini free tier and the same 14 tasks):

```sh
kai bench --backend gemini --model gemma-4-26b-a4b-it --yes \
  --only search-web --only fill-form --only web-form-submit --only download-file \
  --only rename-files --only find-largest --only zip-folder --only open-folder \
  --only mousepad-write --only close-app --only tip-calculator --only calendar-event \
  --only resize-image --only screenshot
# the same command plus --outcomes
```

Compare pass rate, the "False done / gated, never verified" row, steps per
task and model calls. Run each side at least twice (one run per model was
too noisy on 2026-10-10). Turn the default on only if false done goes down
and the pass rate does not.
