#!/usr/bin/env bash
# Start (or reuse) a headless desktop for Kai: Xvfb :99 at 1920x1080x24, a
# D-Bus session bus, the AT-SPI accessibility bus, and openbox. Idempotent:
# running it again reuses whatever is still alive.
#
#   source scripts/desktop/start.sh     # starts it and exports the env here
#   scripts/desktop/start.sh            # starts it; then: source /tmp/kai-desktop/env
#
# Overrides: KAI_DISPLAY (default :99), KAI_SCREEN (1920x1080x24),
# KAI_DESKTOP_DIR (state dir, default /tmp/kai-desktop).

_kai_start() {
  local disp="${KAI_DISPLAY:-:99}"
  local screen="${KAI_SCREEN:-1920x1080x24}"
  local dir="${KAI_DESKTOP_DIR:-/tmp/kai-desktop}"
  local num="${disp#:}"
  mkdir -p "$dir/xdg" && chmod 700 "$dir/xdg"

  _alive() { [ -n "${1:-}" ] && kill -0 "$1" 2>/dev/null; }
  _pid() { cat "$dir/$1.pid" 2>/dev/null; }

  # 1. X server.
  if ! xdpyinfo -display "$disp" >/dev/null 2>&1; then
    rm -f "/tmp/.X${num}-lock" "/tmp/.X11-unix/X${num}"
    Xvfb "$disp" -screen 0 "$screen" -nolisten tcp -ac +extension RANDR \
      >"$dir/xvfb.log" 2>&1 &
    echo $! >"$dir/xvfb.pid"
    local i
    for i in $(seq 1 50); do
      xdpyinfo -display "$disp" >/dev/null 2>&1 && break
      sleep 0.1
    done
    xdpyinfo -display "$disp" >/dev/null 2>&1 || {
      echo "Xvfb did not start; see $dir/xvfb.log" >&2
      return 1
    }
  fi
  export DISPLAY="$disp"

  # 2. Session bus (kept across runs through $dir/dbus.env).
  if [ -f "$dir/dbus.env" ]; then
    # shellcheck disable=SC1091
    . "$dir/dbus.env"
  fi
  if ! _alive "${DBUS_SESSION_BUS_PID:-}"; then
    dbus-launch --sh-syntax | grep -v '^#' >"$dir/dbus.env"
    # shellcheck disable=SC1091
    . "$dir/dbus.env"
  fi
  export DBUS_SESSION_BUS_ADDRESS DBUS_SESSION_BUS_PID

  # 3. Accessibility env for every toolkit, set before any app starts.
  export XDG_RUNTIME_DIR="$dir/xdg"
  export NO_AT_BRIDGE=0
  export GTK_MODULES="gail:atk-bridge"
  export ACCESSIBILITY_ENABLED=1
  export GNOME_ACCESSIBILITY=1
  export QT_ACCESSIBILITY=1
  export QT_LINUX_ACCESSIBILITY_ALWAYS_ON=1
  export SAL_USE_VCLPLUGIN=gtk3
  export MOZ_ENABLE_WAYLAND=0
  # A killed Firefox must not ask "Open in Troubleshoot Mode?" next time.
  export MOZ_DISABLE_AUTO_SAFE_MODE=1
  # Web links go to Firefox (the container has no default-browser setting).
  if [ -z "${BROWSER:-}" ] && command -v firefox >/dev/null 2>&1; then
    export BROWSER=firefox
  fi

  # Folders open in the file manager (`xdg-open ~/dir`), as on a desktop.
  if command -v xdg-mime >/dev/null 2>&1 && command -v pcmanfm >/dev/null 2>&1; then
    xdg-mime default pcmanfm.desktop inode/directory >/dev/null 2>&1 || true
  fi

  # 4. AT-SPI bus launcher (it starts the a11y bus and the registry daemon).
  if ! _alive "$(_pid atspi)"; then
    /usr/libexec/at-spi-bus-launcher --launch-immediately \
      >"$dir/atspi.log" 2>&1 &
    echo $! >"$dir/atspi.pid"
    sleep 0.3
  fi
  dbus-send --session --type=method_call --dest=org.a11y.Bus /org/a11y/bus \
    org.freedesktop.DBus.Properties.Set string:org.a11y.Status \
    string:IsEnabled variant:boolean:true >/dev/null 2>&1 || true
  # at-spi2-registryd is D-Bus-activated on that a11y bus on first use.
  gsettings set org.gnome.desktop.interface toolkit-accessibility true \
    >/dev/null 2>&1 || true

  # 5. Window manager (focus, stacking, the "active" state AT-SPI reports).
  if ! _alive "$(_pid openbox)"; then
    openbox --sm-disable >"$dir/openbox.log" 2>&1 &
    echo $! >"$dir/openbox.pid"
    sleep 0.2
  fi

  # Everything a later shell needs.
  {
    echo "export DISPLAY=$DISPLAY"
    echo "export DBUS_SESSION_BUS_ADDRESS='$DBUS_SESSION_BUS_ADDRESS'"
    echo "export DBUS_SESSION_BUS_PID=$DBUS_SESSION_BUS_PID"
    echo "export XDG_RUNTIME_DIR=$XDG_RUNTIME_DIR"
    echo "export NO_AT_BRIDGE=0 GTK_MODULES=gail:atk-bridge ACCESSIBILITY_ENABLED=1"
    echo "export GNOME_ACCESSIBILITY=1 QT_ACCESSIBILITY=1 QT_LINUX_ACCESSIBILITY_ALWAYS_ON=1"
    echo "export SAL_USE_VCLPLUGIN=gtk3 MOZ_ENABLE_WAYLAND=0 MOZ_DISABLE_AUTO_SAFE_MODE=1"
    [ -n "${BROWSER:-}" ] && echo "export BROWSER=$BROWSER"
  } >"$dir/env"
  echo "Kai desktop up on $DISPLAY (env: $dir/env)"
}

_kai_start "$@"
_kai_rc=$?
# When executed rather than sourced, exit with the status.
if [ "${BASH_SOURCE[0]}" = "$0" ]; then
  exit $_kai_rc
fi
unset -f _kai_start
return $_kai_rc 2>/dev/null || true
