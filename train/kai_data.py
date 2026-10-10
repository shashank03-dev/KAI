"""Shared, torch-free helpers for Kai's training scripts.

Everything here follows the dataset contract `kai-trajectory/1` (see
train/README.md): JSONL rows written by `kai data export`, recorded run
directories (`trace.json`, `system.txt`, `meta.json`, `frames/`) and the bench
task file `bench/tasks.yaml`. Nothing in this module imports torch, so the
`--dry-run` modes of every script work on a laptop.
"""

from __future__ import annotations

import hashlib
import json
import os
import re
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Iterable

REPO = Path(__file__).resolve().parent.parent
TRAIN_DIR = Path(__file__).resolve().parent

# Kai's frame budget (crates/kai-os/src/frame.rs): Gemma 4's default 280 soft
# tokens (~48x48 px each) and never more than 1280 px on the long side.
MODEL_LONG_SIDE = 1280
MODEL_MAX_PIXELS = 280 * 48 * 48
IMAGE_TOKENS = 280  # soft tokens per image at the default budget
MAX_IMAGES = 2      # screenshot + optional zoom crop

# Gemma 4 chat template markers (google/gemma-4-*-it chat_template.jinja,
# "Published: 2026-07-09"). Used only by the dry-run renderer; real training
# always goes through processor.apply_chat_template.
BOS = "<bos>"
END_OF_TURN = "<turn|>"
GEN_PROMPT_SUFFIX = "<|turn>model\n<|channel>thought\n<channel|>"

ACTIONS = {"click", "double_click", "right_click", "type", "key", "scroll",
           "open_app", "open_url", "wait", "ask_user", "done", "shell"}
TARGET_ACTIONS = {"click", "double_click", "right_click", "type"}
DECISION_KEYS = {"thought", "action", "target", "text", "keys", "direction",
                 "amount", "risk", "expect"}
REPLY_EXTRA_KEYS = {"then", "need_screen"}

# One exported row (`kai data export --format sft`). Optional fields may be
# null; unknown fields are allowed so the Rust side can add more later.
SFT_ROW_SCHEMA: dict[str, Any] = {
    "type": "object",
    "required": ["id", "task", "step", "system", "user", "images", "reply"],
    "properties": {
        "id": {"type": "string", "minLength": 1},
        "task": {"type": "string"},
        "task_id": {"type": ["string", "null"]},
        "step": {"type": "integer", "minimum": 0},
        "system": {"type": "string", "minLength": 1},
        "user": {"type": "string", "minLength": 1},
        "images": {"type": "array", "maxItems": MAX_IMAGES, "items": {"type": "string"}},
        "reply": {"type": "string", "minLength": 1},
        "decision": {"type": ["object", "null"]},
        "run_outcome": {"type": ["string", "null"]},
        "check": {"type": ["boolean", "null"]},
        "step_verified": {"type": ["boolean", "null"]},
        "backend": {"type": ["string", "null"]},
        "model": {"type": ["string", "null"]},
        "latency_ms": {"type": ["integer", "number", "null"]},
        "source": {"type": ["string", "null"]},
    },
}


# --------------------------------------------------------------- JSONL ----

def read_jsonl(path: str | os.PathLike) -> tuple[list[dict], list[str]]:
    """Rows and per-line errors (bad JSON, non-object)."""
    rows, errors = [], []
    with open(path, encoding="utf-8") as f:
        for n, line in enumerate(f, 1):
            line = line.strip()
            if not line:
                continue
            try:
                v = json.loads(line)
            except json.JSONDecodeError as e:
                errors.append(f"line {n}: bad JSON ({e.msg})")
                continue
            if not isinstance(v, dict):
                errors.append(f"line {n}: not an object")
                continue
            v["_line"] = n
            rows.append(v)
    return rows, errors


def _validator():
    try:
        import jsonschema  # light; in requirements-dryrun.txt
    except ImportError:
        return None
    return jsonschema.Draft202012Validator(SFT_ROW_SCHEMA)


# ------------------------------------------------------------- replies ----

def extract_json_object(s: str) -> str | None:
    """Port of kai_core::schema::extract_json_object: the first balanced {...}."""
    start = s.find("{")
    if start < 0:
        return None
    depth, in_str, esc = 0, False, False
    for i in range(start, len(s)):
        c = s[i]
        if in_str:
            if esc:
                esc = False
            elif c == "\\":
                esc = True
            elif c == '"':
                in_str = False
            continue
        if c == '"':
            in_str = True
        elif c == "{":
            depth += 1
        elif c == "}":
            depth -= 1
            if depth == 0:
                return s[start:i + 1]
    return None


