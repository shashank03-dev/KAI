#!/usr/bin/env python3
"""LoRA SFT of Gemma 4 on Kai trajectories (`kai data export --format sft`).

Each JSONL row is one model call: system prompt + user turn (+0-2 JPEGs) ->
the raw JSON reply. The prompt is rendered with the model's own chat template
and `add_generation_prompt=True`, exactly as vLLM renders it at serving time
(that adds Gemma 4's empty thought channel `<|channel>thought\\n<channel|>`,
which a plain full-conversation render would NOT contain), then the reply and
the end-of-turn marker are appended. Loss is on those reply tokens only.

Rows are split by task (bench task_id, or a hash of the task text): tasks in
train/heldout.txt (and an optional hashed fraction) never reach training.

    # validate data + build examples, no model, no torch:
    python train/sft_gemma.py --config train/configs/sft_26b.yaml \
        --data bench/sft.jsonl --dry-run
    # train (GPU):
    python train/sft_gemma.py --config train/configs/sft_26b.yaml --data bench/sft.jsonl
"""

from __future__ import annotations

import argparse
import json
import random
import re
import sys
import time
from collections import Counter
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import kai_data as kd  # noqa: E402

DEFAULTS = {
    "model": "google/gemma-4-26B-A4B-it",
    "data": None,
    "output_dir": "train/out/sft",
    "heldout": str(kd.TRAIN_DIR / "heldout.txt"),
    "val_fraction": 0.0,
    "max_len": 4096,
    "eot": kd.END_OF_TURN,
    "canonical_reply": False,
    "compact_reply": False,          # drop null/empty fields (for teacher data)
    "include_model_regex": None,     # keep only rows whose `model` matches
    "exclude_model_regex": None,     # e.g. "^openai/" to keep teacher data out
    "image_prefix_map": None,        # "old_prefix=new_prefix" for absolute image paths
    # LoRA
    "lora_r": 16,
    "lora_alpha": 32,
    "lora_dropout": 0.05,
    # Language-model attention + the dense MLP. Gemma 4's MoE experts are 3-D
    # nn.Parameters (Gemma4TextExperts), not nn.Linear, so LoRA cannot and does
    # not target them; the vision/audio towers are left frozen.
    "lora_target_regex": r".*language_model.*\.(q_proj|k_proj|v_proj|o_proj|gate_proj|up_proj|down_proj)$",
    "load_in_4bit": False,
    "attn_implementation": "sdpa",
    # optimisation
    "epochs": 2,
    "max_steps": -1,
    "lr": 1.0e-4,
    "warmup_steps": 20,
    "per_device_batch": 1,
    "grad_accum": 16,
    "weight_decay": 0.0,
    "eval_steps": 100,
    "save_steps": 200,
    "logging_steps": 10,
    "seed": 17,
    "gradient_checkpointing": True,
}


