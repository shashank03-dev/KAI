#!/bin/sh
# Check a spreadsheet saved by a bench task.
#   check-sheet.sh FILE.ods 'REGEX' [formula]
# Converts FILE to CSV with a private headless LibreOffice profile (so it
# works while Calc is still open) and greps the CSV for REGEX. With a third
# argument "formula", also requires a SUM formula in the saved file.
set -e
file="$1"
want="$2"
[ -s "$file" ] || { echo "missing $file" >&2; exit 1; }
if [ "${3:-}" = formula ]; then
  unzip -p "$file" content.xml | grep -qi 'table:formula="of:=SUM' ||
    { echo "no SUM formula in $file" >&2; exit 1; }
fi
out="$(mktemp -d)"
trap 'rm -rf "$out"' EXIT
soffice -env:UserInstallation="file://$out/profile" --headless \
  --convert-to csv --outdir "$out" "$file" >/dev/null 2>&1
csv="$out/$(basename "${file%.*}").csv"
[ -s "$csv" ] || { echo "conversion failed" >&2; exit 1; }
grep -Eq "$want" "$csv"
