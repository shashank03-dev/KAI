#!/usr/bin/env bash
# Every check that runs without a GPU: byte-compile, synthetic data, all
# dry-runs, the proxy test, shell syntax. Uses train/.venv (dry-run deps only).
#   python3 -m venv train/.venv && train/.venv/bin/pip install -r train/requirements-dryrun.txt
#   train/tests/run_all.sh [chat_template.jinja]
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/../.."
PY="${PY:-train/.venv/bin/python}"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

echo "== py_compile"
"$PY" -m py_compile train/*.py train/tests/*.py

echo "== synthetic dataset (kai-trajectory/1)"
"$PY" train/tests/make_synthetic.py --out "$TMP/synth"

echo "== recordings reader"
"$PY" - "$TMP/synth/runs" <<'EOF'
import sys
sys.path.insert(0, "train")
import kai_data as kd, grpo_env as ge
dirs = kd.find_run_dirs(sys.argv[1])
calls = [c for d in dirs for c in ge.calls_from_recording(str(d))]
assert dirs and calls and all(c["images"] and c["user"] and c["reply"] for c in calls)
print(f"{len(dirs)} run dirs, {len(calls)} model calls read back")
EOF

echo "== sft dry-run (26B config)"
TPL=()
if [[ -n "${1:-}" ]]; then TPL=(--chat-template "$1"); fi
"$PY" train/sft_gemma.py --config train/configs/sft_26b.yaml --data "$TMP/synth/sft.jsonl" --dry-run "${TPL[@]}"
echo "== sft dry-run (E4B config, hashed val split)"
"$PY" train/sft_gemma.py --config train/configs/sft_e4b.yaml --data "$TMP/synth/sft.jsonl" --dry-run --show 0 --val-fraction 0.3

echo "== grpo dry-run"
"$PY" train/grpo_gemma.py --config train/configs/grpo_26b.yaml --dry-run --iterations 2 \
  --tasks-per-iter 3 --group-size 4 --output-dir "$TMP/grpo" --tasks-jsonl "$TMP/synth/grpo.jsonl"

echo "== eval dry-run"
"$PY" train/eval.py --model kai --label dry --dry-run
results=(bench/results/*-gemini-*.json)
"$PY" train/eval.py --from-json "${results[-1]}" --dry-run

echo "== rollout proxy"
"$PY" train/tests/test_proxy.py

echo "== shell syntax"
for f in deploy/digitalocean/serve-lora.sh deploy/digitalocean/train-up.sh deploy/digitalocean/train-down.sh train/tests/run_all.sh; do
  bash -n "$f"
done
if command -v shellcheck >/dev/null; then shellcheck deploy/digitalocean/{serve-lora,train-up,train-down}.sh train/tests/run_all.sh; fi
echo "ALL DRY-RUN CHECKS PASSED"
