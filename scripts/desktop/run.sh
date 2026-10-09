#!/usr/bin/env bash
# Run one command inside the headless desktop (starting it if needed):
#   scripts/desktop/run.sh cargo run -p kai-cli -- doctor
#   scripts/desktop/run.sh --bg mousepad      # detach an app
set -e
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
dir="${KAI_DESKTOP_DIR:-/tmp/kai-desktop}"
"$here/start.sh" >/dev/null
# shellcheck disable=SC1091
. "$dir/env"
if [ "${1:-}" = "--bg" ]; then
  shift
  setsid "$@" >"$dir/$(basename "$1").log" 2>&1 < /dev/null &
  exit 0
fi
exec "$@"