def check_reply(raw: str) -> tuple[bool, str]:
    """Mirror of StepReply::parse_checked for the main decision.

    Returns (ok, reason). `ok` means Kai would accept the reply without a
    repair retry. Follow-ups in `then` are not required to be valid (Kai drops
    bad ones), matching the Rust behaviour.
    """
    js = extract_json_object(raw)
    if js is None:
        return False, "no_json_object"
    try:
        obj = json.loads(js)
    except json.JSONDecodeError:
        return False, "bad_json"
    if not isinstance(obj, dict):
        return False, "not_object"
    unknown = set(obj) - DECISION_KEYS - REPLY_EXTRA_KEYS
    if unknown:
        return False, "unknown_field:" + ",".join(sorted(unknown))
    if not isinstance(obj.get("thought"), str):
        return False, "missing_thought"
    action = obj.get("action")
    if action not in ACTIONS:
        return False, "bad_action"
    risk = obj.get("risk", "none")
    if risk not in ("none", "low", "high"):
        return False, "bad_risk"
    text = obj.get("text") or ""
    if not isinstance(text, str):
        return False, "bad_text"
    keys = obj.get("keys") or []
    if not isinstance(keys, list) or not all(isinstance(k, str) for k in keys):
        return False, "bad_keys"
    target = obj.get("target")
    if target is not None and (not isinstance(target, int) or isinstance(target, bool) or target < 0):
        return False, "bad_target"
    if action in TARGET_ACTIONS and target is None:
        return False, "missing_target"
    if action in ("type", "open_url") and not text:
        return False, "missing_text"
    if action in ("shell", "open_app") and not text.strip():
        return False, "missing_text"
    if action == "key" and not [k for k in keys if k.strip()]:
        return False, "missing_keys"
    return True, "ok"


def compact_reply(raw: str) -> str:
    """Re-serialize a valid reply without null/empty optional fields.

    OpenAI strict structured outputs emit every property (nulls included);
    Gemma under vLLM emits only what it needs. Compacting teacher replies
    keeps the student from learning to spend tokens on nulls."""
    obj = json.loads(extract_json_object(raw) or "{}")

    def clean(d: dict) -> dict:
        keep = {"thought", "action", "risk"}
        return {k: v for k, v in d.items()
                if k in keep or not (v is None or v is False or v == [] or v == "")}

    out = clean(obj)
    if isinstance(out.get("then"), list):
        out["then"] = [clean(x) for x in out["then"] if isinstance(x, dict)]
        if not out["then"]:
            del out["then"]
    return json.dumps(out, ensure_ascii=False, separators=(",", ":"))


def is_strict_json_reply(raw: str) -> bool:
    """True when the whole reply is exactly one JSON object (no prose/fences)."""
    js = extract_json_object(raw)
    return js is not None and raw.strip() == js


# -------------------------------------------------------------- images ----

def kai_target_size(w: int, h: int, max_side: int = MODEL_LONG_SIDE,
                    max_pixels: int = MODEL_MAX_PIXELS) -> tuple[int, int]:
    """Port of FrameConfig::target_size (downscale only, keep aspect)."""
    w, h = max(w, 1), max(h, 1)
    scale = max(w, h) / max(max_side, 1)
    if max_pixels > 0:
        scale = max(scale, (w * h / max_pixels) ** 0.5)
    if scale <= 1.0:
        return w, h
    return max(int(w / scale), 1), max(int(h / scale), 1)


def load_image(path: str | os.PathLike):
    """Open as RGB and downscale exactly like Kai would before sending.

    Kai's recorded frames are already at this size, so this is a no-op for
    them; it only matters for frames from other sources (bigger captures).
    The Gemma 4 processor then snaps to multiples of 48 itself, the same way
    vLLM does at serving time.
    """
    from PIL import Image
    img = Image.open(path)
    img.load()
    img = img.convert("RGB")
    tw, th = kai_target_size(*img.size)
    if (tw, th) != img.size:
        img = img.resize((tw, th), Image.Resampling.LANCZOS)
    return img


def resolve_image(p: str, base: Path, prefix_map: str | None = None) -> Path:
    """Relative paths resolve against the JSONL's folder. `prefix_map`
    ("old=new") rewrites absolute paths exported on another machine."""
    if prefix_map and "=" in prefix_map:
        old, new = prefix_map.split("=", 1)
        if p.startswith(old):
            p = new + p[len(old):]
    q = Path(p)
    return q if q.is_absolute() else (base / q)


