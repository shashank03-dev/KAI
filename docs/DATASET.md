# Kai trajectory dataset (`kai-trajectory/1`)

Every Kai run can be saved as training data, and runs that fail are labelled
automatically. The aim is a data flywheel: run the bench (and real tasks), keep
what went right as supervised examples, keep what went wrong as labelled
failures, export both, and train Gemma on them.

## Recording

| Where | Default | How |
|---|---|---|
| `kai bench` | **on** | every task run goes to `bench/runs/<date>-<backend>-<model>/<task-id>/`. `--runs <dir>` changes the root, `--no-record` turns it off. A second run of the same task in the same batch goes to `<task-id>.2`, `.3`, … so nothing is overwritten. |
| `kai run` | off | `kai run --record <dir> "task"` writes `<dir>/<YYYYmmdd-HHMMSS>-<task slug>/`. Not supported with `--team`. |
| Desktop app | off | setting `record_traces` (`Settings::runs_dir()`, i.e. `<config dir>/Kai/runs/`). |

`bench/runs/` is in `.gitignore`. Recording costs nothing extra on the model
side: it writes what was already sent and received.

In code, recording is one optional field on the per-run options, and the
`Engine` struct does not change:

```rust
let rec = kai_core::TraceRecorder::new(dir, kai_core::RunMeta::new(run_id, task, "app", backend, model))?;
let trace = engine.run_with(task, kai_core::RunOptions { recorder: Some(&rec), ..Default::default() }).await;
// The engine writes trace.json + meta.json when the run ends. A caller that
// knows more (the bench's check command) relabels it:
rec.set_verdict(&trace, Some(check_passed), None)?;
```

## Layout of one run

```
<root>/<run-id>/
  meta.json      what the run was and how it ended
  trace.json     kai_core::Trace, steps carry the exact model call
  system.txt     the system prompt (once per run)
  frames/
    step-000.jpg       the exact marked JPEG sent at step 0
    step-004-crop.jpg  a zoom crop, when one was sent
```

### `meta.json`

```json
{
  "schema": "kai-trajectory/1",
  "run_id": "2026-10-10-gateway-google-gemma-4-26b-a4b-it/write-note",
  "task": "Create a file called kai-note.txt in my home folder that says \"Hello from Kai\".",
  "task_id": "write-note",
  "source": "bench",
  "backend": "gateway",
  "model": "google/gemma-4-26b-a4b-it",
  "kai_version": "30ac4f1-dirty",
  "os": "linux",
  "screen": [1280, 800],
  "started": "2026-10-10T06:06:32+00:00[Etc/UTC]",
  "check": true,
  "outcome": "success",
  "failure": null
}
```

- `task_id` is set for bench runs. `source` is `bench`, `run` or `app`.
- `kai_version` comes from `git describe` when run inside the repo, otherwise
  it is the crate version.
- `screen` is the size of the captured display when the perceptor reports it,
  else `null`.
- `check` is the bench's automatic check (exit 0 = `true`), or `null` when
  there was none. The mock backend never runs checks.
- `outcome` is how the engine says the run ended: `success`, `failed`,
  `stopped`, `step_limit`, `time_limit` or `asked_user`.
- `failure` is `null` when the run passed, else `{category, detail}` (see
  below).

A run **passed** when `check == true`, or when there is no check and
`outcome == "success"`.

### `trace.json`

This is the usual `kai_core::Trace` (the app's history view reads the same
format). Recorded runs fill these optional per-step fields; each one is left
out when empty, so old traces still load:

