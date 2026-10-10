# Teacher trajectories (distillation)

Gemma learns fastest from successful runs of tasks it currently fails. A
stronger "teacher" model can solve those tasks inside Kai. Its runs are then
recorded, filtered by the bench check, and mixed into SFT alongside Gemma's own
successes.

> **Terms-of-service warning, read first.** OpenAI's Terms of Use and Services
> Agreement prohibit using Output "to develop models that compete with
> OpenAI". The Gemini API Additional Terms (updated 2026-04-28) say "You may
> not use the Services to develop models that compete with the Services". A
> Gemma adapter trained on GPT or Gemini trajectories to operate a computer may
> plausibly count as a competing model. That holds even if you never
> redistribute it. Links and dates are in `README.md` § Licensing.
> **Get legal advice before training on teacher data, and before shipping
> anything trained on it.** The pipeline keeps teacher rows identifiable (the
> `model` field), so you can exclude them with one flag.
> Self-distillation on your own vLLM (Gemma's own successes, `--backend vllm`)
> is under Apache 2.0 only. It is the default and the safe path.

## 1. Record teacher runs

Kai's bench records by default (`bench/runs/`). Every run's `meta.json` and
every exported row carry `backend` and `model`.

```sh
# AI Gateway key in the keychain (kai key gateway) or AI_GATEWAY_API_KEY in env
kai bench --backend gateway --model openai/gpt-5-mini --yes
# Run it several times: temperature 0 is still not deterministic end to end
# (screens, timing), and different successful paths make better data.
for i in 1 2 3; do kai bench --backend gateway --model openai/gpt-5-mini --yes; done
# Ad-hoc tasks outside the bench (no automatic check: judge them yourself):
kai run --backend gateway --model openai/gpt-5-mini --record bench/runs "…task…"
```

Use only the bench tasks on the virtual desktop (`scripts/desktop/start.sh`).
Do not record teacher runs on your real desktop: screenshots go to the
provider.

## 2. Filter by check

`kai data export --format sft` keeps only runs whose bench check passed.
Without a check, it keeps runs whose outcome was `success`. The meta
`failure.category` tells you why the others failed (`false_done`, `loop`,
`grounding_miss`, …), which is useful to read but never used for training.

```sh
kai data export --in bench/runs --out train/data/all.jsonl --format sft --min-check true
```

The SFT loader also drops any step whose reply Kai would reject.

## 3. Mix with Gemma's own successes

```sh
# Gemma only (safe default):
python train/sft_gemma.py --config train/configs/sft_26b.yaml --data train/data/all.jsonl \
    --exclude-model-regex '^openai/|gpt' --dry-run
# Gemma + teacher, teacher replies compacted (OpenAI strict mode emits every
# field, nulls included; --compact-reply strips null/empty fields):
python train/sft_gemma.py --config train/configs/sft_26b.yaml --data train/data/all.jsonl \
    --compact-reply --dry-run
```

The dry-run prints `rows by model`, so you can see the mix before paying for a
GPU. Suggested recipe:

- **Keep teacher rows ≤ 50 %**, and prefer teacher runs on tasks where Gemma
  has no success yet. If a task already has Gemma successes, Gemma's own
  replies are closer to what it can actually do: "on-policy" data with the
  same grounding habits and reply lengths.
- **Prefer short runs.** For the same task, keep the run with fewer model
  calls. The teacher's long wandering successes teach wandering.
- **Never include held-out tasks.** The split by `task_id` enforces this,
  whatever the source.
- After SFT, go straight to `eval.py`, then GRPO. RL on Gemma's own rollouts
  corrects the habits it copied from the teacher that it cannot execute.

## 4. Keep it auditable

Record what went into each adapter. `sft_gemma.py` writes `kai_sft.json`
next to the adapter, listing the config and the train/val tasks. Keep the
exported JSONL (it lists `model` per row) with the adapter. Without that you
cannot later answer "was this trained on GPT output?"
