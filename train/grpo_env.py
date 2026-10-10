"""The RL environment for GRPO: Kai itself, on throwaway desktops.

One *episode* = one bench task attempted once by the current policy:

  1. fresh HOME for the episode, on one of N parallel Xvfb desktops
     (scripts/desktop/start.sh with KAI_DISPLAY=:1NN, its own D-Bus/AT-SPI);
  2. the task's `setup` from bench/tasks.yaml;
  3. `kai run --yes --backend vllm --model <policy> --record <dir> "<prompt>"`
     with KAI_ENDPOINT pointing at the rollout proxy (train/rollout_proxy.py),
     which samples at temperature > 0 and logs every model call;
  4. the task's `check` (exit 0 = pass), then `teardown`, then every window on
     that display is closed.

Reward = check (1/0) - step_penalty * model calls - time_penalty * minutes
         + format_weight * (fraction of replies Kai accepts - 1).
Episodes that fail for environment reasons (setup failed, Kai crashed before
any model call) are dropped from their group rather than scored 0.

Safety allowlist: only task ids that exist in bench/tasks.yaml with a Linux
`check`; the setup/check/teardown commands are ALWAYS taken from that file,
never from the GRPO JSONL; held-out tasks are refused; tasks that need the
internet are refused unless --allow-internet; the child gets a scrubbed
environment (no OPENAI/GEMINI/AI_GATEWAY/HF keys, a fresh empty HOME, its own
D-Bus session so no keyring), so a rollout can never reach a real account.
"""

from __future__ import annotations

import json
import os
import random
import shlex
import shutil
import signal
import statistics
import subprocess
import threading
import time
import zlib
from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass, field
from pathlib import Path
from queue import Queue

import kai_data as kd
from rollout_proxy import read_calls

PASS_ENV = ("PATH", "LANG", "LC_ALL", "TERM", "USER", "LOGNAME", "SHELL", "TZ")
NEVER_ENV = ("OPENAI_API_KEY", "GEMINI_API_KEY", "AI_GATEWAY_API_KEY", "HF_TOKEN",
             "HUGGING_FACE_HUB_TOKEN", "ANTHROPIC_API_KEY", "DIGITALOCEAN_ACCESS_TOKEN")


# ------------------------------------------------------------ allowlist ----

def allowed_tasks(bench: dict[str, kd.BenchTask], heldout: set[str],
                  allow_internet: bool = False) -> tuple[dict[str, kd.BenchTask], dict[str, str]]:
    ok, refused = {}, {}
    for tid, t in bench.items():
        if tid in heldout:
            refused[tid] = "held out for evaluation"
        elif not t.check:
            refused[tid] = "no linux check (no reward signal)"
        elif "internet" in t.needs and not allow_internet:
            refused[tid] = "needs the internet (real sites); pass --allow-internet"
        else:
            ok[tid] = t
    return ok, refused


def load_grpo_tasks(path: str | None, allowed: dict[str, kd.BenchTask]) -> tuple[list[kd.BenchTask], list[str]]:
    """Tasks from `kai data export --format grpo` (or all allowed tasks).

    Only `task_id` is trusted from the JSONL; commands come from tasks.yaml.
    """
    if not path:
        return list(allowed.values()), []
    rows, errors = kd.read_jsonl(path)
    out, notes = [], list(errors)
    for r in rows:
        tid = r.get("task_id") or r.get("id")
        if tid not in allowed:
            notes.append(f"refused task {tid!r}: not an allowed bench task")
            continue
        t = allowed[tid]
        if r.get("check") and r["check"] != t.check:
            notes.append(f"{tid}: JSONL check differs from bench/tasks.yaml; using tasks.yaml")
        out.append(t)
    return out, notes


# --------------------------------------------------------------- reward ----

@dataclass
class RewardConfig:
    step_penalty: float = 0.01
    time_penalty_per_min: float = 0.02
    format_weight: float = 0.2


@dataclass
class Episode:
    episode_id: str
    task_id: str
    check: bool | None = None
    model_calls: int = 0
    seconds: float = 0.0
    valid_replies: int = 0
    exit_code: int | None = None
    env_error: str | None = None
    run_dir: str | None = None
    calls: list = field(default_factory=list)
    reward: float = 0.0
    advantage: float = 0.0


def score(ep: Episode, rc: RewardConfig) -> float:
    valid_frac = ep.valid_replies / ep.model_calls if ep.model_calls else 0.0
    return (float(bool(ep.check)) - rc.step_penalty * ep.model_calls
            - rc.time_penalty_per_min * ep.seconds / 60.0
            + rc.format_weight * (valid_frac - 1.0))


def group_advantages(group: list[Episode], scale_std: bool = True) -> bool:
    """Set ep.advantage in place: (r - mean) / std over the valid episodes.
    Returns False (all advantages 0) when the group carries no signal."""
    valid = [e for e in group if e.env_error is None]
    for e in group:
        e.advantage = 0.0
    if len(valid) < 2:
        return False
    rs = [e.reward for e in valid]
    mean, std = statistics.fmean(rs), statistics.pstdev(rs)
    if std < 1e-6:
        return False
    for e in valid:
        e.advantage = (e.reward - mean) / (std if scale_std else 1.0)
    return True


