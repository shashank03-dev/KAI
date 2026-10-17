#!/usr/bin/env python3
"""Plan, run and audit paired Kai bench trials for the OpenAI contract.

The plan command is read-only. The run command requires explicit execution
and a caller-provided preflight that verifies the model and desktop.
"""

import argparse
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import re
import shlex
import shutil
import subprocess
import sys


ROOT = Path(__file__).resolve().parent.parent
DEFAULT_ROOT = ROOT / "bench/results/oai-contract-ab"
FRAMES = ("marks", "pixels", "norm1000", "smart-resize")
BACKENDS = ("local", "vllm", "gemini", "gateway")


def slug(value):
    cleaned = re.sub(r"[^a-zA-Z0-9_.-]+", "-", value).strip(".-")
    if not cleaned:
        raise ValueError("empty experiment name")
    return cleaned


def utc_now():
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def experiment_directory(args):
    return args.root.resolve() / slug(args.backend) / slug(args.model) / slug(args.experiment)


def build_plan(args):
    if not args.only or len(set(args.only)) != len(args.only):
        raise ValueError("provide non-duplicate --only task IDs for a controlled comparison")
    if args.headless and not args.browser:
        raise ValueError("--headless requires --browser")
    if args.rounds < 2:
        raise ValueError("at least two runs per condition are required")
    if args.frame != "marks" and args.calibration is None:
        raise ValueError("pixel contracts require --calibration with a real calibration report")
    exp = experiment_directory(args)
    base = [
        str(args.kai_bin), "bench", "--backend", args.backend, "--model", args.model,
        "--tasks", str(args.tasks.resolve()), "--no-record",
    ]
    for task in args.only:
        base.extend(("--only", task))
    if args.browser:
        base.append("--browser")
    if args.headless:
        base.append("--headless")
    if args.yes:
        base.append("--yes")
    runs = []
    for round_number in range(1, args.rounds + 1):
        for condition in ("baseline", "contract"):
            label = f"{condition}-{round_number}"
            command = base.copy()
            if condition == "contract":
                command.extend(("--oai-contract", args.frame))
            command.extend(("--out", str(exp / label)))
            runs.append({"label": label, "condition": condition, "round": round_number,
                         "command": command, "status": "pending"})
    return {
        "schema": 1,
        "created_utc": utc_now(),
        "experiment_dir": str(exp),
        "backend": args.backend,
        "model": args.model,
        "frame": args.frame,
        "tasks": args.only,
        "browser": args.browser,
        "headless": args.headless,
        "rounds": args.rounds,
        "calibration": str(args.calibration.resolve()) if args.calibration else None,
        "runs": runs,
    }


def check_calibration(plan):
    if plan["frame"] == "marks":
        return
    path = Path(plan["calibration"])
    if not path.is_file():
        raise ValueError(f"calibration report missing: {path}")
    report = json.loads(path.read_text(encoding="utf-8"))
    if (report.get("status"), report.get("selected_frame"), report.get("model")) != (
        "calibrated", plan["frame"], plan["model"]
    ):
        raise ValueError("calibration must have matching model and a confirmed selected_frame")
    if not report.get("input_sha256") or len(report.get("scores", [])) != 3:
        raise ValueError("calibration report is missing trial provenance or frame scores")


def preflight(args, plan):
    check_calibration(plan)
    binary = shutil.which(str(args.kai_bin))
    if not binary or not os.access(binary, os.X_OK):
        raise ValueError(f"not an executable kai binary: {args.kai_bin}")
    if not args.tasks.is_file():
        raise ValueError(f"task manifest missing: {args.tasks}")
    if args.browser:
        browser = os.environ.get("KAI_BROWSER")
        if browser and (not Path(browser).is_file() or not os.access(browser, os.X_OK)):
            raise ValueError(f"KAI_BROWSER does not point to an executable: {browser}")
        if not browser and not any(shutil.which(b) for b in ("chromium", "google-chrome", "chrome", "brave", "microsoft-edge")):
            raise ValueError("no browser detected; set KAI_BROWSER to a runnable Chromium-family binary")
    elif sys.platform.startswith("linux") and not (os.environ.get("DISPLAY") or os.environ.get("WAYLAND_DISPLAY")):
        raise ValueError("no desktop display detected for a desktop bench")
    if not args.preflight_cmd:
        raise ValueError("--preflight-cmd is required: verify model availability/credentials and desktop")
    command = shlex.split(args.preflight_cmd)
    if not command:
        raise ValueError("empty --preflight-cmd")
    print("Running model/desktop preflight", file=sys.stderr)
    subprocess.run(command, cwd=ROOT, check=True)


