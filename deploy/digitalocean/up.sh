#!/usr/bin/env bash
# Create one GPU Droplet serving Gemma 4 via vLLM, behind Caddy TLS.
# One command, repeatable, nothing done by hand in the console.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$here"

# --- Config (override via env) -------------------------------------------------
REGION="${KAI_REGION:-nyc2}"              # H100 regions: nyc2, ams3, tor1
SIZE="${KAI_SIZE:-gpu-h100x1-80gb}"       # 1x H100 80GB; confirmed against the API below
DROPLET_NAME="${KAI_DROPLET:-kai-gemma}"
TAG="kai"
HOSTNAME_MODE="${KAI_HOSTNAME_MODE:-sslip}"  # sslip | <your.domain>
SSH_KEY_NAME="${KAI_SSH_KEY:-}"           # name of an SSH key already in your DO account
IMAGE="${KAI_IMAGE:-ubuntu-24-04-x64}"

# --- Preconditions -------------------------------------------------------------
command -v doctl >/dev/null || { echo "ERROR: doctl not found. See README.md"; exit 1; }
doctl account get >/dev/null 2>&1 || { echo "ERROR: 'doctl auth init' first."; exit 1; }
: "${HF_TOKEN:?Set HF_TOKEN (accept Gemma's license on Hugging Face first). Never commit it.}"

if [[ -z "$SSH_KEY_NAME" ]]; then
  echo "ERROR: set KAI_SSH_KEY to the name of an SSH key in your DO account (doctl compute ssh-key list)."
  exit 1
fi

# --- Verify the region actually offers the GPU size (fail loud, not silent) ----
echo "Checking region/size availability…"
if ! doctl compute region list --format Slug --no-header | grep -qx "$REGION"; then
  echo "ERROR: region '$REGION' not found. Available:"; doctl compute region list --format Slug,Name --no-header
  exit 1
fi
if ! doctl compute size list --format Slug --no-header | grep -qx "$SIZE"; then
  echo "WARNING: size slug '$SIZE' not in the self-service sizes API."
  echo "Per-contract GPU plans may not appear there. Sizes that do:"
  doctl compute size list --format Slug,Memory,VCPUs,Price_Hourly --no-header | grep -i gpu || true
  echo "Set KAI_SIZE to a listed GPU slug, or confirm your contract slug in the control panel, then re-run."
  exit 1
fi

PRICE_HR="$(doctl compute size list --format Slug,Price_Hourly --no-header | awk -v s="$SIZE" '$1==s{print $2}')"
echo "======================================================================"
echo "  Creating GPU Droplet: $DROPLET_NAME ($SIZE) in $REGION"
echo "  Hourly cost: \$${PRICE_HR:-~3.39}/hr — BILLED WHILE IT EXISTS, EVEN IDLE."
echo "  Run deploy/digitalocean/down.sh when you are finished, or you keep paying."
echo "======================================================================"

# --- Generate the vLLM API key (never committed) -------------------------------
mkdir -p .secrets
if [[ ! -f .secrets/vllm_api_key ]]; then
  (openssl rand -hex 24 2>/dev/null || head -c 24 /dev/urandom | xxd -p) | tr -d '\n' > .secrets/vllm_api_key
fi
VLLM_API_KEY="$(cat .secrets/vllm_api_key)"
chmod 600 .secrets/vllm_api_key

# --- Firewall: 22 from caller IP only, 443 open --------------------------------
MY_IP="$(curl -fsS https://checkip.amazonaws.com | tr -d '\n')"
FW_NAME="kai-fw"
if ! doctl compute firewall list --format Name --no-header | grep -qx "$FW_NAME"; then
  doctl compute firewall create \
    --name "$FW_NAME" \
    --tag-names "$TAG" \
    --inbound-rules "protocol:tcp,ports:22,address:${MY_IP}/32 protocol:tcp,ports:443,address:0.0.0.0/0,address:::/0" \
    --outbound-rules "protocol:tcp,ports:all,address:0.0.0.0/0,address:::/0 protocol:udp,ports:all,address:0.0.0.0/0,address:::/0"
else
  echo "Firewall $FW_NAME exists; leaving as-is."
fi

# --- Render cloud-init with secrets substituted (kept out of git) --------------
RENDERED=".secrets/cloud-init.rendered.yaml"
sed -e "s|__HF_TOKEN__|${HF_TOKEN}|g" \
    -e "s|__VLLM_API_KEY__|${VLLM_API_KEY}|g" \
    -e "s|__HOSTNAME_MODE__|${HOSTNAME_MODE}|g" \
    cloud-init.yaml > "$RENDERED"
chmod 600 "$RENDERED"

# --- Create the Droplet --------------------------------------------------------
SSH_KEY_ID="$(doctl compute ssh-key list --format ID,Name --no-header | awk -v n="$SSH_KEY_NAME" '$2==n{print $1}')"
[[ -n "$SSH_KEY_ID" ]] || { echo "ERROR: SSH key '$SSH_KEY_NAME' not found."; exit 1; }

echo "Creating Droplet…"
doctl compute droplet create "$DROPLET_NAME" \
  --region "$REGION" --size "$SIZE" --image "$IMAGE" \
  --ssh-keys "$SSH_KEY_ID" --tag-name "$TAG" \
  --user-data-file "$RENDERED" \
  --wait

IP="$(doctl compute droplet get "$DROPLET_NAME" --format PublicIPv4 --no-header)"
if [[ "$HOSTNAME_MODE" == "sslip" ]]; then
  HOST="${IP//./-}.sslip.io"
else
  HOST="$HOSTNAME_MODE"
  echo "NOTE: point an A record for $HOST at $IP for Caddy to get a cert."
fi

echo "======================================================================"
echo "  Droplet up at $IP"
echo "  Model endpoint will be: https://$HOST/v1"
echo "  vLLM API key (also in .secrets/vllm_api_key):"
echo "      $VLLM_API_KEY"
echo "  cloud-init is still installing vLLM + downloading Gemma (several minutes)."
echo "  Watch readiness:  curl -s https://$HOST/health"
echo "  When done:        deploy/digitalocean/down.sh"
echo "======================================================================"

# Persist connection details for smoke.sh / down.sh
cat > .secrets/endpoint.env <<EOF
KAI_ENDPOINT=https://$HOST/v1
KAI_HOST=$HOST
KAI_IP=$IP
KAI_DROPLET=$DROPLET_NAME
VLLM_API_KEY=$VLLM_API_KEY
EOF
chmod 600 .secrets/endpoint.env
