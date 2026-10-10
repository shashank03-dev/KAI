#!/usr/bin/env python3
"""Online GRPO for Gemma 4 + Kai, with Kai itself as the environment.

Each iteration:
  1. pick `tasks_per_iter` allowed bench tasks (never the held-out ones);
  2. run each `group_size` times with the current LoRA policy served by vLLM,
     on `desktops` parallel Xvfb desktops (train/grpo_env.py), sampling at
     temperature > 0 through train/rollout_proxy.py;
  3. reward = check pass (1/0) - step/time penalties + format term; advantage
     = (r - group mean) / group std;
  4. every model call of an episode becomes a sample (prompt -> sampled reply)
     weighted by that episode's advantage; update the LoRA;
  5. save the adapter, hot-load it into vLLM (/v1/load_lora_adapter) under a
     new name, and use that name as the policy for the next iteration.

Which GRPO implementation (decided against TRL 1.14's docs):
  TRL's GRPOTrainer generates completions itself (or via `rollout_func`,
  experimental, which must return token ids + logprobs for `num_generations`
  completions *per prompt*). Kai's episodes are multi-step: each step is a new
  prompt (fresh screenshot) produced by an external Rust process, so the
  "group" is per task, not per prompt, and steps per episode vary. That does
  not fit GRPOTrainer's contract, and there is no supported way to feed it
  precomputed completions + rewards. So this file implements the GRPO update
  explicitly: group-relative advantages, then a clipped policy-gradient
  (PPO-style ratio against logprobs taken before the update; with
  inner_epochs=1 the ratio is 1 and this is exactly REINFORCE with the group
  mean as baseline), a Dr.GRPO-style constant length normaliser, and an
  optional k3 KL penalty to the frozen base (the adapter disabled).

    python train/grpo_gemma.py --config train/configs/grpo_26b.yaml --dry-run
    python train/grpo_gemma.py --config train/configs/grpo_26b.yaml   # GPU + desktops
"""

from __future__ import annotations

import argparse
import json
import os
import random
import sys
import time
import urllib.request
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import grpo_env as ge  # noqa: E402
import kai_data as kd  # noqa: E402

DEFAULTS = {
    "model": "google/gemma-4-26B-A4B-it",
    "init_adapter": None,            # SFT adapter dir; None = fresh LoRA
    "initial_policy": "kai",         # name vLLM serves the init adapter under
    "output_dir": "train/out/grpo",
    "tasks_jsonl": None,             # `kai data export --format grpo`; None = all allowed
    "tasks_yaml": str(kd.REPO / "bench" / "tasks.yaml"),
    "heldout": str(kd.TRAIN_DIR / "heldout.txt"),
    "allow_internet": False,
    "iterations": 20,
    "tasks_per_iter": 8,
    "group_size": 8,
    "desktops": 4,
    "base_display": 120,
    "kai_bin": "kai",
    "vllm_url": "http://127.0.0.1:8000",
    "vllm_path_map": None,           # "host_prefix=container_prefix" for docker vLLM
    "vllm_sleep": False,             # sleep vLLM (level 1) during updates to free the GPU
    "offload_between_phases": False, # move the trainer's model to CPU during rollouts
    "proxy_port": 8100,
    "temperature": 1.0,
    "top_p": 0.95,
    "top_k": 64,
    "lr": 1.0e-5,
    "inner_epochs": 1,
    "updates_per_iter": 4,
    "clip_eps": 0.2,
    "kl_beta": 0.02,
    "scale_std": True,
    "per_episode_norm": True,
    "len_norm": 192,                 # Kai's max_tokens per reply (ports.rs)
    "step_penalty": 0.01,
    "time_penalty_per_min": 0.02,
    "format_weight": 0.2,
    "episode_timeout_s": 660.0,
    "lora_r": 16,
    "lora_alpha": 32,
    "lora_target_regex": r".*language_model.*\.(q_proj|k_proj|v_proj|o_proj|gate_proj|up_proj|down_proj)$",
    "eot": kd.END_OF_TURN,
    "seed": 17,
}