# ------------------------------------------------------- task grouping ----

def task_key(row: dict) -> str:
    """Split key: the bench id, or a hash of the task text for non-bench runs."""
    tid = row.get("task_id")
    if tid:
        return str(tid)
    return "task:" + hashlib.sha1(str(row.get("task", "")).encode()).hexdigest()[:12]


def load_heldout(path: str | os.PathLike | None) -> set[str]:
    if not path:
        return set()
    out = set()
    for line in Path(path).read_text(encoding="utf-8").splitlines():
        line = line.split("#", 1)[0].strip()
        if line:
            out.add(line)
    return out


def split_by_task(rows: list[dict], heldout: set[str], val_fraction: float = 0.0,
                  seed: str = "kai") -> tuple[list[dict], list[dict]]:
    """Held-out tasks (listed, or hashed into the val fraction) never train."""
    train, val = [], []
    for r in rows:
        k = task_key(r)
        h = int(hashlib.sha1(f"{seed}:{k}".encode()).hexdigest()[:8], 16) / 0xFFFFFFFF
        (val if (k in heldout or h < val_fraction) else train).append(r)
    return train, val


# ----------------------------------------------------- example building ----

@dataclass
class Example:
    id: str
    task_key: str
    system: str
    user: str
    image_paths: list[Path]
    reply: str
    meta: dict = field(default_factory=dict)

    def prompt_messages(self) -> list[dict]:
        """Exactly what Kai's vLLM client sends: system, then user = text
        first, then the screenshot, then the zoom crop (client/vllm.rs)."""
        content: list[dict] = [{"type": "text", "text": self.user}]
        content += [{"type": "image"} for _ in self.image_paths]
        return [{"role": "system", "content": self.system},
                {"role": "user", "content": content}]

    def est_tokens(self) -> int:
        """Rough budget: ~3.5 chars/token for text + 280 per image."""
        chars = len(self.system) + len(self.user) + len(self.reply)
        return int(chars / 3.5) + IMAGE_TOKENS * len(self.image_paths) + 16


def render_prompt_fallback(ex: Example) -> str:
    """Torch-free rendering of the Gemma 4 template for system + one user turn
    with add_generation_prompt=True (enable_thinking=False). Verified against
    the published chat_template.jinja by `sft_gemma.py --dry-run
    --chat-template <file>`; real training never uses this."""
    user = ex.user.strip() + "<|image|>" * len(ex.image_paths)
    return (f"{BOS}<|turn>system\n{ex.system.strip()}{END_OF_TURN}\n"
            f"<|turn>user\n{user}{END_OF_TURN}\n{GEN_PROMPT_SUFFIX}")


def render_prompt_jinja(ex: Example, template_path: str | os.PathLike) -> str:
    """Render with the real chat template via jinja2 (as transformers does)."""
    from jinja2.sandbox import ImmutableSandboxedEnvironment

    def _raise(msg):
        raise ValueError(msg)

    env = ImmutableSandboxedEnvironment(trim_blocks=True, lstrip_blocks=True,
                                        extensions=["jinja2.ext.loopcontrols"])
    env.globals["raise_exception"] = _raise
    tpl = env.from_string(Path(template_path).read_text(encoding="utf-8"))
    return tpl.render(messages=ex.prompt_messages(), add_generation_prompt=True,
                      bos_token=BOS, eos_token="<eos>")


@dataclass
class LoadReport:
    rows: int = 0
    kept: int = 0
    rejected: dict = field(default_factory=dict)
    image_sizes: dict = field(default_factory=dict)
    errors: list = field(default_factory=list)

    def reject(self, reason: str, detail: str = ""):
        self.rejected[reason] = self.rejected.get(reason, 0) + 1
        if detail and len(self.errors) < 20:
            self.errors.append(detail)


