---
name: rename-files-in-folder
description: Batch rename files in a folder, for example add a prefix, number them, or change extensions. Use when the user wants to rename, renumber or tidy up many files at once.
license: MIT
metadata:
  author: kai
  version: "1.0"
---
# Rename files in a folder

This is terminal work: use `shell`, not the file manager. It is faster and
exact.

1. First list the folder so you and the user can see what is there:
   - Linux: `ls -1 "<folder>"`
   - Windows: `Get-ChildItem -Name "<folder>"`
2. Show the plan with a dry run that only prints `old -> new`. For example, to
   add a prefix:
   - Linux: `cd "<folder>" && for f in *; do echo "$f -> 2026-$f"; done`
   - Windows: `Get-ChildItem "<folder>" | ForEach-Object { "$($_.Name) -> 2026-$($_.Name)" }`
3. Run the real rename with the same pattern (`mv -n` on Linux so nothing is
   overwritten; `Rename-Item` on Windows). Mark it `risk: low` unless files
   could be overwritten, then `high`.
4. List the folder again and reply `done` with how many files were renamed.

Never delete files in this skill. If two files would get the same name, stop
and `ask_user`.
