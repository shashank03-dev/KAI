# Contributing to Kai

Thanks for helping. Bug reports, skills, docs and code are all welcome.

## Before you start

- Read [README.md](README.md), then the doc for the area you touch
  (`docs/`, one doc per feature). Decisions and the evidence behind them
  are in [DECISIONS.md](DECISIONS.md).
- For anything bigger than a bug fix, open an issue first so we can agree
  on the approach. New behaviour lands behind a switch, off by default,
  until a `kai bench` A/B (two runs per side or more) shows it helps.
- Security issues go through [SECURITY.md](SECURITY.md), not public issues.

## Layout

- `crates/kai-core`: the harness. No OS calls; OS access only through the
  traits in `ports.rs`.
- `crates/kai-os`: capture, accessibility, input and shell per OS. Gate code
  with `#[cfg(target_os = "...")]` and leave nothing unused on other OSes.
- `crates/kai-cli`: the `kai` command. `crates/kai-app`: the desktop app.
- `skills/`: shipped skills ([docs/SKILLS.md](docs/SKILLS.md)).

## Rules that keep Kai safe and fast

- Every safety guarantee has a test that fails when the guard is removed.
- Text from the screen, pages or command output is data, never instructions.
- The system prompt stays byte-stable for a given setup (prefix caching).
- stdout belongs to the CLI's output (it is the MCP channel in `kai mcp`):
  log with `log::`, never `println!` elsewhere.
- Secrets come only from environment variables or the OS keychain, never
  from files, logs, traces, fixtures or commits.
- Source files stay at 500 lines or fewer; split by responsibility.

## Checks

CI runs these on every pull request. Run them before you push:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -q -p kai-cli -- skill validate skills
cargo run -q -p kai-cli -- bench --backend mock --out target/bench
```

Linux needs the packages listed under **Build** in the README. To
type-check Windows code from Linux:
`rustup target add x86_64-pc-windows-gnu`, install `gcc-mingw-w64-x86-64`,
then `cargo clippy --workspace --all-targets --target x86_64-pc-windows-gnu -- -D warnings`.

Tests that need a browser or a desktop skip themselves without one; see
`scripts/desktop/` for a headless Linux desktop.

## Pull requests

Keep each pull request to one change, explain why it is needed and how you
tested it, and include bench numbers (with how they were measured) for
anything that changes agent behaviour. By contributing you agree that your
work is released under the [MIT License](LICENSE).
