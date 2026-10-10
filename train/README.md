# Training Gemma 4 to be a better Kai

> **Note (2026-10-10):** Model training is parked by the owner (2026-10-10); Kai improves through local experience memory ([docs/MEMORY.md](../docs/MEMORY.md)) instead. train/ is kept for later.

This folder makes Gemma 4 better at Kai's computer use over time, with Kai's
own benchmark as the judge. Everything runs on one DigitalOcean H100 Droplet
(`deploy/digitalocean/train-up.sh`). **Nothing here has been run on a GPU
yet.** The scripts were written against the library versions listed below.
Every script has a `--dry-run` that checks the data end to end without torch,
and `train/tests/run_all.sh` runs all of them.

## The loop

```
record ──► export ──► SFT (LoRA) ──► evaluate on held-out tasks ──► GRPO (online RL) ──► evaluate ──┐
  ▲                                                                                                │
  └──────────────── more recordings: the new adapter's own successful runs ◄───────────────────────┘
```

1. **Record.** `kai bench` records every run to `bench/runs/<run-id>/` by
   default (format `kai-trajectory/1`: `trace.json`, `system.txt`,
   `meta.json`, `frames/`). `kai run --record <dir>` records ad-hoc tasks.
   The app records only if you turn on *record traces* (off by default).
   ```sh
   kai bench --backend vllm --model gemma-4-26b-a4b-it --yes   # base model, served by up.sh
   ```
2. **Export.** One JSONL row per model call. `sft` keeps only steps of runs
   whose bench check passed (or, with no check, outcome `success`).
   ```sh
   kai data export --in bench/runs --out train/data/sft.jsonl --format sft
   kai data export --in bench/runs --out train/data/grpo.jsonl --format grpo
   ```
3. **SFT.** `train/sft_gemma.py` trains a LoRA adapter on that JSONL. It needs
   a GPU (sizes below).
   ```sh
   python train/sft_gemma.py --config train/configs/sft_26b.yaml --data train/data/sft.jsonl --dry-run
   python train/sft_gemma.py --config train/configs/sft_26b.yaml --data train/data/sft.jsonl
   ```
4. **Evaluate.** Serve the adapter and run the held-out bench tasks:
   ```sh
   deploy/digitalocean/serve-lora.sh train/out/sft-26b/adapter      # served as "kai"
   python train/eval.py --model kai --label sft-26b-v1               # appends to train/results.md
   ```
   This runs `kai bench --backend vllm --model kai --only <held-out ids>`.
   It prints the result next to the same tasks from every file in
   `bench/results/`. Numbers come only from those files.
5. **GRPO.** `train/grpo_gemma.py` runs the policy on the training bench tasks
   N times each, on parallel virtual desktops. The bench check is the reward.
   The script updates the LoRA, hot-loads it into vLLM, and repeats (design
   below).
6. **Repeat.** Re-record with the improved adapter (`kai bench --backend vllm
   --model kai`), export again, and run SFT on the union (the self-improvement
   step). Run GRPO again from the new adapter, and evaluate after each stage.
   Adopt an adapter only if held-out success goes up and steps per task do not
   get worse.

### What the model learns, exactly

Each training example is one model call, rendered as Kai sends it to vLLM
(`crates/kai-core/src/client/vllm.rs`, `prompt.rs`):

- **system**: Kai's stable system prompt (`SYSTEM_BASE` plus the skill list),
  taken from the run's `system.txt`.
- **user**: the user turn text (elements list, last 3 steps, notes, `Task:
  …`, `Respond with the next action as JSON.`), then the marked screenshot,
  then the zoom crop if one was sent. Text comes first, as in Kai's request.
  The Gemma 4 model card suggests putting images before text. Changing the
  order would mean changing Kai, so it stays as Kai sends it.
- **assistant**: the raw reply, i.e. one JSON object matching
  `StepReply::json_schema()` (`thought`, `action`, `target`, `text`, `keys`,
  …, optional `then` and `need_screen`). Rows whose reply Kai would reject are
  dropped. `train/kai_data.py:check_reply` ports the Rust validation rules.