def build_examples(rows: Iterable[dict], base_dir: Path, report: LoadReport,
                   check_images: bool = True, require_valid_reply: bool = True,
                   canonical_reply: bool = False, prefix_map: str | None = None,
                   compact: bool = False) -> list[Example]:
    """Validate rows against the contract and turn them into Examples."""
    validator = _validator()
    out = []
    for r in rows:
        report.rows += 1
        where = f"line {r.get('_line')}"
        if validator is not None:
            errs = sorted(validator.iter_errors(r), key=lambda e: list(e.path))
            if errs:
                report.reject("schema", f"{where}: {errs[0].message}")
                continue
        reply = r["reply"]
        ok, why = check_reply(reply)
        if require_valid_reply and not ok:
            report.reject("reply_" + why.split(":")[0], f"{where}: reply rejected ({why})")
            continue
        if compact:
            reply = compact_reply(reply)
        elif canonical_reply:
            reply = extract_json_object(reply) or reply
        paths = [resolve_image(p, base_dir, prefix_map) for p in r.get("images") or []]
        bad = False
        if check_images:
            from PIL import Image
            for p in paths:
                try:
                    with Image.open(p) as im:
                        im.verify()
                    with Image.open(p) as im:
                        size = im.size
                    tgt = kai_target_size(*size)
                    key = f"{size[0]}x{size[1]}" + ("" if tgt == size else f"->{tgt[0]}x{tgt[1]}")
                    report.image_sizes[key] = report.image_sizes.get(key, 0) + 1
                except Exception as e:  # noqa: BLE001 - report any unreadable file
                    report.reject("image", f"{where}: {p}: {e}")
                    bad = True
                    break
        if bad:
            continue
        out.append(Example(
            id=r["id"], task_key=task_key(r), system=r["system"], user=r["user"],
            image_paths=paths, reply=reply,
            meta={k: r.get(k) for k in ("task", "task_id", "step", "model", "backend",
                                        "check", "run_outcome", "source")}))
        report.kept += 1
    return out


# --------------------------------------------------------- bench tasks ----

@dataclass
class BenchTask:
    id: str
    prompt: str
    kind: str
    needs: list[str]
    setup: str | None
    check: str | None
    teardown: str | None


def _linux(v: Any) -> str | None:
    if isinstance(v, dict):
        return v.get("linux")
    return v if isinstance(v, str) else None


def load_bench_tasks(path: str | os.PathLike = REPO / "bench" / "tasks.yaml") -> dict[str, BenchTask]:
    import yaml
    items = yaml.safe_load(Path(path).read_text(encoding="utf-8")) or []
    out = {}
    for t in items:
        out[t["id"]] = BenchTask(
            id=t["id"], prompt=t["prompt"], kind=t.get("kind", ""),
            needs=list(t.get("needs") or []), setup=_linux(t.get("setup")),
            check=_linux(t.get("check")), teardown=_linux(t.get("teardown")))
    return out


# ----------------------------------------------------------- recordings ----

def read_recording(run_dir: str | os.PathLike) -> dict:
    """One recorded run dir -> {meta, system, steps:[{user, reply, images,...}]}."""
    d = Path(run_dir)
    trace = json.loads((d / "trace.json").read_text(encoding="utf-8"))
    meta_p = d / "meta.json"
    meta = json.loads(meta_p.read_text(encoding="utf-8")) if meta_p.exists() else {}
    sys_p = d / "system.txt"
    system = sys_p.read_text(encoding="utf-8") if sys_p.exists() else ""
    steps = []
    for s in trace.get("steps", []):
        if not s.get("reply_raw"):
            continue  # follow-up (`then`) steps made no model call
        imgs = [str(d / p) for p in (s.get("screenshot"), s.get("crop")) if p]
        steps.append({"index": s.get("index"), "user": s.get("prompt_user") or "",
                      "reply": s["reply_raw"], "images": imgs,
                      "verified": s.get("verified"), "model": s.get("model")})
    return {"meta": meta, "system": system, "steps": steps,
            "outcome": trace.get("outcome"), "n_steps": len(trace.get("steps", [])),
            "total_ms": trace.get("total_ms", 0)}


def find_run_dirs(root: str | os.PathLike) -> list[Path]:
    """`root` may itself be a run dir or contain run dirs."""
    root = Path(root)
    if (root / "trace.json").exists():
        return [root]
    return sorted(p.parent for p in root.glob("*/trace.json"))


SAFE_NAME = re.compile(r"[^A-Za-z0-9._-]+")


def safe_name(s: str) -> str:
    return SAFE_NAME.sub("-", s)[:80].strip("-") or "x"


# --------------------------------------------------------------- config ----

def merge_config(defaults: dict, yaml_path: str | None, overrides: dict) -> dict:
    """defaults < YAML file < explicit CLI flags (None means "not given")."""
    cfg = dict(defaults)
    if yaml_path:
        import yaml
        loaded = yaml.safe_load(Path(yaml_path).read_text(encoding="utf-8")) or {}
        unknown = set(loaded) - set(defaults)
        if unknown:
            raise SystemExit(f"{yaml_path}: unknown keys {sorted(unknown)}")
        cfg.update(loaded)
    cfg.update({k: v for k, v in overrides.items() if v is not None and k in defaults})
    return cfg