def write_json(path, data):
    path.parent.mkdir(parents=True, exist_ok=True)
    temp = path.with_suffix(".tmp")
    temp.write_text(json.dumps(data, indent=2, allow_nan=False) + "\n", encoding="utf-8")
    temp.replace(path)


def run_experiment(args, plan):
    if not args.execute:
        raise ValueError("running live models requires `run --execute`")
    preflight(args, plan)
    root = Path(plan["experiment_dir"])
    if root.exists():
        raise ValueError(f"experiment directory already exists: {root}; use a new --experiment")
    manifest = root / "manifest.json"
    write_json(manifest, plan)
    for run in plan["runs"]:
        out_dir = root / run["label"]
        out_dir.mkdir(parents=True)
        run["started_utc"] = utc_now()
        run["status"] = "running"
        write_json(manifest, plan)
        with (out_dir / "kai.log").open("w", encoding="utf-8") as log:
            outcome = subprocess.run(run["command"], stdout=log, stderr=subprocess.STDOUT,
                                     cwd=ROOT, check=False)
        run["returncode"] = outcome.returncode
        run["finished_utc"] = utc_now()
        run["status"] = "done" if outcome.returncode == 0 else "failed"
        write_json(manifest, plan)
        print(f"{run['label']}: {run['status']} (exit {outcome.returncode})")
        if outcome.returncode:
            raise ValueError(f"{run['label']} failed; inspect {out_dir / 'kai.log'}")
    print(f"Results: {root}")
    return root


def read_run(root, plan, run):
    if run.get("status") != "done" or run.get("returncode") != 0:
        raise ValueError(f"{run['label']} has not finished successfully")
    output_dir = root / run["label"]
    files = list(output_dir.glob("*.json"))
    if len(files) != 1:
        raise ValueError(f"{run['label']} must have one Kai result JSON (found {len(files)})")
    result = json.loads(files[0].read_text(encoding="utf-8"))
    expected_backend = plan["backend"] + ("+browser" if plan["browser"] else "")
    if result.get("backend") != expected_backend:
        raise ValueError(f"{run['label']}: unexpected backend {result.get('backend')!r}")
    if result.get("skipped") != 0 or result.get("total") != len(plan["tasks"]):
        raise ValueError(f"{run['label']}: skipped tasks or wrong task count")
    ab = result.get("ab")
    if not isinstance(ab, dict) or not ab.get("applied"):
        raise ValueError(f"{run['label']}: missing A/B configuration evidence")
    flags = ab.get("flags", ab)
    if not isinstance(flags, dict):
        raise ValueError(f"{run['label']}: A/B flags cannot be inspected")
    enabled = flags.get("oai_contract") is not None
    if enabled != (run["condition"] == "contract"):
        raise ValueError(f"{run['label']}: wrong --oai-contract condition in Kai result")
    tasks = result.get("tasks")
    if not isinstance(tasks, list) or {t.get("id") for t in tasks} != set(plan["tasks"]):
        raise ValueError(f"{run['label']}: task IDs disagree with the manifest")
    if len(tasks) != len(plan["tasks"]):
        raise ValueError(f"{run['label']}: duplicate task IDs")
    for task in tasks:
        if type(task.get("pass")) is not bool:
            raise ValueError(f"{run['label']}: missing checked pass/fail for {task['id']}")
        for field in ("steps", "model_calls", "seconds"):
            value = task.get(field)
            if type(value) not in (int, float) or value < 0:
                raise ValueError(f"{run['label']}: invalid {field} for {task['id']}")
    if not isinstance(result.get("machine"), dict):
        raise ValueError(f"{run['label']}: missing machine details")
    return files[0], result


