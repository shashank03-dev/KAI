#!/bin/sh
# Start every browser task from a clean browser: close Firefox and drop its
# saved session, so old tabs and "tab crashed" pages never come back.
# `kai bench --browser` starts every task in a fresh profile instead.
[ "$KAI_BENCH_LANE" = 1 ] && exit 0
pkill -x firefox >/dev/null 2>&1
pkill -x firefox-bin >/dev/null 2>&1
i=0
while pgrep -x firefox >/dev/null 2>&1 && [ $i -lt 20 ]; do sleep 0.25; i=$((i + 1)); done
pkill -9 -x firefox >/dev/null 2>&1
for p in "$HOME"/.mozilla/firefox/*/; do
  rm -rf "$p"sessionstore.jsonlz4 "$p"sessionstore-backups "$p"sessionCheckpoints.json 2>/dev/null
done
true
