#!/bin/sh
# Start the local bench web server once (idempotent). Used by task setup.
dir="$(cd "$(dirname "$0")" && pwd)"
port="${KAI_BENCH_PORT:-8765}"
curl -sf "http://127.0.0.1:$port/ping" >/dev/null 2>&1 && exit 0
setsid python3 "$dir/server.py" --port "$port" >/dev/null 2>&1 </dev/null &
i=0
while [ $i -lt 50 ]; do
  curl -sf "http://127.0.0.1:$port/ping" >/dev/null 2>&1 && exit 0
  sleep 0.1
  i=$((i + 1))
done
echo "bench server did not start on port $port" >&2
exit 1