def aggregate(results):
    tasks = [task for _, _, result in results for task in result["tasks"]]
    count = len(tasks)
    total_passed = sum(task["pass"] for task in tasks)
    by_task = {}
    for task in tasks:
        item = by_task.setdefault(task["id"], {"passed": 0, "runs": 0, "steps": 0,
                                                "model_calls": 0, "seconds": 0.0})
        item["passed"] += task["pass"]
        item["runs"] += 1
        for field in ("steps", "model_calls", "seconds"):
            item[field] += task[field]
    optional = {}
    for field in ("format_repairs", "misclicks"):
        optional[field] = (
            sum(task[field] for task in tasks)
            if all(type(task.get(field)) in (int, float) for task in tasks)
            else None
        )
    return {
        "passed": total_passed,
        "total": count,
        "success_rate": total_passed / count,
        "mean_steps": round(sum(t["steps"] for t in tasks) / count, 3),
        "mean_model_calls": round(sum(t["model_calls"] for t in tasks) / count, 3),
        "mean_seconds": round(sum(t["seconds"] for t in tasks) / count, 3),
        **optional,
        "tasks": by_task,
        "result_files": [str(path) for _, path, _ in results],
    }


def compare_experiment(root):
    manifest = root / "manifest.json"
    if not manifest.is_file():
        raise ValueError(f"manifest missing: {manifest}")
    plan = json.loads(manifest.read_text(encoding="utf-8"))
    if plan.get("schema") != 1 or plan.get("rounds", 0) < 2:
        raise ValueError("unsupported experiment schema or fewer than two rounds")
    expected_labels = {
        f"{condition}-{round_number}"
        for round_number in range(1, plan["rounds"] + 1)
        for condition in ("baseline", "contract")
    }
    if {run.get("label") for run in plan["runs"]} != expected_labels or len(plan["runs"]) != len(expected_labels):
        raise ValueError("manifest is missing or repeats an experiment condition")
    gathered = {"baseline": [], "contract": []}
    interrupted = []
    models = set()
    machines = set()
    for run in plan["runs"]:
        if run["condition"] not in gathered or run["label"] != f"{run['condition']}-{run['round']}":
            raise ValueError("invalid run condition")
        path, result = read_run(root, plan, run)
        for task in result["tasks"]:
            if task.get("outcome") in ("stopped", "asked_user"):
                interrupted.append(f"{run['label']}: {task['id']} ended {task['outcome']}")
        models.add(result.get("model"))
        machine = result["machine"]
        machines.add(tuple(machine.get(key) for key in ("os", "cpu", "cores", "display", "git_commit")))
        gathered[run["condition"]].append((run["label"], path, result))
    if len(models) != 1 or len(machines) != 1:
        raise ValueError("model or machine changed during the paired experiment")
    return {
        "schema": 1,
        "experiment_dir": str(root),
        "backend": plan["backend"],
        "model": plan["model"],
        "served_model": next(iter(models)),
        "frame": plan["frame"],
        "tasks": plan["tasks"],
        "rounds_per_side": plan["rounds"],
        "valid_comparison": not interrupted,
        "interruptions": interrupted,
        "baseline": aggregate(gathered["baseline"]),
        "contract": aggregate(gathered["contract"]),
    }