The prompt is rendered with `add_generation_prompt=True`. With thinking off,
Gemma 4's chat template then adds an empty thought channel
(`<|turn>model\n<|channel>thought\n<channel|>`), and vLLM's chat endpoint
does the same at serving time. Rendering the whole conversation in one go
would leave that channel out, so training would not match serving. The SFT
script therefore builds `prompt(add_generation_prompt) + reply + "<turn|>"`
and computes loss only on the reply and `<turn|>`. The dry-run's
`--chat-template` check compares the built-in renderer against the published
`chat_template.jinja`. It passed (20 of 20 synthetic rows matched).

Images are used as Kai recorded them: already downscaled to Gemma 4's
280-soft-token budget (1070x602 for 16:9). Larger frames from other sources
are downscaled with Kai's own rule (`FrameConfig::target_size`). The
processor then snaps the image to multiples of 48, exactly as vLLM does.

**Split by task.** Rows are grouped by bench `task_id` (or a hash of the task
text for non-bench runs). Tasks listed in `train/heldout.txt`, plus an
optional hashed `--val-fraction`, never reach training. That includes GRPO,
which refuses them. The script asserts that no task appears on both sides.

## Files

| File | What |
|---|---|
| `kai_data.py` | Contract loader and validator, reply checker, image rule, split, chat rendering, bench tasks. No torch. |
| `sft_gemma.py` | LoRA SFT with `transformers.Trainer` and PEFT, loss on reply tokens only. `--dry-run`. |
| `grpo_env.py` | The RL environment: allowlist, desktops, `kai run` episodes, reward, advantages, `FakeRunner`. |
| `rollout_proxy.py` | Localhost proxy between Kai and vLLM: sampling temperature, logprobs, per-episode call logs. |
| `grpo_gemma.py` | The GRPO loop and the explicit policy-gradient update. `--dry-run`. |
| `eval.py` | Held-out bench evaluation, comparison with `bench/results/`, a row in `results.md`. |
| `distill.md` | Teacher trajectories (GPT and others) and the terms-of-service question. |
| `configs/*.yaml` | `sft_26b`, `sft_e4b`, `sft_31b`, `grpo_26b`. |
| `heldout.txt` | The 6 held-out bench tasks. |
| `tests/` | Synthetic-data generator, proxy test, `run_all.sh`. |

## GRPO design (Kai is the environment)

