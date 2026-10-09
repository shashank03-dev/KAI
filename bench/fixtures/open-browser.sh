#!/bin/sh
# Open URL in the first browser found (Firefox on PATH or in /opt/firefox,
# then Chromium/Chrome), detached. Used by task setup.
url="$1"
for b in firefox /opt/firefox/firefox chromium chromium-browser google-chrome; do
  if command -v "$b" >/dev/null 2>&1 || [ -x "$b" ]; then
    setsid -f "$b" "$url" >/dev/null 2>&1 </dev/null
    i=0
    while [ $i -lt 60 ]; do
      wmctrl -l 2>/dev/null | grep -qiE 'firefox|chrom|mozilla|kai bench' && exit 0
      sleep 0.25
      i=$((i + 1))
    done
    exit 0
  fi
done
echo "no browser found" >&2
exit 1
