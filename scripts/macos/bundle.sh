#!/bin/bash
# Build Kai.app for Apple Silicon and zip it: dist/Kai.app and
# dist/kai-macos-aarch64.zip (+ .sha256). Run on a Mac with Rust installed
# (the Command Line Tools are enough; full Xcode is not needed).
#
#   scripts/macos/bundle.sh            build, bundle, sign ad hoc, zip
#   scripts/macos/bundle.sh --install  also copy Kai.app to /Applications
#
# The app is signed ad hoc (no Apple developer certificate). macOS ties the
# Screen Recording and Accessibility permissions to that signature, so after
# a rebuild they may need to be turned off and on again in System Settings.
# docs/MACOS.md has the details.
set -euo pipefail

cd "$(dirname "$0")/../.."
[[ "$(uname -s)" == "Darwin" ]] || { echo "run this on a Mac" >&2; exit 1; }
[[ "$(uname -m)" == "arm64" ]] || { echo "Kai's Mac build is for Apple Silicon" >&2; exit 1; }

version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
dist=dist
app="$dist/Kai.app"

# --skip-checks prepares a build for a later GUI testing session without
# executing the bundled CLI or any validation task.
skip_checks=false
install=false
for arg in "$@"; do
    case "$arg" in
        --skip-checks) skip_checks=true ;;
        --install) install=true ;;
        *) echo "unknown option: $arg" >&2; exit 2 ;;
    esac
done
cargo build --release -p kai-app -p kai-cli

rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp target/release/kai-app target/release/kai "$app/Contents/MacOS/"
cp -R skills LICENSE README.md "$app/Contents/Resources/"
# Package repository fixtures/reports only. Raw local runs and captures are private.
git ls-files -z --cached --others --exclude-standard -- bench |     tar --null -T - -cf - | tar -xf - -C "$app/Contents/Resources/"

# The finished app icon: the cobalt orbit. `assets/icons/Kai.icns` is rebuilt
# from the master render by `scripts/icons/build.sh`; the same ribbon is the
# in-app mark (docs/APP_ICON.md).
cp assets/icons/Kai.icns "$app/Contents/Resources/Kai.icns"
icon_key="<key>CFBundleIconFile</key><string>Kai</string>"

cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>Kai</string>
  <key>CFBundleDisplayName</key><string>Kai</string>
  <key>CFBundleIdentifier</key><string>io.github.shashank03-dev.kai</string>
  <key>CFBundleExecutable</key><string>kai-app</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>${version}</string>
  <key>CFBundleVersion</key><string>${version}</string>
  <key>LSMinimumSystemVersion</key><string>13.0</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.productivity</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSSupportsAutomaticGraphicsSwitching</key><true/>
  ${icon_key}
</dict>
</plist>
PLIST

# Codex is Apache-2.0. Bundle an already-installed native helper when
# available so the GUI subscription connection needs no terminal setup.
# Credentials/configuration are never copied. Claude Code is not redistributed.
if command -v codex >/dev/null 2>&1; then
    helper=$(command -v codex)
    if file -L "$helper" | grep -q 'Mach-O'; then
        mkdir -p "$app/Contents/Resources/providers"
        cp -L "$helper" "$app/Contents/Resources/providers/codex"
        cp assets/licenses/codex-LICENSE.txt "$app/Contents/Resources/providers/LICENSE-codex.txt"
        codesign --force --sign - --timestamp=none "$app/Contents/Resources/providers/codex"
    fi
fi

# Inner binaries first, then the bundle (a plain --deep is deprecated).
codesign --force --sign - --timestamp=none "$app/Contents/MacOS/kai"
codesign --force --sign - --timestamp=none "$app/Contents/MacOS/kai-app"
codesign --force --sign - --timestamp=none "$app"
codesign --verify --strict "$app"

# Smoke test the bundled CLI the way the release workflow does.
if ! "$skip_checks"; then
    "$app/Contents/MacOS/kai" skill validate "$app/Contents/Resources/skills"
    "$app/Contents/MacOS/kai" models list >/dev/null
fi

zip="$dist/kai-macos-aarch64.zip"
rm -f "$zip"
ditto -c -k --keepParent "$app" "$zip"
(cd "$dist" && shasum -a 256 "$(basename "$zip")" > "$(basename "$zip").sha256")
echo "built $app and $zip"

if "$install"; then
    rm -rf /Applications/Kai.app
    cp -R "$app" /Applications/
    echo "installed /Applications/Kai.app"
fi
