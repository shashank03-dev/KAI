#!/usr/bin/env python3
"""Write the licenses of every crate linked into the shipped Mac binaries.

Usage: third_party_licenses.py <output file>

Walks `cargo metadata` for aarch64-apple-darwin from kai-app and kai-cli,
following normal (runtime) dependencies only, and copies each crate's own
license files. Identical texts are written once, with the crates that share
them listed above. Crates that ship no license file are listed with their
SPDX expression and repository so the gap is visible, not hidden.
"""

import hashlib
import json
import pathlib
import subprocess
import sys

ROOTS = ("kai-app", "kai-cli")
TARGET = "aarch64-apple-darwin"
LICENSE_PREFIXES = ("license", "licence", "copying", "notice", "unlicense")


def metadata():
    out = subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--locked",
         "--filter-platform", TARGET],
        check=True, capture_output=True, text=True,
    ).stdout
    return json.loads(out)


def runtime_closure(meta):
    packages = {p["id"]: p for p in meta["packages"]}
    nodes = {n["id"]: n for n in meta["resolve"]["nodes"]}
    todo = [pid for pid, p in packages.items() if p["name"] in ROOTS]
    seen = set()
    while todo:
        pid = todo.pop()
        if pid in seen:
            continue
        seen.add(pid)
        for dep in nodes[pid]["deps"]:
            if any(k["kind"] is None for k in dep["dep_kinds"]):
                todo.append(dep["pkg"])
    workspace = set(meta["workspace_members"])
    return sorted(
        (packages[pid] for pid in seen if pid not in workspace),
        key=lambda p: (p["name"], p["version"]),
    )


def license_files(package):
    root = pathlib.Path(package["manifest_path"]).parent
    return sorted(
        f for f in root.iterdir()
        if f.is_file() and f.name.lower().startswith(LICENSE_PREFIXES)
    )


def main():
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    texts = {}
    missing = []
    crates = runtime_closure(metadata())
    for p in crates:
        name = f"{p['name']} {p['version']} ({p.get('license') or 'no SPDX expression'})"
        files = license_files(p)
        if not files:
            missing.append(f"{name} · {p.get('repository') or 'no repository listed'}")
        for f in files:
            body = f.read_text(errors="replace").strip()
            key = hashlib.sha256(body.encode()).hexdigest()
            texts.setdefault(key, (body, []))[1].append(f"{name} · {f.name}")
    out = [
        "Third-party software in Kai for macOS (Apple Silicon)",
        f"{len(crates)} Rust crates linked into kai-app and kai.",
        "Fonts and icons embedded by Ely GPUI Components and the bundled",
        "Codex helper have their licenses in the same folder as this file.",
        "",
    ]
    for body, users in texts.values():
        out += ["=" * 78, *users, "-" * 78, body, ""]
    if missing:
        out += ["=" * 78, "Crates without a license file in their package:", *missing, ""]
    pathlib.Path(sys.argv[1]).write_text("\n".join(out))
    print(f"{len(crates)} crates, {len(texts)} distinct license texts, "
          f"{len(missing)} without a license file")


if __name__ == "__main__":
    main()
