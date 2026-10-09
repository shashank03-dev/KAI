#!/usr/bin/env bash
# Stop the headless desktop started by start.sh (and the apps on it).
dir="${KAI_DESKTOP_DIR:-/tmp/kai-desktop}"
disp="${KAI_DISPLAY:-:99}"

for name in openbox atspi xvfb; do
  pid="$(cat "$dir/$name.pid" 2>/dev/null)"
  [ -n "$pid" ] && kill "$pid" 2>/dev/null
  rm -f "$dir/$name.pid"
done
if [ -f "$dir/dbus.env" ]; then
  # shellcheck disable=SC1091
  . "$dir/dbus.env"
  [ -n "${DBUS_SESSION_BUS_PID:-}" ] && kill "$DBUS_SESSION_BUS_PID" 2>/dev/null
  rm -f "$dir/dbus.env"
fi
# An X server on our display that lost its pid file. Other desktops on the
# same machine (other displays, other a11y buses) are left alone; the a11y
# bus and its registry exit with our session bus.
pkill -f "Xvfb $disp " 2>/dev/null
rm -f "$dir/env"
echo "Kai desktop stopped"
