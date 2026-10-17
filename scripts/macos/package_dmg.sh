#!/bin/bash
# Build dist/Kai.dmg: Kai.app next to an Applications shortcut, for
# drag-and-drop install. Runs scripts/macos/bundle.sh first, then checks the
# bundle and the image before declaring success.
#
#   scripts/macos/package_dmg.sh                build, check, package
#   scripts/macos/package_dmg.sh --skip-build   package the existing dist/Kai.app
#
# Signing and notarization use only what is configured:
#   KAI_SIGN_IDENTITY     "Developer ID Application: …" in the keychain
#   KAI_NOTARY_PROFILE    a `xcrun notarytool store-credentials` profile
# Without them the image is ad hoc signed and NOT notarized, and this script
# says so. It never changes Gatekeeper or any other macOS protection.
set -euo pipefail

cd "$(dirname "$0")/../.."
dist=dist
app="$dist/Kai.app"
dmg="$dist/Kai.dmg"
identity="${KAI_SIGN_IDENTITY:--}"
notary="${KAI_NOTARY_PROFILE:-}"

build=true
for arg in "$@"; do
    case "$arg" in
        --skip-build) build=false ;;
        *) echo "unknown option: $arg" >&2; exit 2 ;;
    esac
done
"$build" && scripts/macos/bundle.sh
[[ -d "$app" ]] || { echo "no $app: run without --skip-build" >&2; exit 1; }

fail() { echo "check failed: $*" >&2; exit 1; }

# The bundle: structure, architecture, signature, and nothing private.
check_bundle() {
    local a="$1"
    for f in Contents/Info.plist Contents/MacOS/kai-app Contents/MacOS/kai \
             Contents/Resources/Kai.icns Contents/Resources/skills \
             Contents/Resources/LICENSE Contents/Resources/licenses/THIRD_PARTY_LICENSES.txt; do
        [[ -e "$a/$f" ]] || fail "missing $f"
    done
    plutil -lint "$a/Contents/Info.plist" >/dev/null || fail "Info.plist is not valid"
    for bin in "$a/Contents/MacOS/kai-app" "$a/Contents/MacOS/kai"; do
        [[ "$(lipo -archs "$bin")" == arm64 ]] || fail "$bin is not arm64-only"
    done
    if [[ -e "$a/Contents/Resources/providers/codex" ]]; then
        [[ -e "$a/Contents/Resources/licenses/codex-LICENSE.txt" ]] || fail "Codex without its license"
        codesign --verify --strict "$a/Contents/Resources/providers/codex" || fail "Codex signature"
    fi
    codesign --verify --strict --deep "$a" || fail "app signature"
    local private
    private=$(find "$a" \( -name '*.env' -o -name '.env*' -o -name '*.pem' -o -name '*.key' \
        -o -name 'settings.json' -o -name '*.jsonl' -o -path '*/traces/*' -o -path '*/results/*' \
        -o -path '*/target/*' -o -name '*.rlib' -o -name '*.d' \) -print)
    [[ -z "$private" ]] || fail "private or build files in the bundle: $private"
    # Key-shaped strings in the text resources (binaries are checked by name only).
    if grep -rIlE 'sk-[A-Za-z0-9_-]{20,}|AIza[0-9A-Za-z_-]{35}|gh[pousr]_[A-Za-z0-9]{36}|-----BEGIN [A-Z ]*PRIVATE KEY' \
        "$a/Contents/Resources" --exclude=THIRD_PARTY_LICENSES.txt; then
        fail "key-shaped text in the bundle"
    fi
}
check_bundle "$app"

# A clean folder holding only what the image shows.
stage=$(mktemp -d "${TMPDIR:-/tmp}/kai-dmg.XXXXXX")
mount=$(mktemp -d "${TMPDIR:-/tmp}/kai-dmg-mount.XXXXXX")
cleanup() {
    hdiutil detach "$mount" -quiet 2>/dev/null || true
    rm -rf "$stage" "$mount"
}
trap cleanup EXIT
ditto "$app" "$stage/Kai.app"
ln -s /Applications "$stage/Applications"

rm -f "$dmg" "$dmg.sha256"
hdiutil create -volname Kai -srcfolder "$stage" -fs APFS -format UDZO -ov "$dmg" -quiet
hdiutil verify "$dmg" -quiet || fail "image checksum"

if [[ "$identity" == "-" ]]; then
    codesign --force --sign - --timestamp=none "$dmg"
else
    codesign --force --sign "$identity" --timestamp "$dmg"
fi
codesign --verify --strict "$dmg" || fail "image signature"

notarized=false
if [[ -n "$notary" && "$identity" != "-" ]]; then
    xcrun notarytool submit "$dmg" --keychain-profile "$notary" --wait
    xcrun stapler staple "$dmg"
    xcrun stapler validate "$dmg"
    notarized=true
elif [[ -n "$notary" ]]; then
    echo "KAI_NOTARY_PROFILE is set but KAI_SIGN_IDENTITY is not: ad hoc builds cannot be notarized" >&2
fi

# What a person gets: mount read-only and check the contents again.
hdiutil attach "$dmg" -readonly -nobrowse -noautoopen -mountpoint "$mount" -quiet
[[ -L "$mount/Applications" && "$(readlink "$mount/Applications")" == /Applications ]] \
    || fail "Applications shortcut"
check_bundle "$mount/Kai.app"
hdiutil detach "$mount" -quiet

(cd "$dist" && shasum -a 256 "$(basename "$dmg")" > "$(basename "$dmg").sha256")
echo "built $dmg ($(du -h "$dmg" | cut -f1))"
if "$notarized"; then
    echo "signed with: $identity; notarized and stapled"
else
    echo "signing: ${identity/#-/ad hoc}; NOT notarized."
    echo "Gatekeeper will ask before the first open on another Mac: Control-click Kai.app › Open."
fi
spctl --assess --type execute --verbose=2 "$app" 2>&1 | sed 's/^/gatekeeper assessment: /' || true