def markdown_report(comparison):
    lines = [
        "# Kai baseline vs OpenAI contract (measured runs)", "",
        f"Backend: `{comparison['backend']}` · Model: `{comparison['model']}` · "
        f"Contract frame: `{comparison['frame']}` · "
        f"Replicates per side: {comparison['rounds_per_side']}", "",
        ("**Comparison validity: complete runs**" if comparison["valid_comparison"]
         else "**Comparison validity: invalid due to interrupted task execution.**"),
        *(f"- {reason}" for reason in comparison["interruptions"]),
        "" if not comparison["valid_comparison"] else "",
        "| Metric | Baseline | Contract |", "| --- | ---: | ---: |",
    ]
    for label, key in (
        ("Checked task success", "passed"),
        ("Mean steps/task", "mean_steps"),
        ("Mean model calls/task", "mean_model_calls"),
        ("Mean seconds/task", "mean_seconds"),
        ("Format repairs", "format_repairs"),
        ("Misclicks", "misclicks"),
    ):
        def value(side):
            entry = comparison[side]
            if key == "passed":
                return f"{entry['passed']}/{entry['total']} ({entry['success_rate']:.1%})"
            return "unavailable" if entry[key] is None else str(entry[key])
        lines.append(f"| {label} | {value('baseline')} | {value('contract')} |")
    lines.extend(("", "## Per-task passes", "", "| Task | Baseline | Contract |",
                  "| --- | ---: | ---: |"))
    for task in comparison["tasks"]:
        def passes(side):
            item = comparison[side]["tasks"][task]
            return f"{item['passed']}/{item['runs']}"
        lines.append(f"| {task} | {passes('baseline')} | {passes('contract')} |")
    lines.extend(("", "Source JSON files:", ""))
    for side in ("baseline", "contract"):
        lines.extend(f"- `{path}`" for path in comparison[side]["result_files"])
    lines.extend(("", "Metric fields absent from the source JSON are unavailable, not zero.", ""))
    return "\n".join(lines)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="action", required=True)
    for action in ("plan", "run"):
        sub = commands.add_parser(action)
        sub.add_argument("--backend", required=True, choices=BACKENDS)
        sub.add_argument("--model", required=True)
        sub.add_argument("--frame", required=True, choices=FRAMES)
        sub.add_argument("--only", action="append", default=[], metavar="TASK")
        sub.add_argument("--rounds", type=int, default=2)
        sub.add_argument("--root", type=Path, default=DEFAULT_ROOT)
        sub.add_argument("--experiment", default=datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ"))
        sub.add_argument("--tasks", type=Path, default=ROOT / "bench/tasks.yaml")
        sub.add_argument("--kai-bin", default="kai")
        sub.add_argument("--browser", action="store_true")
        sub.add_argument("--headless", action="store_true")
        sub.add_argument("--yes", action="store_true", help="auto-approve ordinary bench steps; computer actions still need a human at the terminal")
        sub.add_argument("--calibration", type=Path, help="successful calibration report for this model/frame")
        if action == "run":
            sub.add_argument("--execute", action="store_true", help="explicitly authorize live model trials")
            sub.add_argument("--preflight-cmd", help="command that checks model, keys and desktop; must exit zero")
    report = commands.add_parser("report")
    report.add_argument("--root", type=Path, default=DEFAULT_ROOT)
    report.add_argument("--backend", required=True, choices=BACKENDS)
    report.add_argument("--model", required=True)
    report.add_argument("--experiment", required=True)
    args = parser.parse_args()
    try:
        if args.action == "report":
            output_dir = experiment_directory(args)
            comparison = compare_experiment(output_dir)
            write_json(output_dir / "comparison.json", comparison)
            (output_dir / "comparison.md").write_text(markdown_report(comparison), encoding="utf-8")
            print(f"Comparison written: {output_dir / 'comparison.md'}")
        else:
            plan = build_plan(args)
            if args.action == "plan":
                print(f"Experiment: {plan['experiment_dir']}")
                for trial in plan["runs"]:
                    print(f"{trial['label']}: {shlex.join(trial['command'])}")
                print("Planning only; no model calls or desktop actions.")
            else:
                run_experiment(args, plan)
    except (ValueError, OSError, subprocess.CalledProcessError, json.JSONDecodeError) as error:
        parser.exit(2, f"benchmark failed: {error}\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
