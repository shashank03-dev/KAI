# Continuation prompt — paste this into the new session

---

You are continuing work on **Kai** (`shashank03-dev/KAI`, branch `main`): an
open-source desktop agent in Rust for **Linux and Windows** that lets Gemma 4
operate the computer. `kai-core` is the OS-free harness, `kai-os` holds the OS
ports, `kai-cli` is the `kai` CLI and `kai-app` is the GPUI desktop app.

## Read first (don't redo what is done)

`README.md`, `DECISIONS.md` (the newest sections), `docs/HARNESS.md`, and
the end of `docs/RESEARCH_2026_10_FOUNDATIONS.md` §7 (status of the ranked
plan). Open a feature doc only when your work touches that feature:
VERIFICATION, PROCEDURES, MEMORY, MCP, SAFETY, HUMAN_CHECKS, EXECUTOR.

Already built and tested, each **off by default** until measured:
`--outcomes`, `--macro-history`, `--procedures`, `--best-of 3`, `--recover`,
plus screen-scoped lessons, app facts, `kai memory consolidate` / `exam`,
`kai mcp` and the safety rules. Don't rebuild any of these.

## How to work: research first, then build

For **each** item below, in order:

1. **Research before writing code.** Read the code it touches and the
   relevant docs, and look up current sources (papers, OS API docs, how
   other agents do it) with dates. Write a short design note: what you'll
   build, why, what you rejected, and how you'll test and measure it. Put
   it in the feature's doc, or a new `docs/<FEATURE>.md`. Only then build.
   If the research says an item isn't worth it, don't build it; record why
   in `DECISIONS.md`.
2. **Build it to the repo's standard.** Match the surrounding code's style,
   naming and comment density. Keep OS-free logic in `kai-core` behind a
   port, and OS code in `kai-os`. Nothing hardcoded that belongs in config:
   no paths, model names, keys or machine-specific values. Never print to
   stdout outside the CLI's user output (stdout is the MCP channel in
   `kai mcp`). New behaviour goes behind a switch, off until measured.
3. **Test it properly.** Add unit tests and an engine test with the fakes in
   `crates/kai-core/tests/common/`. Every safety guarantee gets a test that
   fails when the guard is removed.
4. **Measure, honestly.** Compare before and after with `kai bench`. Never
   claim a number that wasn't measured, and say how it was measured.

## Repo rules

- Commit as the repo owner with **no** `Co-Authored-By` or Claude lines. Do
  not open a PR unless asked.
- **One push per batch, only when I say "push".** Commit locally; backing up
  to a non-`main` branch is free (CI runs only on `main`). Before pushing,
  run `cargo fmt --all --check`,
  `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace`, `kai skill validate skills` and
  `kai bench --backend mock`. Windows code can't compile here, so keep it
  small and cfg-gated.
- Files are at most 500 lines. English only. No macOS work.
- Secrets come from environment variables or the OS keychain. Never write
  them to files, logs, traces or commits.
- Disk is tight: use `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0`.

## What to build, in order

1. **Measure what exists (needs a model key and the GUI apps).** Run
   `kai bench --backend gemini --model gemma-4-26b-a4b-it` with and without
   each switch, at least twice per side. The exact commands are in
   `bench/results/2026-10-12-verification.md` and each feature's doc. Turn a
   default on only where the pass rate holds and the target metric improves
   (false done, steps, model calls). If there's no key, say so and go to 2.
2. **Close the two known safety gaps.**
   - Typing into an already-focused password field with no `target` doesn't
     ask for approval (predates this work).
   - Procedure replay doesn't read the screen before untargeted steps, so a
     CAPTCHA or suspicious text there goes unseen.
3. **Background, window-scoped driving** (RESEARCH_2026_10_FOUNDATIONS §4):
   - a pointer fallback that doesn't steal focus or move the real cursor;
   - per-window capture instead of whole-screen;
   - a session per sub-agent, so Team subtasks run side by side.
   Research the Linux (AT-SPI, X11/Wayland) and Windows (UIA, SendInput,
   PrintWindow) options first.
4. **Driver regression matrix:** tiny per-toolkit fixture apps (GTK, Qt,
   Electron or web) that check an accessibility action really changed the
   widget, so a driver change can't silently fall back to the mouse.
5. **Visual re-aim for procedures, only if 1 shows replays failing on
   renamed targets:** normalised cross-correlation on Kai's own capture,
   searched near the element's old position. `rustautogui` was evaluated and
   rejected; see `DECISIONS.md`.

6. **Browser lane follow-ups** (`docs/BROWSER.md`, built 2026-10-12, off
   unless `--browser`): measure the bench's web tasks with and without
   `--browser` on a live model; page text for reading tasks
   (`docs/RESEARCH_2026_10_VENDORS.md` §5.2); a switch in the app.

**When done:** tell me plainly what you researched, what you built, what you
measured (with numbers), what you didn't, and what I need to do next.