- **Episode**: one allowed bench task. The episode gets a fresh, empty `HOME`
  on one of N Xvfb desktops (`scripts/desktop/start.sh` with
  `KAI_DISPLAY=:120+i`, each with its own D-Bus and AT-SPI). The steps are:
  the task's `setup`, then `kai run --yes --backend vllm --model <policy>
  --record <dir> "<prompt>"`, then the task's `check`, then `teardown`. Last,
  every window on that display is closed.
- **Sampling**: Kai always asks for temperature 0. If all 8 attempts are
  identical, the group has no variance and there is nothing to learn. Kai's
  Rust code is not changed for training. Instead, `KAI_ENDPOINT` points at
  `rollout_proxy.py` (`/ep/<episode>/v1`). The proxy sets temperature 1.0,
  top_p 0.95 and top_k 64 (the model card's sampling settings), asks for
  logprobs, and logs the exact prompt, images and sampled reply of every call.
  vLLM's strict `json_schema` output stays on, so replies remain valid JSON.
- **Reward** = `check` (1 or 0) − 0.01 × model calls − 0.02 × minutes +
  0.2 × (accepted-reply fraction − 1). Under vLLM's structured outputs the
  format term is nearly always 0. It matters for backends without
  constrained decoding. Episodes that fail for environment reasons (setup
  failed, no model call) are dropped from their group rather than scored 0.
- **Advantage** = (r − group mean) / group std, where a group is one task × N
  attempts. Groups with no spread are skipped. Every model call in an episode
  becomes a sample (prompt → sampled reply) with the episode's advantage,
  weighted 1/calls so that long failing episodes do not dominate.
- **Update: explicit, not TRL.** TRL 1.14's `GRPOTrainer` generates the
  completions itself, or through the experimental `rollout_func`. That hook
  must return token ids and logprobs for `num_generations` completions *per
  prompt*. Kai's groups are per task and multi-step: every step has a new
  prompt, made by an external process, and the number of steps varies. TRL
  also has no supported way to accept precomputed completions plus rewards.
  So `grpo_gemma.py` implements the update itself:
  - the PPO-clip objective against pre-update logprobs (with
    `inner_epochs: 1` the ratio is 1, so this is exactly REINFORCE with the
    group mean as baseline);
  - a constant normaliser (Kai's 192 max tokens, as in Dr. GRPO);
  - an optional k3 KL penalty against the base model (the adapter disabled,
    so no second copy of the model is loaded).

  vLLM's logprobs are logged but not used for importance sampling: tokens
  from vLLM and from transformers can differ slightly. Treat it as on-policy.
- **One GPU, two phases.** vLLM serving 26B-A4B plus a bf16 trainer of the
  same model do not both fit in 80 GB. vLLM runs with `--enable-sleep-mode`:
  during the update it sleeps at level 1 (weights to CPU, KV cache dropped),
  and the trainer moves to the GPU. During rollouts the trainer moves to CPU
  (the H100 Droplet has 240 GiB RAM per DigitalOcean's listing; check with
  `doctl compute size list`). After each update the adapter is saved to
  `iter-NNN`, loaded with `POST /v1/load_lora_adapter` as `kai-grpo-NNN`
  (this needs `VLLM_ALLOW_RUNTIME_LORA_UPDATING=True`), and the previous
  adapter is unloaded. `cloud-init-train.yaml` turns all of this on, bound to
  127.0.0.1 only.
- **Safety allowlist**: only task ids from `bench/tasks.yaml` that have a
  Linux `check`. Setup, check and teardown commands always come from that
  file; the GRPO JSONL supplies only ids. Held-out tasks are refused. Tasks
  that need the internet (real websites) are refused unless you pass
  `--allow-internet`. Each child process gets a scrubbed environment (no
  OpenAI, Gemini, AI Gateway or HF keys), an empty `HOME` and its own D-Bus
  session (so no keyring), with stdin closed so approvals cannot be granted.
  Do not add tasks that log into real accounts, send messages or spend money.

## Sizing on one H100 80 GB (US$3.39/h)

**Measured** data point (`bench/results/2026-10-10-gemini-gemma-4-26b-a4b-it.json`):
304 model calls over 30 tasks used 568,889 prompt and 26,276 completion tokens.
That is about **1,900 prompt + 90 reply tokens per call** and about 10 calls per
task. (These are Gemini-API token counts; vLLM counts each image as 280
tokens, so treat the numbers as a ballpark.)

**Data volume.** One full bench pass yields about 300 calls. With about 75 %
of tasks passing, roughly 150–250 of them are usable for SFT. A useful first
SFT set is 2,000–5,000 calls, i.e. 10–20 passes, mixing the base model's
successes with any teacher data you are allowed to use (see `distill.md`).
With only 24 training tasks, the model will memorise them. Add task variants
(other file names, values and apps) before expecting gains on the held-out
tasks.

The rows below are **estimates, not measurements**. They are compute ≈ 6 ×
active params × tokens, at an effective 30–40 % of the H100's bf16 peak, plus
model download and setup. Each epoch is 4,000 calls × 2,000 tokens ≈ 8M tokens.

| Model | Method | Frozen weights in GPU | Fits? | Per epoch (8M tokens) | Typical run | Cost |
|---|---|---|---|---|---|---|
| E4B (`google/gemma-4-E4B-it`) | bf16 LoRA | ~16 GB | easily | ~10–30 min | ~1 h | ~US$3–5 |
| 26B-A4B (`google/gemma-4-26B-A4B-it`), MoE 25.2B / 3.8B active | bf16 LoRA (r16 ≈ 18M trainable) | ~51 GB | yes (batch 1, grad checkpointing) | ~30–90 min | 2 epochs ≈ 2–4 h | ~US$7–14 |
| 26B-A4B | QLoRA (bnb 4-bit) | ~45 GB | yes, but saves little | similar | — | — |
| 31B (`google/gemma-4-31B-it`), dense | bf16 LoRA | ~62 GB | too tight with logits and activations | — | — | — |
| 31B | QLoRA NF4 | ~18–20 GB | yes | ~2–4 h | 2 epochs ≈ 5–9 h | ~US$17–30 |
| 26B-A4B | GRPO, 8 tasks × 8 attempts × 20 iterations | phases take turns | yes, with sleep and offload | — | ~25–40 min per iteration ≈ 8–13 h | ~US$27–45 |

Notes:

- On 26B-A4B the MoE experts are 3-D `nn.Parameter`s
  (`Gemma4TextExperts.gate_up_proj` / `down_proj` in transformers 5.17).
  bitsandbytes quantizes only `nn.Linear` layers, so QLoRA leaves most of the
  weights in bf16. For the same reason LoRA targets only attention and the
  dense MLP (`lora_target_regex`). The experts are not adapted.
- HF transformers' MoE kernels are slower per active parameter than dense
  ones. That is why the 26B-A4B range is wide. Measure on the first run and
  update this table.
- The GRPO estimate assumes 60–100 s per episode (the Gemini bench run
  averaged 105 s per task, including setup), 4 desktops, and about 5–10 min
  per update including sleep and wake. Download and cloud-init add about
  30 min per Droplet.
- **Destroy the Droplet when done** (`deploy/digitalocean/train-down.sh`). It
  bills while it exists, even when idle.

## Library versions assumed (PyPI, checked 2026-10-10; picked ≥ 2 weeks old)

`train/requirements-gpu.txt`: torch 2.13.0, torchvision 0.28.0, transformers
5.17.0, peft 0.21.1, accelerate 1.15.0, bitsandbytes 0.50.2. vLLM runs in its
container, `vllm/vllm-openai:v0.25.0`, the same image as serving. Its
`Gemma4ForConditionalGeneration` implements `SupportsLoRA`; I checked this in
the v0.25.0 and v0.30.0 sources. Unsloth (2026.10.x) has Gemma 4 recipes too,
but is not used, to keep the stack small. TRL 1.14.1 was evaluated and is not
needed (see the GRPO design above). Newer releases at the time of writing were
transformers 5.19, TRL 1.15, peft 0.21.2 and vLLM 0.31; they were not used.
The dry-run needs only `requirements-dryrun.txt` (pillow, pyyaml, jsonschema,
jinja2).

## Licensing (read before you ship an adapter; check with a lawyer)

Sources accessed 2026-10-10:

- **Gemma 4 weights are Apache 2.0.** The Hugging Face model cards for
  `google/gemma-4-26B-A4B-it`, `-31B-it` and `-E4B-it` list `license:
  apache-2.0` and link to
  [ai.google.dev/gemma/docs/gemma_4_license](https://ai.google.dev/gemma/docs/gemma_4_license).
  That page is the Apache License 2.0 text ("Last updated 2026-04-01 UTC").
  So the older [Gemma Terms of Use](https://ai.google.dev/gemma/terms) and
  its "Model Derivatives" pass-through clauses, which governed Gemma 1–3, do
  not appear to govern Gemma 4.
  - If you redistribute an adapter or merged weights, follow Apache 2.0 §4:
    include the license, keep the notices, and mark your changes.
  - The [Gemma Prohibited Use Policy](https://ai.google.dev/gemma/prohibited_use_policy)
    (last modified 2024) is still linked from Google's Gemma legal pages, but
    the Gemma 4 license page does not incorporate it. Whether it binds Gemma 4
    users is unclear: **check with a lawyer**, and follow it anyway as good
    practice.
- **Outputs of OpenAI models.** OpenAI's
  [Terms of Use](https://openai.com/policies/terms-of-use/) and
  [Services Agreement](https://openai.com/policies/services-agreement/)
  (API) prohibit using Output "to develop models that compete with OpenAI".
  openai.com returned HTTP 403 to the fetch tool on 2026-10-10. The wording
  here comes from search results and a third-party tracker that reports the
  clause as unchanged through 2026; **read the current text yourself**.
  Fine-tuning Gemma into a computer-use agent on GPT trajectories may
  plausibly count as developing a competing model. **Do not ship such an
  adapter without legal advice.**
- **Outputs obtained through the Gemini API.** The
  [Gemini API Additional Terms](https://ai.google.dev/gemini-api/terms)
  (last updated 2026-04-28) say: "You may not use the Services to develop
  models that compete with the Services (e.g., Gemini API or Google AI
  Studio)." This covers Gemma 4 served *through the Gemini API* (`kai bench
  --backend gemini`) as well. Whether training an open model on those runs
  is allowed is unclear: **check with a lawyer**. The clean path is to
  record Gemma's own runs on your own vLLM (`--backend vllm`), where only
  Apache 2.0 applies.
- **AI Gateway** (`--backend gateway`): the gateway's terms apply, *and*
  each upstream provider's terms.
- **Privacy.** Screenshots can contain private data. Train only on bench
  runs on the virtual desktops, or on recordings whose owners agreed. Never
  upload app recordings by default.
