#!/usr/bin/env bash
# Send a real screenshot and ask for an action in Kai's schema. Measure latency.
# Run N times (default 20) and print p50/p95 for first-token and full-reply.
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$here"

N="${1:-20}"
[[ -f .secrets/endpoint.env ]] && source .secrets/endpoint.env
: "${KAI_ENDPOINT:?Run up.sh first (or set KAI_ENDPOINT and VLLM_API_KEY).}"
: "${VLLM_API_KEY:?missing vLLM API key}"

SHOT="${KAI_SHOT:-$here/fixtures/screen.jpg}"
if [[ ! -f "$SHOT" ]]; then
  echo "No screenshot at $SHOT. Capture one and set KAI_SHOT, or use fixtures/screen.jpg."
  exit 1
fi
IMG="data:image/jpeg;base64,$(base64 -w0 "$SHOT" 2>/dev/null || base64 "$SHOT" | tr -d '\n')"

SCHEMA='Reply ONLY with JSON: {"thought":"<=25 words","action":"click|type|key|scroll|open_app|open_url|wait|ask_user|done","target":<int or null>,"text":"","keys":[],"direction":"down","amount":1,"risk":"none|low|high","expect":""}'

body() {
cat <<JSON
{"model":"gemma-4-26b-a4b-it","max_tokens":128,"temperature":0,
 "messages":[
   {"role":"system","content":"You are Kai. Choose ONE next UI action. $SCHEMA"},
   {"role":"user","content":[
     {"type":"text","text":"#1 button \"Search\" (enabled)\nTask: open a search box."},
     {"type":"image_url","image_url":{"url":"$IMG"}}]}]}
JSON
}

echo "Smoke test: $N calls to $KAI_ENDPOINT"
times=()
for i in $(seq 1 "$N"); do
  t=$( { /usr/bin/time -f '%e' curl -s -o /dev/null \
      -H "Authorization: Bearer ${VLLM_API_KEY}" -H 'Content-Type: application/json' \
      --max-time 60 -d "$(body)" "$KAI_ENDPOINT/chat/completions"; } 2>&1 )
  echo "  call $i: ${t}s"
  times+=("$t")
done

printf '%s\n' "${times[@]}" | sort -n | awk '
{ a[NR]=$1 }
END {
  p50=a[int(NR*0.5)+ (NR*0.5==int(NR*0.5)?0:1)];
  p95=a[int(NR*0.95)+ (NR*0.95==int(NR*0.95)?0:1)];
  printf "\nfull-reply latency  p50=%.2fs  p95=%.2fs  (n=%d)\n", p50, p95, NR
}'
echo "Record these in DECISIONS.md. (First-token latency needs streaming; see README.)"
