#!/bin/bash
# Rebuild every derived Kai icon asset from the master render. Run on macOS
# (sips, iconutil, swiftc). Outputs, all under assets/icons/:
#   Kai.iconset/ + Kai.icns   the macOS bundle icon
#   Kai.ico                   the Windows executable icon (build.rs embeds it)
#   kai-app-512.png           the runtime window icon and docs
#   kai-mark.png              the in-app mark (transparent ribbon, 512 px)
#
#   scripts/icons/build.sh [master.png]   default: assets/icons/kai-app-v3.png
set -euo pipefail

script_dir=$(cd "$(dirname "$0")" && pwd)
cd "$script_dir/../.."
icons=assets/icons
master=${1:-$icons/kai-app-v3.png}

[[ -f "$master" ]] || { echo "no master render at $master" >&2; exit 1; }
for tool in sips iconutil swiftc python3; do
    command -v "$tool" >/dev/null || { echo "missing $tool (this needs macOS)" >&2; exit 1; }
done

# macOS iconset and icns.
rm -rf "$icons/Kai.iconset"
mkdir -p "$icons/Kai.iconset"
for s in 16 32 128 256 512; do
    sips -z $s $s "$master" --out "$icons/Kai.iconset/icon_${s}x${s}.png" >/dev/null
    d=$((s * 2))
    sips -z $d $d "$master" --out "$icons/Kai.iconset/icon_${s}x${s}@2x.png" >/dev/null
done
iconutil -c icns "$icons/Kai.iconset" -o "$icons/Kai.icns"

# 512 px full icon (runtime window icon; docs and README).
sips -z 512 512 "$master" --out "$icons/kai-app-512.png" >/dev/null

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

# Windows .ico: PNG-compressed entries, as Vista and later accept.
for s in 16 32 48 64 128 256; do
    sips -z $s $s "$master" --out "$tmp/icon_${s}.png" >/dev/null
done
python3 - "$tmp" "$icons/Kai.ico" <<'PY'
import struct, sys
tmp, out = sys.argv[1], sys.argv[2]
sizes = [16, 32, 48, 64, 128, 256]
blobs = [open(f"{tmp}/icon_{s}.png", "rb").read() for s in sizes]
header = struct.pack("<HHH", 0, 1, len(blobs))
entries, offset = b"", 6 + 16 * len(blobs)
for s, b in zip(sizes, blobs):
    wh = 0 if s == 256 else s
    entries += struct.pack("<BBBBHHII", wh, wh, 0, 0, 1, 32, len(b), offset)
    offset += len(b)
open(out, "wb").write(header + entries + b"".join(blobs))
PY

# In-app mark: the cobalt ribbon lifted off the tile (see extract-mark.swift).
swiftc -O "$script_dir/extract-mark.swift" -o "$tmp/extract-mark"
"$tmp/extract-mark" "$master" "$tmp/kai-mark-full.png" 8
sips --resampleWidth 512 "$tmp/kai-mark-full.png" --out "$icons/kai-mark.png" >/dev/null

echo "rebuilt icons in $icons from $master"
