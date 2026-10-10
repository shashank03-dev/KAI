#!/usr/bin/env bash
# Serve a LoRA adapter (from train/sft_gemma.py or train/grpo_gemma.py) on the
# Droplet that up.sh created, next to the base model, under the name "kai".
#
#   deploy/digitalocean/serve-lora.sh train/out/sft-26b/adapter        # serve as "kai"
#   deploy/digitalocean/serve-lora.sh train/out/grpo-26b/iter-019 kai2 # another name
#   deploy/digitalocean/serve-lora.sh --remove                         # back to base only
#
# Then point Kai at it (the base model stays available under its own name):
#   source deploy/digitalocean/.secrets/endpoint.env
#   export KAI_ENDPOINT VLLM_API_KEY
#   kai run   --backend vllm --model kai "find the 5 biggest files in Downloads"
#   kai bench --backend vllm --model kai
# In the app: Settings > model id = kai (backend: DigitalOcean / vLLM).
#
# How: copies the adapter to /root/adapters/<name> on the Droplet and adds a
# systemd drop-in that restarts the same vLLM container with
#   --enable-lora --lora-modules <name>=/adapters/<name> --max-lora-rank <r>
# (r = the adapter's rank rounded up to a size vLLM accepts). The base model,
# API key, port, prefix caching and image limits stay exactly as in
# cloud-init.yaml. No secrets are written by this script.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
[[ -f "$here/.secrets/endpoint.env" ]] || { echo "ERROR: run up.sh first (no .secrets/endpoint.env)."; exit 1; }
# shellcheck disable=SC1091
source "$here/.secrets/endpoint.env"
: "${KAI_IP:?endpoint.env has no KAI_IP}"
SSH=(ssh -o StrictHostKeyChecking=accept-new "root@${KAI_IP}")
VLLM_IMAGE="${VLLM_IMAGE:-vllm/vllm-openai:v0.25.0}"   # same image as cloud-init.yaml

if [[ "${1:-}" == "--remove" ]]; then
  "${SSH[@]}" 'rm -f /etc/systemd/system/vllm.service.d/lora.conf && systemctl daemon-reload && systemctl restart vllm'
  echo "LoRA removed; vLLM restarting with the base model only. Watch: curl -s https://${KAI_HOST:-<host>}/health"
  exit 0
fi

ADAPTER="${1:?usage: serve-lora.sh <adapter-dir> [name] | --remove}"
NAME="${2:-kai}"
[[ "$NAME" =~ ^[A-Za-z0-9._-]+$ ]] || { echo "ERROR: name must be [A-Za-z0-9._-]+"; exit 1; }
[[ -f "$ADAPTER/adapter_config.json" ]] || { echo "ERROR: $ADAPTER/adapter_config.json not found (not a PEFT adapter dir)."; exit 1; }
[[ -f "$ADAPTER/adapter_model.safetensors" ]] || { echo "ERROR: $ADAPTER/adapter_model.safetensors not found."; exit 1; }

# The adapter's rank -> --max-lora-rank (must be >= every served adapter's rank).
RANK="$(python3 -c 'import json,sys; c=json.load(open(sys.argv[1])); print(int(c.get("r", 16)))' "$ADAPTER/adapter_config.json")"
MAX_RANK=8
for r in 8 16 32 64 128 256; do
  if (( RANK <= r )); then MAX_RANK=$r; break; fi
done
BASE="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1])).get("base_model_name_or_path",""))' "$ADAPTER/adapter_config.json")"
echo "Adapter: $ADAPTER (rank $RANK, base '${BASE:-?}') -> served as '$NAME', --max-lora-rank $MAX_RANK"

"${SSH[@]}" "mkdir -p /root/adapters/$NAME"
scp -o StrictHostKeyChecking=accept-new "$ADAPTER/adapter_config.json" "$ADAPTER/adapter_model.safetensors" \
  "root@${KAI_IP}:/root/adapters/$NAME/"

# The adapter must be trained on the model the Droplet serves.
REMOTE_MODEL="$("${SSH[@]}" "sed -n 's/^MODEL_ID=//p' /etc/kai/vllm.env")"
if [[ -n "$BASE" && "${BASE,,}" != "${REMOTE_MODEL,,}" ]]; then
  echo "ERROR: adapter base '$BASE' != Droplet model '$REMOTE_MODEL'. Refusing."
  exit 1
fi

"${SSH[@]}" "mkdir -p /etc/systemd/system/vllm.service.d && cat > /etc/systemd/system/vllm.service.d/lora.conf" <<EOF
# Written by deploy/digitalocean/serve-lora.sh: base model + LoRA adapter "$NAME".
[Service]
ExecStartPre=
ExecStartPre=-/usr/bin/docker rm -f kai-vllm
ExecStartPre=/usr/bin/docker pull $VLLM_IMAGE
ExecStart=
ExecStart=/usr/bin/docker run --rm --name kai-vllm \\
  --gpus all --ipc=host --shm-size 16g \\
  -p 127.0.0.1:8000:8000 \\
  -e HF_TOKEN=\${HF_TOKEN} \\
  -e VLLM_API_KEY=\${VLLM_API_KEY} \\
  -v /root/.cache/huggingface:/root/.cache/huggingface \\
  -v /root/adapters:/adapters:ro \\
  $VLLM_IMAGE \\
  --model \${MODEL_ID} \\
  --served-model-name \${SERVED_NAME} \\
  --host 0.0.0.0 --port 8000 \\
  --api-key \${VLLM_API_KEY} \\
  --max-model-len 16384 \\
  --gpu-memory-utilization 0.90 \\
  --limit-mm-per-prompt.image 2 \\
  --enable-prefix-caching \\
  --async-scheduling \\
  --enable-lora \\
  --max-loras 1 \\
  --max-lora-rank $MAX_RANK \\
  --lora-modules $NAME=/adapters/$NAME
EOF

"${SSH[@]}" 'systemctl daemon-reload && systemctl restart vllm'
echo "vLLM restarting with LoRA '$NAME' (model reload takes a few minutes)."
echo "Check it is listed:"
echo "  curl -s -H \"Authorization: Bearer \$VLLM_API_KEY\" \$KAI_ENDPOINT/models | python3 -m json.tool"
echo "Use it:  kai bench --backend vllm --model $NAME   (with KAI_ENDPOINT/VLLM_API_KEY from .secrets/endpoint.env)"
