#!/bin/bash
# Build Kai.app for Apple Silicon and zip it: dist/Kai.app and
# dist/kai-macos-aarch64.zip (+ .sha256). Run on a Mac with Rust installed
# (the Command Line Tools are enough; full Xcode is not needed).
#
#   scripts/macos/bundle.sh                build, bundle, sign, zip
#   scripts/macos/bundle.sh --install      also copy Kai.app to /Applications
#   scripts/macos/bundle.sh --skip-checks  skip the bundled CLI smoke checks
#
# Signing: with KAI_SIGN_IDENTITY set to a "Developer ID Application: …"
# identity in the keychain, Kai is signed with it (hardened runtime, secure
# timestamp). Without it the app is signed ad hoc: it runs on the Mac that
# built it, and elsewhere macOS asks before the first open. Notarization is
# scripts/macos/package_dmg.sh's job. docs/MACOS.md has the details.
#
# KAI_BUNDLE_CODEX=0 leaves out the Codex helper (Apache-2.0) that is
# otherwise copied, with OpenAI's own signature untouched, from an installed
# Codex. Its credentials and configuration are never copied.
set -euo pipefail

cd "$(dirname "$0")/../.."
[[ "$(uname -s)" == "Darwin" ]] || { echo "run this on a Mac" >&2; exit 1; }
[[ "$(uname -m)" == "arm64" ]] || { echo "Kai's Mac build is for Apple Silicon" >&2; exit 1; }

version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
dist=dist
app="$dist/Kai.app"
res="$app/Contents/Resources"
identity="${KAI_SIGN_IDENTITY:--}"

skip_checks=false
install=false
for arg in "$@"; do
    case "$arg" in
        --skip-checks) skip_checks=true ;;
        --install) install=true ;;
        *) echo "unknown option: $arg" >&2; exit 2 ;;
    esac
done
cargo build --release --locked -p kai-app -p kai-cli

rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$res/licenses"
cp target/release/kai-app target/release/kai "$app/Contents/MacOS/"
cp -R skills README.md "$res/"
cp LICENSE "$res/LICENSE"
# Benchmark tasks and fixtures only: result files are raw runs and stay out,
# as do untracked local captures and traces.
git ls-files -z --cached -- bench ':!:bench/results' | tar --null -T - -cf - | tar -xf - -C "$res/"

# Licenses of everything shipped inside the app.
python3 -I scripts/macos/third_party_licenses.py "$res/licenses/THIRD_PARTY_LICENSES.txt"
ely=vendor/ely-gpui-component
cp "$ely/LICENSE" "$res/licenses/ely-gpui-component-LICENSE.txt"
cp "$ely/assets/icons/LICENSE" "$res/licenses/lucide-icons-LICENSE.txt"
cp "$ely/assets/fonts/Inter-LICENSE.txt" "$ely/assets/fonts/JetBrainsMono-LICENSE.txt" \
   "$ely/assets/fonts/ibm-plex-sans/IBMPlexSans-LICENSE.txt" "$res/licenses/"

# The finished app icon: the cobalt orbit (docs/APP_ICON.md).
cp assets/icons/Kai.icns "$res/Kai.icns"

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
  <key>LSArchitecturePriority</key><array><string>arm64</string></array>
  <key>LSApplicationCategoryType</key><string>public.app-category.productivity</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSSupportsAutomaticGraphicsSwitching</key><true/>
  <key>CFBundleIconFile</key><string>Kai</string>
</dict>
</plist>
PLIST

# Codex is shipped only with a valid signature of its own (OpenAI's
# Developer ID): re-signing it would replace the publisher's identity.
if [[ "${KAI_BUNDLE_CODEX:-1}" != 0 ]] && command -v codex >/dev/null 2>&1; then
    helper=$(python3 -I -c 'import os,sys; print(os.path.realpath(sys.argv[1]))' "$(command -v codex)")
    if lipo -archs "$helper" 2>/dev/null | grep -qw arm64 && codesign --verify --strict "$helper" 2>/dev/null; then
        mkdir -p "$res/providers"
        cp "$helper" "$res/providers/codex"
        cp assets/licenses/codex-LICENSE.txt "$res/licenses/codex-LICENSE.txt"
        codex_version=$("$helper" --version 2>&1 </dev/null || true)
        echo "${codex_version:-unknown} (copied from $helper)" > "$res/providers/VERSION-codex.txt"
    else
        echo "warning: $helper is not a signed arm64 binary; Codex is not bundled" >&2
    fi
fi

sign() {
    if [[ "$identity" == "-" ]]; then
        codesign --force --sign - --timestamp=none "$@"
    else
        codesign --force --sign "$identity" --options runtime --timestamp "$@"
    fi
}
# Inner binaries first, then the bundle (a plain --deep is deprecated).
sign "$app/Contents/MacOS/kai"
sign "$app/Contents/MacOS/kai-app"
sign "$app"
codesign --verify --strict --deep "$app"

if ! "$skip_checks"; then
    "$app/Contents/MacOS/kai" skill validate "$res/skills"
    "$app/Contents/MacOS/kai" models list >/dev/null
fi

zip="$dist/kai-macos-aarch64.zip"
rm -f "$zip"
ditto -c -k --keepParent "$app" "$zip"
(cd "$dist" && shasum -a 256 "$(basename "$zip")" > "$(basename "$zip").sha256")
echo "built $app and $zip (signed: ${identity/#-/ad hoc})"

if "$install"; then
    rm -rf /Applications/Kai.app
    cp -R "$app" /Applications/
    echo "installed /Applications/Kai.app"
fi
