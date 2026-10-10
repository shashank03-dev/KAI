#!/usr/bin/env bash
# Create a GPU Droplet for TRAINING (train/README.md): 1x H100 80GB with the
# pinned training stack (train/requirements-gpu.txt), Kai built from source,
# the headless desktop for GRPO rollouts, and a localhost-only vLLM with LoRA
# hot-swap + sleep mode. SSH only (from your IP); nothing is served publicly.
#
#   export HF_TOKEN=hf_...  KAI_SSH_KEY=my-laptop
#   deploy/digitalocean/train-up.sh
#   deploy/digitalocean/train-down.sh     # ALWAYS, when done: it bills while it exists
#
# Env: KAI_REGION (nyc2), KAI_SIZE (gpu-h100x1-80gb), KAI_TRAIN_DROPLET (kai-train),
#      KAI_TRAIN_MODEL (google/gemma-4-26B-A4B-it), KAI_REPO_URL, KAI_REPO_REF (main).
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$here"

REGION="${KAI_REGION:-nyc2}"
SIZE="${KAI_SIZE:-gpu-h100x1-80gb}"
DROPLET_NAME="${KAI_TRAIN_DROPLET:-kai-train}"
TAG="kai-train"
SSH_KEY_NAME="${KAI_SSH_KEY:-}"
IMAGE="${KAI_IMAGE:-ubuntu-24-04-x64}"
MODEL_ID="${KAI_TRAIN_MODEL:-google/gemma-4-26B-A4B-it}"
REPO_URL="${KAI_REPO_URL:-https://github.com/shashank03-dev/KAI}"
REPO_REF="${KAI_REPO_REF:-main}"

command -v doctl >/dev/null || { echo "ERROR: doctl not found. See README.md"; exit 1; }
doctl account get >/dev/null 2>&1 || { echo "ERROR: 'doctl auth init' first."; exit 1; }
: "${HF_TOKEN:?Set HF_TOKEN in your environment. Never commit it.}"
[[ -n "$SSH_KEY_NAME" ]] || { echo "ERROR: set KAI_SSH_KEY (doctl compute ssh-key list)."; exit 1; }
for v in "$MODEL_ID" "$REPO_URL" "$REPO_REF"; do
  [[ "$v" =~ ^[A-Za-z0-9._:/@-]+$ ]] || { echo "ERROR: unexpected characters in '$v'"; exit 1; }
done

if ! doctl compute size list --format Slug --no-header | grep -qx "$SIZE"; then
  echo "ERROR: size '$SIZE' not offered to this account. GPU sizes:"
  doctl compute size list --format Slug,Price_Hourly --no-header | grep -i gpu || true
  exit 1
fi
PRICE_HR="$(doctl compute size list --format Slug,Price_Hourly --no-header | awk -v s="$SIZE" '$1==s{print $2}')"
echo "======================================================================"
echo "  Creating TRAINING Droplet $DROPLET_NAME ($SIZE, $REGION), model $MODEL_ID"
echo "  \$${PRICE_HR:-~3.39}/hr, billed while it exists. Destroy with train-down.sh."
echo "======================================================================"

mkdir -p .secrets
if [[ ! -f .secrets/vllm_train_api_key ]]; then
  (openssl rand -hex 24 2>/dev/null || head -c 24 /dev/urandom | xxd -p) | tr -d '\n' > .secrets/vllm_train_api_key
fi
chmod 600 .secrets/vllm_train_api_key
VLLM_API_KEY="$(cat .secrets/vllm_train_api_key)"

MY_IP="$(curl -fsS https://checkip.amazonaws.com | tr -d '\n')"
FW_NAME="kai-train-fw"
if ! doctl compute firewall list --format Name --no-header | grep -qx "$FW_NAME"; then
  doctl compute firewall create --name "$FW_NAME" --tag-names "$TAG" \
    --inbound-rules "protocol:tcp,ports:22,address:${MY_IP}/32" \
    --outbound-rules "protocol:tcp,ports:all,address:0.0.0.0/0,address:::/0 protocol:udp,ports:all,address:0.0.0.0/0,address:::/0"
else
  echo "Firewall $FW_NAME exists; leaving as-is (SSH from the IP it was created with)."
fi

RENDERED=".secrets/cloud-init-train.rendered.yaml"
umask 077
sed -e "s|__HF_TOKEN__|${HF_TOKEN}|g" \
    -e "s|__VLLM_API_KEY__|${VLLM_API_KEY}|g" \
    -e "s|__MODEL_ID__|${MODEL_ID}|g" \
    -e "s|__REPO_URL__|${REPO_URL}|g" \
    -e "s|__REPO_REF__|${REPO_REF}|g" \
    cloud-init-train.yaml > "$RENDERED"

SSH_KEY_ID="$(doctl compute ssh-key list --format ID,Name --no-header | awk -v n="$SSH_KEY_NAME" '$2==n{print $1}')"
[[ -n "$SSH_KEY_ID" ]] || { echo "ERROR: SSH key '$SSH_KEY_NAME' not found."; exit 1; }

doctl compute droplet create "$DROPLET_NAME" \
  --region "$REGION" --size "$SIZE" --image "$IMAGE" \
  --ssh-keys "$SSH_KEY_ID" --tag-name "$TAG" \
  --user-data-file "$RENDERED" --wait

IP="$(doctl compute droplet get "$DROPLET_NAME" --format PublicIPv4 --no-header)"
cat > .secrets/train.env <<EOF
KAI_TRAIN_IP=$IP
KAI_TRAIN_DROPLET=$DROPLET_NAME
EOF
echo "======================================================================"
echo "  Training Droplet at $IP. cloud-init builds Kai, the desktop and the"
echo "  venv (~20-30 min); done when /root/KAI/train/.ready exists:"
echo "    ssh root@$IP 'test -f /root/KAI/train/.ready && echo ready'"
echo "  Then:  ssh root@$IP;  cd /root/KAI;  source ~/.kai-train.env"
echo "         train/.venv-gpu/bin/python train/sft_gemma.py --config train/configs/sft_26b.yaml --data <sft.jsonl>"
echo "  Copy data up with:  scp sft.jsonl root@$IP:/root/KAI/train/  (frames referenced by"
echo "  absolute paths must exist there too: export with relative paths or rsync bench/runs)."
echo "  Destroy:  deploy/digitalocean/train-down.sh"
echo "======================================================================"
