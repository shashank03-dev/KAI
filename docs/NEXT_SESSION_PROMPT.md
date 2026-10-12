# Continuation prompt — paste this into the new session

---

You are continuing work on **Kai** (`shashank03-dev/KAI`): an open-source
desktop agent in Rust for **Linux and Windows** that lets Gemma 4 operate
the computer. `kai-core` is the OS-free harness, `kai-os` the OS ports,
`kai-cli` the `kai` CLI, `kai-app` the GPUI desktop app.

## Read first (don't redo what is done)

`README.md`, the newest sections of `DECISIONS.md` (2026-10-12: live runs,
the desktop app driven for real, items 3 and 4), and
`bench/results/2026-10-12-live.md`. Then only the docs your work touches:
`docs/BACKGROUND.md`, `docs/DRIVER_MATRIX.md`, `docs/SAFETY.md`,
`docs/HARNESS.md`, `docs/VERIFICATION.md`, `docs/PROCEDURES.md`.

Done and measured on 2026-10-12 with live Gemma 4 26B (Gemini API):
- harness fixes from live runs (shell repair, repeat guard, retry delay,
  self-kill guard, detached openers): −25% steps, −34% calls, −36% tokens;
- every switch measured twice; none turned on (`--outcomes`,
  `--macro-history`, `--recover`, `--best-of 3`, procedures, window scope);
- both safety gaps closed (focus-aware untargeted typing; replay reads the
  screen before every step);
- window-scoped frames (`KAI_WINDOW_SCOPE=1`, off), driver matrix
  (`crates/kai-os/tests/driver_matrix.rs`, ignored without a desktop);
- three app bugs found by driving `kai-app` (Team approvals dropped,
  false approvals from substring risk words, needless Team splits).

## How to work

Research first, write a short design note, build behind a switch (off until
measured), test (every safety guarantee gets a test that fails without the
guard), measure with `kai bench` at least twice per side, and say how.

## Repo rules

- Commit as the repo owner with **no** `Co-Authored-By` or Claude lines. No
  PR unless asked. Before pushing: `cargo fmt --all --check`,
  `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace`, `kai skill validate skills`,
  `kai bench --backend mock`.
- Files at most 500 lines. English only. No macOS work.
- Secrets only from environment variables or the OS keychain; never in
  files, logs, traces or commits.
- `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0`. Desktop for live runs:
  `sudo scripts/desktop/install.sh`, `source scripts/desktop/start.sh`.

## What is left, in order

1. **False done on "show the folder".** Gemma's main live failure: it lists a
   folder in the terminal and says done when asked to show it in the file
   manager. Look at why the DONE gate did not fire (no outcome named) and
   whether a `window_title` outcome can be asked for when the task says
   "show / open … in <app>".
2. **Qt accessibility.** Qt 5.15 apps publish no AT-SPI tree in the test
   container. Find out whether that is the container or Qt with AT-SPI 2.52
   (try a real KDE session, Qt 6), and fix or document.
3. **Firefox page buttons** expose no AT-SPI action, so Kai clicks them with
   the mouse. Check whether a newer Firefox or `accessibility.*` prefs
   expose `press`; otherwise leave the mouse path.
4. **Windows:** UI Automation events, `PostMessage` clicks for Win32
   controls, and the driver matrix on a real Windows machine.
5. **Chromium/Electron row** of the driver matrix (needs a non-snap
   Chromium or an Electron fixture).

**When done:** tell me plainly what you researched, built, measured (with
numbers), what you didn't, and what I need to do next.
