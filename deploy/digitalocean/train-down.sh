#!/usr/bin/env bash
# Destroy the TRAINING Droplet created by train-up.sh. Only destroying it stops
# billing. Never touches the serving Droplet (kai-gemma) or its endpoint.env.
# Copy anything you want to keep first, e.g.:
#   rsync -a root@$KAI_TRAIN_IP:/root/KAI/train/out/ train/out/
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$here"

NAME="${KAI_TRAIN_DROPLET:-}"
if [[ -z "$NAME" && -f .secrets/train.env ]]; then
  NAME="$(sed -n 's/^KAI_TRAIN_DROPLET=//p' .secrets/train.env)"
fi
NAME="${NAME:-kai-train}"
[[ "$NAME" != "kai-gemma" ]] || { echo "Refusing: '$NAME' is the serving Droplet; use down.sh."; exit 1; }

command -v doctl >/dev/null || { echo "doctl not found."; exit 1; }
if doctl compute droplet list --format Name --no-header | grep -qx "$NAME"; then
  echo "Destroying training Droplet '$NAME'…"
  doctl compute droplet delete "$NAME" --force
  echo "Destroyed. Billing for it has stopped."
else
  echo "No Droplet named '$NAME'. Nothing to destroy."
fi
rm -f .secrets/train.env
echo "The kai-train-fw firewall is left in place (free): doctl compute firewall delete kai-train-fw --force"
