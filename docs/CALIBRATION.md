# Coordinate calibration and OpenAI-contract A/B runs

Status (2026-10-16): fixture, scorer and A/B runner implemented; **model
calibration and valid paired A/B measurements still outstanding**. One real
Gemma 4 A/B attempt has two baseline completions and two contract runs halted
by human approval requirements, so its reported comparison is invalid:
`bench/results/oai-contract-ab/gemini/gemma-4-26b-a4b-it/20261016T132320Z/`.
The local Holo3.1 4B GGUF download resumed with byte ranges but is incomplete
(1,346,817,006 bytes of the main weight file as of this check, plus the
projector still required). The incomplete `.part` was preserved for a future
resumable `kai models install holo-3.1-4b-q4`; it cannot serve calibration or
benchmark requests. Its preflight record lives under
`bench/results/oai-contract-ab/local/holo-3.1-4b-q4/`.
See `RESEARCH_2026_10_OPENAI_PARITY.md` §5b and §7 Phase 2 for the
experiment requirements.

## Design and boundaries

The local fixture `bench/fixtures/site/calibration.html` places a 64×64 red
square with its center at `(0.75 × viewport width, 0.60 × viewport height)`.
It has no scrolling, remote assets, text labels containing the answer, or
browser-side randomness. The specified viewports are **1280×800** (center
960,480) and **1440×900** (center 1080,540). Only the **captured screenshot
pixels**, excluding browser chrome, count as the viewport. Use device pixel
ratio 1 and verify PNG dimensions before asking a model.

Ask the model *"Click the center of the red square; return only the raw x,y
coordinates in your usual computer-use coordinate system."* Ten independent
predictions per resolution are required. Capture and retain the exact image,
model id/version, processor and sampling settings, and raw output separately.
Never prime the model with the target coordinates or a proposed coordinate
frame. The input to `scripts/calibrate_coordinates.py` is JSON Lines, one
object per prediction: `{"width":1280,"height":800,"x":960,"y":480}`.
If a `screenshot` PNG path is present, the evaluator checks its intrinsic
dimensions and records its SHA-256 in its result. Store screenshots/outputs in
a private scratch directory if they contain personal information.

The evaluator scores three hypotheses in **screenshot pixels**: `pixels`,
`norm1000`, and Qwen2.5-compatible `smart-resize` (28-pixel factor,
78,400 minimum and 12,845,056 maximum pixels). Each hypothesis reports the
mean center error and target hit rate at each size. It selects a frame only
if both sizes have at least ten samples, the best hypothesis has at least 9/10
hits per size, its mean error is at most 16 px, and it beats the next best
candidate by at least 10 px mean error. Otherwise the result is `ambiguous`
or `insufficient_data`. Smart-resize settings for Qwen3.5/Holo3.1 are **not
established**: use the smart-resize hypothesis only if the served processor
actually uses those settings. Close candidates intentionally remain
ambiguous instead of producing an unjustified catalog entry.

Considered and rejected: choosing Holo3.1's frame from model lineage alone;
assuming a click inside the square proves the precise frame; and enabling
pixel behavior on the basis of synthetic predictions. The fixture can prove
the scorer works, but only recorded model predictions support calibration.
Do not update the model catalog until a reproducible report identifies a
frame unambiguously; Gemma 4 continues to use `marks` by default.

## Reproduce calibration

Start the fixture with `python3 bench/fixtures/server.py --port 8765` and open
`http://127.0.0.1:8765/calibration.html` in a browser. Capture exactly the
two viewport sizes above; inspect the resulting PNG dimensions and visually
check that the square has the specified center. Submit each screenshot to
the model ten times per viewport, with the prompt above and constant model
settings. Save one JSONL row per raw reply. Use `--input` with `--model` and
`--out` to produce a report under `bench/results/`:

```sh
python3 scripts/calibrate_coordinates.py \
  --input /path/to/raw-coordinate-predictions.jsonl \
  --model holo-3.1-4b-q4 \
  --out bench/results/calibration/holo-3.1-4b-q4.json
```

Keep the input and captured images for audit. The report includes raw input
SHA-256, individual errors, complete trial counts and a decision reason; a
reported classification is not itself evidence that the model is reliable on
unseen interfaces. A different viewport, DPR, cropping, model checkpoint, or
image processor invalidates comparisons across runs.

## Reproduce the paired benchmark

`scripts/bench_oai_contract.py` runs the existing `kai bench` harness; it
does not implement a second task runner or change the core execution path.
The default `plan` command prints a four-run sequence for a chosen backend
and model: baseline-1, contract-1, baseline-2, contract-2. Every run uses
the same explicit `--only` task IDs, `--no-record`, and a unique results
directory. Supply a working `kai` binary rather than triggering a build.

```sh
python3 scripts/bench_oai_contract.py plan --backend vllm \
  --model gemma-4-26b-a4b-it --frame marks \
  --only fill-form --only web-form-submit

python3 scripts/bench_oai_contract.py plan --backend local \
  --model holo-3.1-4b-q4 --frame norm1000 \
  --calibration bench/results/calibration/holo-3.1-4b-q4.json \
  --browser --headless \
  --only fill-form --only web-form-submit
```

For actual execution, first verify the model is serving, credentials are
available to Kai through its configured secret source, and the desktop or
headless browser works. The script requires `run --execute` and a preflight
command returning exit code 0 for this verification. It refuses to run a
pixel-mode Holo experiment without a matching, successful calibration report.
Use `--frame marks` to measure Holo's mark baseline where appropriate, and
record it as a **separate** experiment; baseline-vs-contract holds the model,
tasks and environment constant, but the contract itself changes prompting.
Do not put tokens, API keys, private screenshots, or raw sensitive model
outputs into committed results. No live trial is performed by `plan`.

```sh
python3 scripts/bench_oai_contract.py run --execute \
  --kai-bin /absolute/path/to/kai --backend vllm \
  --model gemma-4-26b-a4b-it --frame marks \
  --preflight-cmd '/path/to/verify-model-and-desktop' \
  --only fill-form --only web-form-submit

python3 scripts/bench_oai_contract.py report \
  --root bench/results/oai-contract-ab --backend vllm \
  --model gemma-4-26b-a4b-it --experiment 'id-from-run'
```

The runner stores a manifest containing exact arguments and completion
status. The reporter requires two completed runs **per side**, identical
task IDs, no skipped tasks, and consistent backend/model and execution
settings. It reports each task's pass result and aggregated success, steps,
model calls and seconds. Format repairs and misclicks are `unavailable`
unless the runner's JSON records them; no estimate is substituted. If
missing or invalid files are present it fails instead of publishing a
comparison. `--out` uses separate directories because Kai's dated output
filenames otherwise overwrite repeat trials.

These scripts intentionally do not automatically buy API tokens, download
models, start a desktop, or alter catalog settings. Live results are only
meaningful if the runtime, hardware, task list, model, and date are recorded.
