# CLAUDE.md

Kai is an open-source **desktop app for Linux, Windows and macOS (Apple
Silicon)** that lets an
open model (Gemma 4 by default) use the computer: it looks at the screen,
decides, clicks, types and runs commands one visible step at a time. Rust,
MIT. Start with `README.md`; decisions and their evidence are in
`DECISIONS.md` (append-only, newest at the bottom of each section).

## Rules (the owner's, non-negotiable)

1. **Commit identity.** Every commit, yours and every subagent's, is made as
   the owner's GitHub account: `Dev3 ^~^ <shashankgowda3162@gmail.com>`
   (`git config user.name 'Dev3 ^~^'`,
   `git config user.email 'shashankgowda3162@gmail.com'`). No
   `Co-Authored-By`, no Claude or session lines in commit messages, no PR
   unless asked.
2. **Test properly, never loop.** Write focused tests that prove the change;
   run the narrowest test first (`cargo test -p <crate> --test <file>` or
   `-- <name>`), the full suite once before pushing. If the same failure
   comes back twice, stop, find the root cause, or report it. Do not retry
   blindly, re-run long jobs to "see if it passes", or poll in a loop:
   background long work and wait for its notification. Tokens and paid API
   calls are the owner's money.
3. **Research before building.** Before any new file or feature: read the
   code it touches and the relevant `docs/`, check current sources (papers,
   OS and API docs, how other agents do it, with dates), and write a short
   design note (what, why, what was rejected, how it will be tested and
   measured) in the feature's doc or a new `docs/<FEATURE>.md`. If the
   research says it is not worth it, record why in `DECISIONS.md` and stop.
4. **Clean code.** Match the surrounding code's style, naming and comment
   density; small functions; no dead code, no stray debug output, no
   hardcoded paths, model names, keys or machine values (config or env).
   Files stay at **500 lines or fewer**: split by responsibility. English
   only.
5. **Linux and Windows, checked before every push.** CI
   (`.github/workflows/ci.yml`) runs on pushes to `main` and on pull
   requests, but a red run after the push is too late. Every change must build and pass clippy for **both**
   targets before it is pushed (see Checks). Gate OS code with
   `#[cfg(target_os = "linux")]` / `#[cfg(target_os = "windows")]`, and make
   sure nothing is left unused on the other OS (CI uses `-D warnings`).
   macOS (Apple Silicon) is supported since 2026-10-15 (`docs/MACOS.md`):
   gate it with `#[cfg(target_os = "macos")]` and run its clippy check too.

6. Before running any build command which takes time i need you to think before and make me build ship and proceed faster rather than just waiting for the build to complete

7. code faster and ship better and make sure you everything in here is production quality 

## Layout

```
crates/kai-core   the harness, OS-free: engine loop, schema, risk, prompts,
                  skills, memory, procedures, outcome ledger, model clients
                  (vLLM, Gemini, OpenAI-compatible, local llama.cpp, mock).
                  OS access only through the traits in ports.rs.
crates/kai-os     eyes and hands per OS: capture, AT-SPI / UI Automation,
                  Set-of-Marks, input, launcher, shell, kill switch,
                  keychain, local model runtime, web/ (the browser lane).
crates/kai-cli    `kai`: run, bench, doctor, models, skills, mcp.
crates/kai-app    the GPUI desktop app (Ely components, vendor/).
skills/           shipped skills (Agent Skill standard), loaded from disk.
bench/            tasks.yaml, fixtures (local web server), results/.
deploy/           cloud deployment (DigitalOcean).
train/            distillation, fine-tune configs and eval (train/README.md).
docs/             one doc per feature; RESEARCH_*.md for research notes.
scripts/desktop/  a headless Linux desktop for live runs (Xvfb, openbox,
                  AT-SPI, Firefox ESR).
```

## How things are built here

- **OS-free logic in `kai-core`** behind a port; OS code in `kai-os`.
- **New behaviour goes behind a switch, off by default**, until a live
  `kai bench` A/B (at least two runs per side) shows it helps. Say how
  numbers were measured; never claim one that was not.
- **Every safety guarantee gets a test that fails when the guard is
  removed** (risk rules, password fields, address policy, human checks,
  page text treated as data).
- **The system prompt stays byte-stable** for a given setup (prefix
  caching, fine-tunes). Add prompt rules only when they apply (see
  `Perceptor::reads_pages`).
- **stdout belongs to the CLI's user output** (it is the MCP channel in
  `kai mcp`): log with `log::`, never `println!` elsewhere.
- **Secrets** come only from environment variables or the OS keychain:
  never in files, logs, traces, test fixtures or commits. Scan before
  pushing if a key was in play.

## Checks (run all before pushing)

Use the same Rust as CI's `stable` (clippy lints change between versions:
`rustup update stable`). Save disk with
`export CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0`.

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy --workspace --all-targets --target x86_64-pc-windows-gnu -- -D warnings
cargo clippy --workspace --all-targets --target aarch64-apple-darwin -- -D warnings  # see below
cargo test --workspace
cargo run -q -p kai-cli -- skill validate skills
cargo run -q -p kai-cli -- bench --backend mock --out target/bench
```

The Windows check needs `rustup target add x86_64-pc-windows-gnu` (and
`gcc-mingw-w64-x86-64` on Ubuntu). It type-checks Windows code without a
Windows machine; it does not run it.

The macOS check from Linux needs `rustup target add aarch64-apple-darwin`.
No macOS SDK is available off a Mac, so `ring`'s C code is compiled against
stub headers (`TargetConditionals.h`, `string.h`, `assert.h` in a scratch
directory, with `CC_aarch64_apple_darwin=clang` and
`CFLAGS_aarch64_apple_darwin="--target=aarch64-apple-darwin -ffreestanding -nostdlibinc -I<stubs>"`),
and GPUI's shader file only exists after its build script ran on a Mac: an
empty `stitched_shaders.metal` in the `gpui-pre-apple-*/out` build folder
lets the check finish. On a Mac, run the plain command.

Live tests that need a browser or a desktop skip themselves without one:
- Browser lane: `KAI_BROWSER=/path/to/chrome cargo test -p kai-os --test browser_lane --test browser_edges`.
- Desktop: `sudo scripts/desktop/install.sh` once, then
  `scripts/desktop/run.sh <command>` (screen capture needs the window
  manager it starts).
- Live model: `kai bench --backend gemini --model gemma-4-26b-a4b-it`
  (add `--browser --headless` for the browser lane). Keys via
  `GEMINI_API_KEY` (comma-separated for several), never on disk.

## Traps already hit (don't repeat them)

- `pkill -f <pattern>` matches your own shell when the pattern is in the
  command line; kill by PID or from a script file.
- Behind a TLS-inspecting proxy, HTTP clients must trust the OS store
  (reqwest uses `rustls-tls-native-roots` for this).
- A local toolchain older than CI's misses new clippy lints.
- Bench result files are named by date, backend and model: use `--out`
  per run, or runs overwrite each other.
- Skill selection is keyword-based: check that a skill cannot hijack a task
  it was not written for (see `skill::opens_elsewhere`).
