---
name: run-terminal-task
description: Do a job in the terminal, such as installing a package, checking disk space, finding large files, running a build or tests, or looking up network details. Use when the task is naturally a command rather than clicking.
license: MIT
metadata:
  author: kai
  version: "1.0"
---
# Terminal work

Use `shell` with one command per step and read its output before the next
step. Commands run in the user's home folder: `sh` on Linux, PowerShell on
Windows.

- Look before you change: list, check versions, do a dry run first.
- Prefer commands that print a short answer (`df -h`, `du -sh * | sort -h | tail`,
  `Get-PSDrive`, `ip addr`, `ipconfig`).
- Anything that deletes, overwrites, needs `sudo`/administrator, or changes the
  system is `risk: high`. Kai also marks these high on its own.
- Never pipe a downloaded script into a shell.
- If a command fails, read the error and fix the cause; do not retry blindly.
- Reply `done` with the answer in plain words in `thought`.