def parse_args(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--config")
    ap.add_argument("--dry-run", action="store_true",
                    help="fake runner, no desktops/kai/vLLM/model; exercises everything else")
    for k, v in DEFAULTS.items():
        flag = "--" + k.replace("_", "-")
        if isinstance(v, bool):
            ap.add_argument(flag, dest=k, action=argparse.BooleanOptionalAction, default=None)
        else:
            ap.add_argument(flag, dest=k, type=type(v) if v is not None else str, default=None)
    a = ap.parse_args(argv)
    return a, kd.merge_config(DEFAULTS, a.config, vars(a))


# ------------------------------------------------------------------ vLLM ----

class VllmAdmin:
    """LoRA hot-swap + sleep mode on the local vLLM server.

    Needs: --enable-lora, VLLM_ALLOW_RUNTIME_LORA_UPDATING=True (load/unload),
    and for sleep: --enable-sleep-mode with VLLM_SERVER_DEV_MODE=1. Keep this
    server on localhost: these endpoints must never be public.
    """

    def __init__(self, base: str, path_map: str | None):
        self.base = base.rstrip("/")
        self.key = os.environ.get("VLLM_API_KEY", "")
        self.map = tuple(path_map.split("=", 1)) if path_map else None

    def _post(self, path: str, body: dict | None = None):
        req = urllib.request.Request(self.base + path, method="POST",
                                     data=json.dumps(body or {}).encode(),
                                     headers={"Content-Type": "application/json",
                                              "Authorization": f"Bearer {self.key}"})
        with urllib.request.urlopen(req, timeout=600) as r:
            return r.read()

    def load(self, name: str, path: Path):
        p = str(Path(path).resolve())
        if self.map and p.startswith(self.map[0]):
            p = self.map[1] + p[len(self.map[0]):]
        self._post("/v1/load_lora_adapter", {"lora_name": name, "lora_path": p})

    def unload(self, name: str):
        self._post("/v1/unload_lora_adapter", {"lora_name": name})

    def sleep(self):
        self._post("/sleep?level=1")

    def wake(self):
        self._post("/wake_up")


# --------------------------------------------------------------- updater ----

class PolicyUpdater:
    """Holds the trainable LoRA policy and applies one GRPO update."""

    def __init__(self, cfg):
        import torch
        from peft import LoraConfig, PeftModel, get_peft_model
        from transformers import AutoModelForImageTextToText, AutoProcessor
        self.torch, self.cfg = torch, cfg
        self.processor = AutoProcessor.from_pretrained(cfg["model"])
        base = AutoModelForImageTextToText.from_pretrained(
            cfg["model"], dtype=torch.bfloat16, device_map={"": 0}, attn_implementation="sdpa")
        base.config.use_cache = False
        base.enable_input_require_grads()
        if cfg["init_adapter"]:
            self.model = PeftModel.from_pretrained(base, cfg["init_adapter"], is_trainable=True)
        else:
            self.model = get_peft_model(base, LoraConfig(
                r=cfg["lora_r"], lora_alpha=cfg["lora_alpha"], lora_dropout=0.0,
                target_modules=cfg["lora_target_regex"], task_type="CAUSAL_LM"))
        self.model.gradient_checkpointing_enable(gradient_checkpointing_kwargs={"use_reentrant": False})
        params = [p for p in self.model.parameters() if p.requires_grad]
        self.opt = torch.optim.AdamW(params, lr=cfg["lr"], weight_decay=0.0)
        self.params = params

    def _token_logps(self, sample, grad: bool, ref: bool = False):
        from sft_gemma import encode_example
        torch = self.torch
        ex = kd.Example(id=sample["episode"], task_key=sample["task_id"], system=sample["system"],
                        user=sample["user"], image_paths=[Path(p) for p in sample["images"]],
                        reply=sample["reply"])
        enc = encode_example(self.processor, ex, self.cfg["eot"])
        labels = enc.pop("labels")[0]
        n = int((labels != -100).sum())
        inputs = {k: v.to(self.model.device) for k, v in enc.items()}
        ctx = torch.enable_grad() if grad else torch.no_grad()
        with ctx:
            if ref:
                with self.model.disable_adapter():
                    out = self.model(**inputs, logits_to_keep=n + 1)
            else:
                out = self.model(**inputs, logits_to_keep=n + 1)
            logits = out.logits[0, -(n + 1):-1].float()       # predicts the last n tokens
            tgt = labels[-n:].to(logits.device)
            return torch.log_softmax(logits, -1).gather(-1, tgt[:, None])[:, 0]

    def step(self, samples: list[dict]) -> dict:
        torch, cfg = self.torch, self.cfg
        self.model.train()
        old = None
        if cfg["inner_epochs"] > 1:
            old = [self._token_logps(s, grad=False) for s in samples]
        stats = {"loss": 0.0, "kl": 0.0, "clip_frac": 0.0, "tokens": 0, "optimizer_steps": 0}
        order = list(range(len(samples)))
        for _ in range(cfg["inner_epochs"]):
            random.shuffle(order)
            k = max(1, cfg["updates_per_iter"])
            for mb in [order[i::k] for i in range(k)]:
                if not mb:
                    continue
                wsum = sum(samples[i]["weight"] for i in mb)
                self.opt.zero_grad(set_to_none=True)
                for i in mb:
                    s = samples[i]
                    logp = self._token_logps(s, grad=True)
                    base = old[i].to(logp.device) if old is not None else logp.detach()
                    ratio = torch.exp(logp - base)
                    a = float(s["advantage"])
                    clipped = torch.clamp(ratio, 1 - cfg["clip_eps"], 1 + cfg["clip_eps"])
                    per_tok = -torch.minimum(ratio * a, clipped * a)
                    if cfg["kl_beta"] > 0:
                        ref = self._token_logps(s, grad=False, ref=True)
                        d = ref - logp
                        kl = torch.exp(d) - d - 1          # k3 estimator, >= 0
                        per_tok = per_tok + cfg["kl_beta"] * kl
                        stats["kl"] += float(kl.mean())
                    loss = per_tok.sum() / cfg["len_norm"] * (s["weight"] / wsum)
                    loss.backward()
                    stats["loss"] += float(loss)
                    stats["tokens"] += int(logp.numel())
                    stats["clip_frac"] += float(((ratio - 1).abs() > cfg["clip_eps"]).float().mean())
                torch.nn.utils.clip_grad_norm_(self.params, 1.0)
                self.opt.step()
                stats["optimizer_steps"] += 1
        n = max(1, len(samples) * cfg["inner_epochs"])
        stats["kl"] /= n
        stats["clip_frac"] /= n
        return stats

    def save(self, path: Path):
        self.model.save_pretrained(path)
        self.processor.save_pretrained(path)

    def to(self, device: str):
        self.model.to(device)
        if device == "cpu":
            self.torch.cuda.empty_cache()


# ------------------------------------------------------------------ loop ----

def run(cfg, dry: bool) -> int:
    rng = random.Random(cfg["seed"])
    random.seed(cfg["seed"])
    out = Path(cfg["output_dir"])
    out.mkdir(parents=True, exist_ok=True)
    bench = kd.load_bench_tasks(cfg["tasks_yaml"])
    heldout = kd.load_heldout(cfg["heldout"])
    allowed, refused = ge.allowed_tasks(bench, heldout, cfg["allow_internet"])
    tasks, notes = ge.load_grpo_tasks(cfg["tasks_jsonl"], allowed)
    print(f"allowed bench tasks: {len(allowed)}; refused: {len(refused)}")
    for tid, why in sorted(refused.items()):
        print(f"  refused {tid}: {why}")
    for n in notes:
        print(f"  note: {n}")
    if not tasks:
        print("no allowed tasks to train on")
        return 1
    print(f"training tasks this run: {[t.id for t in tasks]}")
    rc = ge.RewardConfig(cfg["step_penalty"], cfg["time_penalty_per_min"], cfg["format_weight"])

    proxy = admin = updater = None
    if dry:
        runner = ge.FakeRunner(out / "rollouts", seed=cfg["seed"])
        desktops = ge.make_desktops(cfg["desktops"], cfg["base_display"], out / "desktops", start=False)
    else:
        from rollout_proxy import ProxyConfig, RolloutProxy
        proxy = RolloutProxy(ProxyConfig(cfg["vllm_url"].rstrip("/") + "/v1", out / "proxy-logs",
                                         cfg["temperature"], cfg["top_p"], cfg["top_k"]),
                             cfg["proxy_port"]).__enter__()
        runner = ge.KaiRunner(ge.RunnerConfig(kai_bin=cfg["kai_bin"], work_dir=out / "rollouts",
                                              episode_timeout_s=cfg["episode_timeout_s"]), proxy)
        desktops = ge.make_desktops(cfg["desktops"], cfg["base_display"], out / "desktops", start=True)
        admin = VllmAdmin(cfg["vllm_url"], cfg["vllm_path_map"])
        if cfg["init_adapter"]:
            try:  # no-op error if the server already serves it via --lora-modules
                admin.load(cfg["initial_policy"], Path(cfg["init_adapter"]))
            except Exception as e:  # noqa: BLE001
                print(f"note: loading {cfg['initial_policy']} into vLLM: {e}")
        updater = PolicyUpdater(cfg)
        if cfg["offload_between_phases"]:
            updater.to("cpu")
    pool = ge.RolloutPool(runner, desktops)
    policy = cfg["initial_policy"]
    try:
        for it in range(cfg["iterations"]):
            t0 = time.time()
            batch = rng.sample(tasks, min(cfg["tasks_per_iter"], len(tasks)))
            groups = pool.run_groups(batch, cfg["group_size"], policy, tag=f"it{it:03d}")
            samples, with_signal, eps_all = [], 0, []
            for tid, eps in groups.items():
                for e in eps:
                    e.reward = ge.score(e, rc)
                with_signal += ge.group_advantages(eps, cfg["scale_std"])
                for e in eps:
                    samples += ge.episode_samples(e, cfg["per_episode_norm"])
                eps_all += eps
            ok = [e for e in eps_all if e.env_error is None]
            row = {"iter": it, "policy": policy, "episodes": len(eps_all), "env_errors": len(eps_all) - len(ok),
                   "success_rate": round(sum(bool(e.check) for e in ok) / max(1, len(ok)), 4),
                   "mean_reward": round(sum(e.reward for e in ok) / max(1, len(ok)), 4),
                   "mean_calls": round(sum(e.model_calls for e in ok) / max(1, len(ok)), 2),
                   "valid_reply_frac": round(sum(e.valid_replies for e in ok) / max(1, sum(e.model_calls for e in ok)), 4),
                   "groups": len(groups), "groups_with_signal": with_signal, "samples": len(samples)}
            if dry:
                for tid, eps in groups.items():
                    print(f"  it{it} {tid:<16} rewards " +
                          " ".join(f"{e.reward:+.2f}" for e in eps) + "  adv " +
                          " ".join(f"{e.advantage:+.2f}" for e in eps))
                for s in samples[:2]:      # the samples must build real prompts
                    ex = kd.Example(s["episode"], s["task_id"], s["system"], s["user"],
                                    [Path(p) for p in s["images"]], s["reply"])
                    for p in ex.image_paths:
                        kd.load_image(p)
                    assert kd.render_prompt_fallback(ex).endswith(kd.GEN_PROMPT_SUFFIX)
                row["update"] = "skipped (dry run)"
            elif samples:
                if cfg["vllm_sleep"]:
                    admin.sleep()
                if cfg["offload_between_phases"]:
                    updater.to("cuda")
                row["update"] = updater.step(samples)
                path = out / f"iter-{it:03d}"
                updater.save(path)
                if cfg["offload_between_phases"]:
                    updater.to("cpu")
                if cfg["vllm_sleep"]:
                    admin.wake()
                new = f"kai-grpo-{it:03d}"
                admin.load(new, path)
                if policy.startswith("kai-grpo-"):
                    admin.unload(policy)
                policy = new
            else:
                row["update"] = "skipped (no group had a reward spread)"
            row["seconds"] = round(time.time() - t0, 1)
            print(json.dumps(row))
            with open(out / "metrics.jsonl", "a", encoding="utf-8") as f:
                f.write(json.dumps(row) + "\n")
    finally:
        if proxy is not None:
            proxy.__exit__(None, None, None)
    print(f"done; last policy: {policy}; metrics in {out / 'metrics.jsonl'}")
    return 0


def main(argv=None):
    a, cfg = parse_args(argv)
    return run(cfg, a.dry_run)


if __name__ == "__main__":
    sys.exit(main())
