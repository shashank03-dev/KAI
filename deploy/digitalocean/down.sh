#!/usr/bin/env bash
# Destroy the GPU Droplet. Only destroying it stops billing.
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$here"

DROPLET_NAME="${KAI_DROPLET:-kai-gemma}"
[[ -f .secrets/endpoint.env ]] && source .secrets/endpoint.env || true
DROPLET_NAME="${KAI_DROPLET:-$DROPLET_NAME}"

command -v doctl >/dev/null || { echo "doctl not found."; exit 1; }

if doctl compute droplet list --format Name --no-header | grep -qx "$DROPLET_NAME"; then
  echo "Destroying Droplet '$DROPLET_NAME'…"
  doctl compute droplet delete "$DROPLET_NAME" --force
  echo "Destroyed. Billing for the GPU has stopped."
else
  echo "No Droplet named '$DROPLET_NAME'. Nothing to destroy."
fi

echo "Note: the kai-fw firewall and your SSH key are left in place (free)."
echo "Remove the firewall with: doctl compute firewall delete kai-fw --force"
rm -f .secrets/endpoint.env