| Field | Meaning |
|---|---|
| `prompt_system_hash` | hex FNV-1a (64-bit) of the system prompt; the text is in `system.txt` |
| `prompt_user` | the exact user turn sent this step |
| `reply_raw` | the exact raw reply text, before parsing (after a repair retry, the reply that decoded) |
| `model` | model id that answered (the careful model's id when Kai escalated) |
| `prompt_tokens`, `completion_tokens` | usage the backend reported (summed over a repair retry) |
| `screenshot` | `frames/step-NNN.jpg`, the marked JPEG sent; absent on tree-only steps |
| `crop` | `frames/step-NNN-crop.jpg` when a zoom crop was sent |

Steps with no model call (follow-ups from a reply's `then` list, plan-cache
replays, skill steps) are recorded as steps without these fields. Their
`backend` label says which kind they are.

The trace also has an `errors` list: model-call and observation errors that
produced no step, for example `malformed reply at step 3: ...` or
`model error at step 5: backend returned status 429: ...`. The failure labels
use it. Every trace has this list, recorded or not.

## Failure taxonomy

`kai_core::failure::classify(&trace, check)` labels every run that did not
pass. The first rule that matches wins:

| Category | Rule |
|---|---|
| `rate_limited` | an error mentions 429, a rate limit, a quota or too many requests |
| `false_done` | the model said `done` (outcome success) but the check failed |
| `malformed` | two or more replies that would not decode, even after the repair retry |
| `env_error` | an action errored (`path: error` / `ERROR:` output), a shell command timed out, an observation failed, or (bench) the task's setup command failed |
| `loop` | the same action on the same target (and text/keys) three or more times; `wait` and `scroll` are not counted |
| `step_limit` | the run hit the step limit |
| `time_limit` | the run hit the time limit |
| `gave_up` | the model chose `ask_user`, or Kai stopped and asked (no progress, repeated errors) |
| `grounding_miss` | best effort: an unverified click whose thought quotes a name (`click "Save"`) that the clicked mark does not carry |
| `needs_human` | the run hit a CAPTCHA, one-time code, sign-in approval or question that nobody answered (headless run, timeout, or too many asks); see `docs/HUMAN_CHECKS.md` |
| `other` | anything else (stopped by the user, failed) |

The bench results JSON gets a `failure` per task and a `failures`
histogram, and the Markdown gets a Failure column plus a
`Failure categories:` line. These are new fields; no existing field was renamed.

## Export

```
kai data export --in bench/runs --out data/all.jsonl                  # every model call
kai data export --in bench/runs --out data/sft.jsonl  --format sft    # passed runs only
kai data export --in bench/runs --out data/grpo.jsonl --format grpo   # one row per task
kai data stats  --in bench/runs [--json]
```

Filters work with every format: `--only-success` keeps runs that passed, and
`--min-check true|false` keeps runs whose check had that value (runs with no
check are dropped).

### `all` and `sft`: one JSON object per model call

Only steps that have `reply_raw` count:

```json
{
  "id": "<run-id>/<step>",
  "task": "...", "task_id": "write-note", "step": 0,
  "system": "<system.txt>",
  "user": "<prompt_user>",
  "images": ["/abs/path/bench/runs/.../frames/step-000.jpg"],
  "reply": "<reply_raw>",
  "decision": { "thought": "...", "action": "shell", "...": "..." },
  "run_outcome": "success", "check": true, "step_verified": true,
  "backend": "gateway", "model": "google/gemma-4-26b-a4b-it",
  "latency_ms": 1840, "source": "bench"
}
```

`images` are absolute paths: the screenshot first, then the crop if there was
one. The list is empty on tree-only steps (text only). `sft` keeps only steps of runs that
passed (check true, or outcome success when there is no check). `all` keeps
every run, so failed runs can be used as negatives or for analysis.

### `grpo`: one JSON object per task

Rows are keyed by `task_id` (or by the prompt when there is none). Setup,
check and teardown come from the task files (`--tasks`, repeatable; default
`bench/tasks.yaml` plus `bench/variants.yaml` if present):

```json
{
  "id": "write-note", "task_id": "write-note", "task": "<prompt>",
  "kind": "terminal", "needs": [],
  "setup": {"linux": "...", "windows": "..."}, "check": {...}, "teardown": null,
  "variant_of": null,
  "runs": 3, "passed": 2, "failures": {"loop": 1},
  "run_dirs": ["/abs/path/bench/runs/.../write-note", "..."]
}
```

An RL environment resets the machine with `setup`, lets the policy act, and
scores the episode with `check` (exit 0 = reward 1). `run_dirs` points at
reference trajectories.

### How `train/` uses it

The training scripts are being written separately:

- `train/sft_gemma.py` reads `--format sft` JSONL. Each row is one chat
  example: `system`, then the `user` text with `images` attached, then the
  `reply` as the target.
- `train/grpo_env.py` reads `--format grpo` JSONL as its task list. It runs
  `setup` and `check` on a disposable desktop (VM or container) and uses the
  check as the reward.

## Task variants (more training data from the same tasks)

A task in `bench/tasks.yaml` may declare `vars:`:

```yaml
- id: write-note
  prompt: Create a file called {note_file} in my home folder that says "{note_text}".
  vars:
    note_file: [kai-note.txt, reminder.txt, todo.txt]
    note_text: [Hello from Kai, Meeting moved to 3pm, Pick up the parcel at 5]
  check:
    linux: grep -q "{note_text}" ~/{note_file}
```

`{name}` is replaced in the title, prompt, setup, check, teardown and recorded
replies. The bench **always** runs the first value of each var. That rendering
is the same text the task had before vars existed, so head-to-head numbers stay
comparable. Values may not contain quotes, backslashes, `$`, backticks or
braces, because they go into shell commands.

```
kai bench variants --tasks bench/tasks.yaml --out bench/variants.yaml --n 3
kai bench --backend gateway --tasks bench/variants.yaml     # recorded under bench/runs/
```

Variants are written with ids `<id>--v<k>` and `variant_of: <id>`. They are for
**training data only, never for the head-to-head numbers**. The file's header
says so, and a bench run over variants writes its results to
`bench/results/variants/`, which `kai bench report` does not read.

Tasks with vars today: `fill-form`, `write-note`, `rename-files`,
`find-largest`, `search-in-files`, `mousepad-write`, `email-draft`,
`calendar-event`.

## Privacy

- **Screenshots can contain private data**: whatever was on screen (mail,
  documents, names, notifications) ends up in `frames/`. The element list and
  the user turn in `trace.json` hold the same text. Review a dataset before
  you share it, and record on a clean test desktop when you can.
- Recording is **local only**. Kai never uploads recorded runs. Nothing leaves
  the machine except the model call itself, which happens whether or not you
  record.
- Recording is **off by default** in the app (`record_traces`) and in
  `kai run`. It is on by default only for `kai bench`, which runs on the bench
  fixtures.
- API keys are never recorded. They are not part of prompts, replies, traces
  or meta. The element list never shows a secure field's value. However,
  any text the model types (including a password a task asks for) appears in
  that step's `reply_raw` and `decision`. Do not record runs that use real
  credentials.
- Delete a run by deleting its folder.