def episode_samples(ep: Episode, per_episode_norm: bool = True) -> list[dict]:
    """Per-model-call training samples carrying the episode's advantage."""
    if ep.env_error or ep.advantage == 0.0 or not ep.calls:
        return []
    w = 1.0 / len(ep.calls) if per_episode_norm else 1.0
    return [{"system": c["system"], "user": c["user"], "images": c["images"],
             "reply": c["reply"], "vllm_logprobs": c.get("logprobs"),
             "advantage": ep.advantage, "weight": w, "episode": ep.episode_id,
             "task_id": ep.task_id} for c in ep.calls]


def calls_from_recording(run_dir: str) -> list[dict]:
    rec = kd.read_recording(run_dir)
    return [{"system": rec["system"], "user": s["user"], "images": s["images"],
             "reply": s["reply"], "logprobs": None} for s in rec["steps"]]


# -------------------------------------------------------------- desktops ----

@dataclass
class Desktop:
    index: int
    display: str
    state_dir: Path
    env: dict = field(default_factory=dict)

    def start(self) -> dict:
        env = {k: os.environ[k] for k in PASS_ENV if k in os.environ}
        env.update(KAI_DISPLAY=self.display, KAI_DESKTOP_DIR=str(self.state_dir),
                   KAI_SCREEN="1920x1080x24", HOME=str(self.state_dir / "home0"))
        Path(env["HOME"]).mkdir(parents=True, exist_ok=True)
        subprocess.run(["bash", str(kd.REPO / "scripts/desktop/start.sh")], env=env,
                       check=True, capture_output=True, timeout=60)
        self.env = parse_env_file(self.state_dir / "env")
        return self.env

    def close_windows(self):
        env = dict(self.env, PATH=os.environ.get("PATH", "/usr/bin:/bin"))
        try:
            out = subprocess.run(["wmctrl", "-l"], env=env, capture_output=True, text=True,
                                 timeout=10).stdout
            for line in out.splitlines():
                subprocess.run(["wmctrl", "-i", "-c", line.split()[0]], env=env, timeout=10)
        except (OSError, subprocess.SubprocessError):
            pass


def parse_env_file(path: Path) -> dict:
    env = {}
    for line in Path(path).read_text().splitlines():
        if line.startswith("export "):
            for tok in shlex.split(line[len("export "):]):
                if "=" in tok:
                    k, v = tok.split("=", 1)
                    env[k] = v
    return env


# --------------------------------------------------------------- runners ----

@dataclass
class RunnerConfig:
    kai_bin: str = "kai"
    bench_dir: Path = kd.REPO / "bench"
    work_dir: Path = Path("train/out/rollouts")
    episode_timeout_s: float = 660.0   # Kai's own limit is 10 min; small margin
    vllm_api_key_env: str = "VLLM_API_KEY"


def _sh(cmd: str, env: dict, timeout: float) -> int:
    try:
        return subprocess.run(["sh", "-c", cmd], env=env, timeout=timeout,
                              stdin=subprocess.DEVNULL, capture_output=True).returncode
    except subprocess.TimeoutExpired:
        return 124


class KaiRunner:
    """Runs one real episode. Needs a desktop, the kai binary and the proxy."""

    def __init__(self, cfg: RunnerConfig, proxy):
        self.cfg, self.proxy = cfg, proxy

    def run(self, task: kd.BenchTask, policy: str, desk: Desktop, episode_id: str) -> Episode:
        ep = Episode(episode_id=episode_id, task_id=task.id)
        home = desk.state_dir / "homes" / episode_id
        shutil.rmtree(home, ignore_errors=True)
        home.mkdir(parents=True)
        env = {k: os.environ[k] for k in PASS_ENV if k in os.environ}
        env.update(desk.env)
        env.update(HOME=str(home), KAI_BENCH_DIR=str(self.cfg.bench_dir),
                   KAI_ENDPOINT=self.proxy.endpoint(episode_id),
                   VLLM_API_KEY=os.environ.get(self.cfg.vllm_api_key_env, "local"))
        for k in NEVER_ENV:
            env.pop(k, None)
        if task.setup and _sh(task.setup, env, 120) != 0:
            ep.env_error = "setup failed"
            return ep
        rec_dir = Path(self.cfg.work_dir) / "recordings" / episode_id
        rec_dir.parent.mkdir(parents=True, exist_ok=True)
        cmd = [self.cfg.kai_bin, "run", "--yes", "--backend", "vllm", "--model", policy,
               "--record", str(rec_dir), task.prompt]
        t0 = time.time()
        proc = subprocess.Popen(cmd, env=env, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                                stderr=subprocess.DEVNULL, start_new_session=True)
        try:
            ep.exit_code = proc.wait(timeout=self.cfg.episode_timeout_s)
        except subprocess.TimeoutExpired:
            os.killpg(proc.pid, signal.SIGKILL)
            proc.wait()
            ep.exit_code = -9
        ep.seconds = time.time() - t0
        ep.check = _sh(task.check, env, 60) == 0 if task.check else None
        if task.teardown:
            _sh(task.teardown, env, 60)
        desk.close_windows()
        runs = kd.find_run_dirs(rec_dir) if rec_dir.exists() else []
        ep.run_dir = str(runs[-1]) if runs else None
        ep.calls = read_calls(self.proxy.cfg.log_dir, episode_id)
        if not ep.calls and ep.run_dir:
            ep.calls = calls_from_recording(ep.run_dir)
        finish(ep)
        if ep.model_calls == 0:
            ep.env_error = f"no model call (kai exit {ep.exit_code})"
        shutil.rmtree(home, ignore_errors=True)
        return ep


