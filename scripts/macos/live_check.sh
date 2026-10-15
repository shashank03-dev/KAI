#!/bin/bash
# Kai's eyes and hands on a real Mac, end to end, with no model: open a
# TextEdit document, read the screen and the accessibility tree, type into
# the document through the Accessibility API and through the mouse and
# keyboard, and read the text back from the tree each time. Needs Screen
# Recording and Accessibility for the app running it (your terminal).
#
#   scripts/macos/live_check.sh [OUT_DIR]     (default target/mac-live)
#
# Exits non-zero on the first check that fails; OUT_DIR keeps the probe
# output, a screenshot and the marked frames the model would see.
set -euo pipefail

cd "$(dirname "$0")/../.."
[[ "$(uname -s)" == "Darwin" ]] || { echo "run this on a Mac" >&2; exit 1; }
out=${1:-target/mac-live}
mkdir -p "$out"
cargo build --release -p kai-os --example probe
probe=target/release/examples/probe

fail() { echo "FAIL: $*" >&2; exit 1; }

# A fresh, empty plain-text document in front, its text area focused.
open_doc() {
    local doc="$out/$1.txt"
    : > "$doc"
    open -a TextEdit "$doc"
    sleep 4
}

# The text area's value, as Kai's own accessibility read sees it.
doc_text() {
    "$probe" value @textfield | sed -n 's/^value #[0-9]*: //p'
}

echo "== screen and tree"
open_doc mouse
screencapture -x "$out/screen.png" || echo "(screencapture unavailable)"
"$probe" tree 3 | tee "$out/tree.txt"
grep -q "marks [1-9]" "$out/tree.txt" \
    || fail "no marks: is Accessibility allowed for this terminal?"
"$probe" frames "$out" | tee "$out/frames.txt"

echo "== mouse and keyboard: click the document, type"
"$probe" mouse @textfield "kai mouse ok" | tee "$out/mouse.txt"
got=$(doc_text)
echo "document now: $got"
# TextEdit capitalises the first word as it is typed: compare without case
# (macOS ships bash 3.2, so through tr).
lower=$(printf '%s' "$got" | tr '[:upper:]' '[:lower:]')
[[ "$lower" == *"kai mouse ok"* ]] || fail "typed text did not land (got: $got)"

echo "== accessibility: fill an empty document"
open_doc a11y
"$probe" a11y @textfield "kai a11y ok" | tee "$out/a11y.txt"
grep -q "done=true" "$out/a11y.txt" || fail "the accessibility path declined"
got=$(doc_text)
echo "document now: $got"
[[ "$got" == "kai a11y ok" ]] || fail "AXValue fill did not stick (got: $got)"

osascript -e 'quit app "TextEdit" saving no' >/dev/null 2>&1 || true
echo "all live checks passed"
