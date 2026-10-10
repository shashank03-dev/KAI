#!/usr/bin/env python3
"""Evaluate a model on the held-out bench tasks and log a row to train/results.md.

Runs the real bench (`kai bench --yes --backend <b> --model <m> --only <id>...`)
restricted to train/heldout.txt, reads the results JSON that `kai bench`
writes, and prints success rate, steps per task and seconds per step next to
the same held-out subset of every existing file in bench/results/. Numbers
only ever come from results files; a task missing from a file shows "—".

    # serve the adapter first (deploy/digitalocean/serve-lora.sh), then:
    python train/eval.py --model kai --label sft-26b-v1
    # tabulate an existing results file without running anything:
    python train/eval.py --from-json bench/results/2026-10-10-gemini-gemma-4-26b-a4b-it.json --dry-run
"""

from __future__ import annotations

import argparse
import datetime as dt
import json
import shlex
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import kai_data as kd  # noqa: E402

HEADER = ("| Date | Label | Backend | Model | Held-out tasks run | Passed | Success | "
          "Steps/task | s/step | Results file |\n|---|---|---|---|---|---|---|---|---|---|\n")


def metrics(results: dict, ids: list[str]) -> dict | None:
    rows = [t for t in results.get("tasks", []) if t.get("id") in ids]
    if not rows:
        return None
    steps = sum(t.get("steps") or 0 for t in rows)
    secs = sum(t.get("seconds") or 0.0 for t in rows)
    passed = sum(1 for t in rows if t.get("pass"))
    return {"n": len(rows), "passed": passed, "success": passed / len(rows),
            "steps_per_task": steps / len(rows), "s_per_step": (secs / steps) if steps else None,
            "missing": sorted(set(ids) - {t.get("id") for t in rows})}


def fmt(m: dict | None, key: str, pct=False, nd=2) -> str:
    if not m or m.get(key) is None:
        return "—"
    v = m[key]
    return f"{100 * v:.0f}%" if pct else f"{v:.{nd}f}"


def latest_results(results_dir: Path) -> list[tuple[Path, dict]]:
    """Newest file per (backend, model), as `kai bench report` does."""
    best: dict[tuple, tuple[str, Path, dict]] = {}
    for p in sorted(results_dir.glob("*.json")):
        try:
            d = json.loads(p.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            continue
        if "tasks" not in d:
            continue
        key = (d.get("backend"), d.get("model"))
        if key not in best or (d.get("date", ""), p.name) > (best[key][0], best[key][1].name):
            best[key] = (d.get("date", ""), p, d)
    return [(p, d) for _, p, d in sorted(best.values(), key=lambda x: (x[0], x[1].name))]


def run_bench(a, ids: list[str], out_dir: Path) -> Path:
    out_dir.mkdir(parents=True, exist_ok=True)
    cmd = [a.kai_bin, "bench", "--yes", "--backend", a.backend, "--model", a.model,
           "--tasks", a.tasks_yaml, "--out", str(out_dir)]
    for i in ids:
        cmd += ["--only", i]
    print("+", shlex.join(cmd))
    if a.dry_run:
        return Path()
    before = set(out_dir.glob("*.json"))
    subprocess.run(cmd, check=True)
    new = sorted(set(out_dir.glob("*.json")) - before, key=lambda p: p.stat().st_mtime)
    if not new:
        new = sorted(out_dir.glob("*.json"), key=lambda p: p.stat().st_mtime)
    if not new:
        raise SystemExit(f"kai bench wrote no results JSON into {out_dir}")
    return new[-1]


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--model", default="kai", help="served model / adapter name (vLLM --lora-modules name)")
    ap.add_argument("--backend", default="vllm")
    ap.add_argument("--label", help="row label, e.g. sft-26b-v1 (default: model)")
    ap.add_argument("--heldout", default=str(kd.TRAIN_DIR / "heldout.txt"))
    ap.add_argument("--tasks-yaml", default="bench/tasks.yaml")
    ap.add_argument("--kai-bin", default="kai")
    ap.add_argument("--results-dir", default=str(kd.REPO / "bench" / "results"))
    ap.add_argument("--out-dir", default="train/out/eval")
    ap.add_argument("--results-md", default=str(kd.TRAIN_DIR / "results.md"))
    ap.add_argument("--from-json", help="use this kai bench results JSON instead of running")
    ap.add_argument("--dry-run", action="store_true",
                    help="print the bench command and the table; run nothing, append nothing")
    a = ap.parse_args(argv)

    ids = sorted(kd.load_heldout(a.heldout))
    bench = kd.load_bench_tasks(kd.REPO / a.tasks_yaml if not Path(a.tasks_yaml).is_absolute() else a.tasks_yaml)
    unknown = [i for i in ids if i not in bench]
    if unknown:
        raise SystemExit(f"held-out ids not in {a.tasks_yaml}: {unknown}")
    print(f"held-out tasks ({len(ids)}): {', '.join(ids)}")

    if a.from_json:
        res_path = Path(a.from_json)
    else:
        label = kd.safe_name(a.label or a.model)
        res_path = run_bench(a, ids, Path(a.out_dir) / label)
    mine = json.loads(res_path.read_text(encoding="utf-8")) if res_path.is_file() else None
    m = metrics(mine, ids) if mine else None

    print("\n| Results | Backend | Model | Run/held-out | Success | Steps/task | s/step |")
    print("|---|---|---|---|---|---|---|")
    if m:
        print(f"| **this run** ({res_path.name}) | {mine.get('backend')} | {mine.get('model')} | "
              f"{m['n']}/{len(ids)} | {fmt(m, 'success', pct=True)} | {fmt(m, 'steps_per_task')} | "
              f"{fmt(m, 's_per_step')} |")
    for p, d in latest_results(Path(a.results_dir)):
        if mine and p.resolve() == res_path.resolve():
            continue
        bm = metrics(d, ids)
        print(f"| {p.name} | {d.get('backend')} | {d.get('model')} | "
              f"{bm['n'] if bm else 0}/{len(ids)} | {fmt(bm, 'success', pct=True)} | "
              f"{fmt(bm, 'steps_per_task')} | {fmt(bm, 's_per_step')} |")
    if m and m["missing"]:
        print(f"\nnot in this run (skipped or failed to start): {m['missing']}")

    if not m:
        print("\nno results for the held-out tasks; nothing to record")
        return 0 if a.dry_run else 1
    row = (f"| {mine.get('date') or dt.date.today().isoformat()} | {a.label or a.model} | "
           f"{mine.get('backend')} | {mine.get('model')} | {m['n']}/{len(ids)} | {m['passed']} | "
           f"{fmt(m, 'success', pct=True)} | {fmt(m, 'steps_per_task')} | {fmt(m, 's_per_step')} | "
           f"{res_path.name} |\n")
    if a.dry_run:
        print("\nrow that would be appended to", a.results_md, ":\n" + row)
        return 0
    md = Path(a.results_md)
    text = md.read_text(encoding="utf-8") if md.exists() else "# Kai training results\n\n"
    if HEADER.splitlines()[0] not in text:
        text += "\n" + HEADER
    md.write_text(text + row, encoding="utf-8")
    print(f"\nappended to {md}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
