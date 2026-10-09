#!/usr/bin/env bash
# Install the headless desktop Kai is tested on (Ubuntu 24.04, run as root):
# Xvfb + openbox + the AT-SPI accessibility bus + a few real apps, and a
# non-snap Firefox ESR from Mozilla into /opt/firefox (Ubuntu's firefox and
# chromium packages are snaps, which do not run in containers).
#
#   sudo scripts/desktop/install.sh            # apt packages + Firefox
#   KAI_SKIP_FIREFOX=1 scripts/desktop/install.sh
set -euo pipefail

SUDO=""
[ "$(id -u)" -eq 0 ] || SUDO="sudo"

export DEBIAN_FRONTEND=noninteractive
$SUDO apt-get update -q
$SUDO apt-get install -y -q --no-install-recommends \
  xvfb openbox xdotool wmctrl x11-utils imagemagick tigervnc-standalone-server \
  dbus-x11 at-spi2-core libatk-bridge2.0-0t64 libgtk-3-0t64 \
  mousepad pcmanfm libreoffice-calc libreoffice-gtk3 gtk-3-examples zenity \
  xz-utils curl ca-certificates \
  libasound2t64 libx11-xcb1 libdbus-glib-1-2 libpci3 || {
    # Package names differ slightly between releases; retry the essentials.
    $SUDO apt-get install -y -q --no-install-recommends \
      xvfb openbox xdotool wmctrl x11-utils dbus-x11 at-spi2-core \
      mousepad pcmanfm libreoffice-calc xz-utils curl ca-certificates
  }

if [ "${KAI_SKIP_FIREFOX:-0}" != "1" ] && [ ! -x /opt/firefox/firefox ]; then
  # Mozilla's redirect always points at the newest ESR tarball (linux x86_64).
  url="https://download.mozilla.org/?product=firefox-esr-latest-ssl&os=linux64&lang=en-US"
  tmp="$(mktemp -d)"
  curl -fsSL -o "$tmp/firefox.tar.xz" "$url"
  $SUDO tar -xJf "$tmp/firefox.tar.xz" -C /opt
  rm -rf "$tmp"
  $SUDO ln -sf /opt/firefox/firefox /usr/local/bin/firefox
fi

# Firefox only exposes its tree over AT-SPI when told to. A policy file makes
# that hold for every profile. It also trusts the system CA store (p11-kit), so
# a TLS-inspecting proxy whose CA the system trusts doesn't break every page.
if [ -d /opt/firefox ]; then
  $SUDO mkdir -p /opt/firefox/distribution
  $SUDO tee /opt/firefox/distribution/policies.json >/dev/null <<'JSON'
{
  "policies": {
    "DisableAppUpdate": true,
    "DontCheckDefaultBrowser": true,
    "OverrideFirstRunPage": "",
    "SecurityDevices": {
      "Add": { "System trust": "/usr/lib/x86_64-linux-gnu/pkcs11/p11-kit-trust.so" }
    },
    "Preferences": {
      "accessibility.force_disabled": { "Value": -1, "Status": "locked" }
    }
  }
}
JSON
fi

echo "desktop stack installed; start it with: source scripts/desktop/start.sh"