def parse_args(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--config", help="YAML file with any of the defaults below")
    ap.add_argument("--dry-run", action="store_true",
                    help="validate JSONL + images and build chat examples; never loads the model")
    ap.add_argument("--chat-template", help="dry-run: render with this chat_template.jinja "
                    "and compare against the built-in renderer")
    ap.add_argument("--show", type=int, default=1, help="dry-run: examples to print")
    for k, v in DEFAULTS.items():
        flag = "--" + k.replace("_", "-")
        if isinstance(v, bool):
            ap.add_argument(flag, dest=k, action=argparse.BooleanOptionalAction, default=None)
        else:
            ap.add_argument(flag, dest=k, type=type(v) if v is not None else str, default=None)
    a = ap.parse_args(argv)
    cfg = kd.merge_config(DEFAULTS, a.config, vars(a))
    if not cfg["data"]:
        ap.error("--data (or `data:` in the config) is required")
    return a, cfg


def load_split(cfg, check_images=True):
    data = Path(cfg["data"])
    rows, line_errors = kd.read_jsonl(data)
    rep = kd.LoadReport()
    inc, exc = cfg["include_model_regex"], cfg["exclude_model_regex"]
    if inc or exc:
        kept_rows = []
        for r in rows:
            m = str(r.get("model") or "")
            if (inc and not re.search(inc, m)) or (exc and re.search(exc, m)):
                rep.reject("model_filter")
                rep.rows += 1
            else:
                kept_rows.append(r)
        rows = kept_rows
    for e in line_errors:
        rep.reject("jsonl", e)
    rep.rows += len(line_errors)
    exs = kd.build_examples(rows, data.parent, rep, check_images=check_images,
                            canonical_reply=cfg["canonical_reply"],
                            prefix_map=cfg["image_prefix_map"], compact=cfg["compact_reply"])
    too_long = [e for e in exs if e.est_tokens() > cfg["max_len"]]
    for e in too_long:
        rep.reject("too_long", f"{e.id}: ~{e.est_tokens()} tokens > max_len {cfg['max_len']}")
    exs = [e for e in exs if e.est_tokens() <= cfg["max_len"]]
    heldout = kd.load_heldout(cfg["heldout"])
    train, val = kd.split_by_task(
        [{"task_id": e.task_key, "_ex": e} for e in exs], heldout, cfg["val_fraction"])
    train = [r["_ex"] for r in train]
    val = [r["_ex"] for r in val]
    assert not ({e.task_key for e in train} & {e.task_key for e in val}), "task leaked across split"
    return rep, train, val, heldout


def dry_run(a, cfg):
    t0 = time.time()
    rep, train, val, heldout = load_split(cfg)
    print(f"data: {cfg['data']}")
    print(f"rows read: {rep.rows}   kept: {len(train) + len(val)}   rejected: {dict(rep.rejected)}")
    for e in rep.errors[:8]:
        print("  reject:", e)
    print(f"images by size (src[->resized]): {rep.image_sizes}")
    tk = Counter(e.task_key for e in train)
    vk = Counter(e.task_key for e in val)
    print(f"train: {len(train)} examples from {len(tk)} tasks {dict(tk)}")
    print(f"val (held out, never trained): {len(val)} examples from {len(vk)} tasks {dict(vk)}")
    missing = sorted(heldout - set(vk))
    if missing:
        print(f"note: {len(missing)} held-out task ids have no rows in this file: {missing}")
    print(f"rows by model: {dict(Counter(str(e.meta.get('model')) for e in train + val))}")
    imgs = Counter(len(e.image_paths) for e in train + val)
    toks = [e.est_tokens() for e in train + val]
    if toks:
        print(f"images per example: {dict(imgs)}   est tokens/example: min {min(toks)} "
              f"mean {sum(toks) // len(toks)} max {max(toks)}   est train tokens/epoch: "
              f"{sum(e.est_tokens() for e in train)}")
    # Load every image exactly as training would (RGB + Kai's downscale rule).
    for e in (train + val)[:50]:
        for p in e.image_paths:
            kd.load_image(p)
    for e in (train or val)[: a.show]:
        prompt = kd.render_prompt_fallback(e)
        print("\n--- example", e.id, "---")
        print(json.dumps(e.prompt_messages(), ensure_ascii=False)[:600], "...")
        print("rendered prompt tail:", repr(prompt[-160:]))
        print("completion (loss here only):", repr(e.reply + cfg["eot"]))
    if a.chat_template:
        bad = 0
        for e in train + val:
            if kd.render_prompt_jinja(e, a.chat_template) != kd.render_prompt_fallback(e):
                bad += 1
        print(f"\nchat template check vs {a.chat_template}: "
              f"{len(train) + len(val) - bad} match, {bad} differ")
        if bad:
            return 1
    print(f"\ndry run OK in {time.time() - t0:.1f}s (model not loaded)")
    return 0 if train else 1


# ------------------------------------------------------------- training ----

class KaiSFTDataset:
    def __init__(self, examples, processor, cfg):
        self.examples, self.processor, self.cfg = examples, processor, cfg

    def __len__(self):
        return len(self.examples)

    def __getitem__(self, i):
        return encode_example(self.processor, self.examples[i], self.cfg["eot"])


def encode_example(processor, ex, eot):
    """Tokenize one example; labels = -100 except the reply + end-of-turn."""
    import torch
    prompt = processor.apply_chat_template(ex.prompt_messages(), tokenize=False,
                                           add_generation_prompt=True)
    completion = ex.reply + eot
    images = [kd.load_image(p) for p in ex.image_paths]
    kw = {"images": [images]} if images else {}
    # The rendered prompt already starts with <bos>: add no special tokens.
    enc = processor(text=[prompt + completion], return_tensors="pt",
                    add_special_tokens=False, **kw)
    ids = enc["input_ids"][0]
    comp = processor.tokenizer(completion, add_special_tokens=False)["input_ids"]
    n = len(comp)
    if ids[-n:].tolist() != comp:
        # Tokens merged across the boundary: measure the prompt instead.
        p_enc = processor(text=[prompt], return_tensors="pt", add_special_tokens=False, **kw)
        n = ids.shape[0] - p_enc["input_ids"].shape[1]
    labels = torch.full_like(ids, -100)
    labels[-n:] = ids[-n:]
    enc["labels"] = labels.unsqueeze(0)
    return dict(enc)


def make_collator(pad_id):
    import torch

    def collate(items):
        seq = max(it["input_ids"].shape[1] for it in items)
        out = {}
        for k in items[0]:
            vals = [it[k] for it in items]
            if vals[0].dim() >= 2 and vals[0].shape[0] == 1 and \
                    all(v.shape[1] == it["input_ids"].shape[1] for v, it in zip(vals, items)):
                fill = pad_id if k == "input_ids" else (-100 if k == "labels" else 0)
                padded = [torch.nn.functional.pad(v, (0, seq - v.shape[1]), value=fill)
                          if v.dim() == 2 else
                          torch.cat([v, v.new_full((1, seq - v.shape[1], *v.shape[2:]), fill)], 1)
                          for v in vals]
                out[k] = torch.cat(padded, 0)
            else:  # image tensors: concatenate along the image axis
                out[k] = torch.cat(vals, 0)
        return out
    return collate


def train(cfg):
    import torch
    from peft import LoraConfig, get_peft_model, prepare_model_for_kbit_training
    from transformers import (AutoModelForImageTextToText, AutoProcessor,
                              BitsAndBytesConfig, Trainer, TrainingArguments)

    random.seed(cfg["seed"])
    rep, train_ex, val_ex, _ = load_split(cfg)
    print(f"kept {len(train_ex)} train / {len(val_ex)} val examples; rejected {rep.rejected}")
    if not train_ex:
        raise SystemExit("no training examples")

    processor = AutoProcessor.from_pretrained(cfg["model"])
    processor.tokenizer.padding_side = "right"
    quant = None
    if cfg["load_in_4bit"]:
        quant = BitsAndBytesConfig(load_in_4bit=True, bnb_4bit_quant_type="nf4",
                                   bnb_4bit_use_double_quant=True,
                                   bnb_4bit_compute_dtype=torch.bfloat16,
                                   llm_int8_skip_modules=["vision_tower", "embed_vision",
                                                          "audio_tower", "lm_head"])
    model = AutoModelForImageTextToText.from_pretrained(
        cfg["model"], dtype=torch.bfloat16, device_map={"": 0},
        attn_implementation=cfg["attn_implementation"], quantization_config=quant)
    model.config.use_cache = False
    if quant is not None:
        model = prepare_model_for_kbit_training(
            model, use_gradient_checkpointing=cfg["gradient_checkpointing"])
    elif cfg["gradient_checkpointing"]:
        model.enable_input_require_grads()
    model = get_peft_model(model, LoraConfig(
        r=cfg["lora_r"], lora_alpha=cfg["lora_alpha"], lora_dropout=cfg["lora_dropout"],
        target_modules=cfg["lora_target_regex"], bias="none", task_type="CAUSAL_LM"))
    model.print_trainable_parameters()

    args = TrainingArguments(
        output_dir=cfg["output_dir"], num_train_epochs=cfg["epochs"], max_steps=cfg["max_steps"],
        learning_rate=cfg["lr"], warmup_steps=cfg["warmup_steps"], lr_scheduler_type="cosine",
        per_device_train_batch_size=cfg["per_device_batch"], per_device_eval_batch_size=1,
        gradient_accumulation_steps=cfg["grad_accum"], weight_decay=cfg["weight_decay"],
        bf16=True, gradient_checkpointing=cfg["gradient_checkpointing"],
        gradient_checkpointing_kwargs={"use_reentrant": False},
        eval_strategy="steps" if val_ex else "no", eval_steps=cfg["eval_steps"],
        save_strategy="steps", save_steps=cfg["save_steps"], save_total_limit=3,
        logging_steps=cfg["logging_steps"], report_to="none", remove_unused_columns=False,
        dataloader_num_workers=2, seed=cfg["seed"], max_grad_norm=1.0)
    trainer = Trainer(
        model=model, args=args, data_collator=make_collator(processor.tokenizer.pad_token_id),
        train_dataset=KaiSFTDataset(train_ex, processor, cfg),
        eval_dataset=KaiSFTDataset(val_ex, processor, cfg) if val_ex else None)
    trainer.train()
    out = Path(cfg["output_dir"]) / "adapter"
    model.save_pretrained(out)          # adapter_config.json + adapter_model.safetensors
    processor.save_pretrained(out)
    (out / "kai_sft.json").write_text(json.dumps(
        {"config": cfg, "train_examples": len(train_ex), "val_examples": len(val_ex),
         "train_tasks": sorted({e.task_key for e in train_ex}),
         "val_tasks": sorted({e.task_key for e in val_ex})}, indent=1))
    print(f"saved PEFT adapter to {out}  (serve: deploy/digitalocean/serve-lora.sh {out})")


def main(argv=None):
    a, cfg = parse_args(argv)
    if a.dry_run:
        return dry_run(a, cfg)
    train(cfg)
    return 0


if __name__ == "__main__":
    sys.exit(main())