def finish(ep: Episode):
    ep.model_calls = len(ep.calls)
    ep.valid_replies = sum(1 for c in ep.calls if kd.check_reply(c["reply"])[0])


class FakeRunner:
    """Dry-run stand-in: no desktop, no kai, no model. Writes proxy-format call
    logs and a minimal recording so the rest of the pipeline runs for real."""

    def __init__(self, work_dir: Path, seed: int = 0, image: Path | None = None):
        self.work_dir = Path(work_dir)
        self.rng = random.Random(seed)
        self.lock = threading.Lock()
        self.image = image

    def _img(self) -> str:
        with self.lock:
            if self.image is None or not self.image.exists():
                from PIL import Image
                self.image = self.work_dir / "fake-frame.jpg"
                self.work_dir.mkdir(parents=True, exist_ok=True)
                Image.new("RGB", (1070, 602), (230, 230, 230)).save(self.image, "JPEG")
        return str(self.image)

    def run(self, task: kd.BenchTask, policy: str, desk: Desktop, episode_id: str) -> Episode:
        with self.lock:
            p = 0.25 + 0.5 * (zlib.crc32(task.id.encode()) % 7) / 7
            success = self.rng.random() < p
            n = self.rng.randint(1, 6)
            malformed = self.rng.random() < 0.15
        ep = Episode(episode_id=episode_id, task_id=task.id, check=success, exit_code=0,
                     seconds=n * 2.5)
        img = self._img()
        rec = self.work_dir / "recordings" / episode_id
        (rec / "frames").mkdir(parents=True, exist_ok=True)
        steps = []
        for i in range(n):
            last = i == n - 1
            reply = (json.dumps({"thought": "finished", "action": "done", "risk": "none"}) if last
                     else json.dumps({"thought": "run it", "action": "shell", "text": "ls ~",
                                      "risk": "low", "expect": "listing"}))
            if malformed and i == 0:
                reply = "I think I should click."
            user = f"Elements:\n#1 button \"OK\"\n\nTask: {task.prompt}\n\nRespond with the next action as JSON."
            ep.calls.append({"system": "You are Kai. (fake)", "user": user, "images": [img],
                             "reply": reply, "logprobs": None})
            shutil.copy(img, rec / "frames" / f"step-{i:03d}.jpg")
            steps.append({"index": i, "screenshot": f"frames/step-{i:03d}.jpg",
                          "prompt_user": user, "reply_raw": reply, "verified": True})
        (rec / "trace.json").write_text(json.dumps({"task": task.prompt, "steps": steps,
                                                    "outcome": "success" if success else "failed",
                                                    "total_ms": int(ep.seconds * 1000)}))
        (rec / "system.txt").write_text("You are Kai. (fake)")
        ep.run_dir = str(rec)
        finish(ep)
        return ep


# ------------------------------------------------------------------ pool ----

class RolloutPool:
    """Runs groups of episodes across N desktops in parallel."""

    def __init__(self, runner, desktops: list[Desktop]):
        self.runner = runner
        self.free: Queue = Queue()
        for d in desktops:
            self.free.put(d)
        self.n = len(desktops)

    def _one(self, task, policy, episode_id):
        desk = self.free.get()
        try:
            return self.runner.run(task, policy, desk, episode_id)
        except Exception as e:  # noqa: BLE001 - one broken episode must not stop training
            return Episode(episode_id=episode_id, task_id=task.id, env_error=f"runner: {e}")
        finally:
            self.free.put(desk)

    def run_groups(self, tasks: list[kd.BenchTask], group_size: int, policy: str,
                   tag: str) -> dict[str, list[Episode]]:
        jobs = [(t, f"{tag}-{kd.safe_name(t.id)}-{g}") for t in tasks for g in range(group_size)]
        with ThreadPoolExecutor(max_workers=self.n) as ex:
            eps = list(ex.map(lambda j: self._one(j[0], policy, j[1]), jobs))
        groups: dict[str, list[Episode]] = {}
        for e in eps:
            groups.setdefault(e.task_id, []).append(e)
        return groups


def make_desktops(n: int, base_display: int, root: Path, start: bool) -> list[Desktop]:
    ds = [Desktop(i, f":{base_display + i}", Path(root) / f"desktop-{i}") for i in range(n)]
    for d in ds:
        d.state_dir.mkdir(parents=True, exist_ok=True)
        if start:
            d.start()
    return ds
