#!/bin/sh
# Start every browser task from a clean browser: close Firefox and drop its
# saved session, so old tabs and "tab crashed" pages never come back. Also
# used as a teardown, so no Firefox window or dialog is left on top of the
# next task.
pkill -x firefox >/dev/null 2>&1
pkill -x firefox-bin >/dev/null 2>&1
i=0
while pgrep -x firefox >/dev/null 2>&1 && [ $i -lt 20 ]; do sleep 0.25; i=$((i + 1)); done
pkill -9 -x firefox >/dev/null 2>&1
pkill -9 -f '/firefox/' >/dev/null 2>&1
for p in "$HOME"/.mozilla/firefox/*/; do
  # A killed Firefox leaves its profile locked ("Firefox is already
  # running") and counts a crash ("Open Firefox in Troubleshoot Mode?").
  rm -rf "$p"sessionstore.jsonlz4 "$p"sessionstore-backups "$p"sessionCheckpoints.json \
    "$p"lock "$p".parentlock 2>/dev/null
  [ -f "$p"prefs.js ] && sed -i '/toolkit.startup.recent_crashes/d' "$p"prefs.js
done
true
