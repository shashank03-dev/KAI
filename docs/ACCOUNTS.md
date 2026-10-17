# Accounts & models

Settings › Accounts & models is the one place to pick a provider, connect
it, refresh its models and choose the model Kai uses. Choosing saves.
Nothing on the page sends a model request or starts a run.

## What it does

- **One provider at a time.** `pages/connections.rs` holds a small state
  machine (`Status`: idle, loading, connected, empty, signed out, canceled,
  failed). Every request carries a generation and the provider/endpoint
  (`source`) it was made for; an answer is applied only when both still
  match. Switching provider (`switch_to`) aborts the request and clears the
  catalog, status and error. Cancel wins over a late answer. A task that
  ends without answering turns into a failure instead of spinning forever.
- **Provider and model always fit.** `kai_core::catalog::check` says which
  IDs a backend can run: Claude Code takes `sonnet`/`opus`/`haiku` or
  `claude-…`; Codex and the OpenAI API refuse Gemma, Gemini and Claude IDs;
  the Gemini API takes Gemma and Gemini. Switching provider replaces a slot
  that does not fit with that provider's default (Codex has none: its
  models come only from the signed-in account). Picking, typing an exact
  ID, starting a run and building a Claude Code or Codex client all go
  through the same check, so Claude Code is never handed a Gemma model.
- **Origins are explicit.** Each entry is `discovered` (the provider's own
  catalog), `documented alias` (names Kai documents, e.g. Claude Code's
  aliases, since Claude Code has no catalog), or `entered by hand`. IDs are
  shown and sent exactly. Codex's `inputModalities` is shown as
  "advertises image input"; a listing is never claimed to prove access or
  screenshot support.
- **Credentials stay where they belong.** ChatGPT/Codex sign-in runs inside
  Codex (`codex app-server`, `account/login/start`); Kai only opens the
  `https://auth.openai.com` URL it returns. Claude Code uses
  `claude auth login|logout|status`. Kai reads no subscription token. A
  compatible server's key is bound to the origin it was saved for
  (`connections::compatible_key`) and never sent to another endpoint.
- **No surprise spending.** Opening Kai no longer sends a connection test
  (it used to send one image request at start and on every backend switch).
  The status bar says "Model not checked · click to test".
- **Missing tools are explained.** `missing_dependency` looks for the Codex
  or Claude Code binary on disk (bundle, PATH, Homebrew, ~/.local/bin) and
  the page says so instead of failing on Connect.

## The crash it fixes

Ely's `Select` asserts it has at least one choice unless it is disabled.
With Codex or Claude Code selected and no catalog fetched, the model pickers
got zero choices and `Select::render` aborted the app whenever Settings was
shown (`select Name("fast") has no choices`, five reports on 2026-10-17).
Empty catalogs now disable the picker with a "connect or refresh first"
placeholder (`accounts::picker_state`).

## Rejected

- Patching Ely's `Select` to tolerate empty lists: the assert is a
  deliberate contract and would hide the caller's missing empty state.
- MCP as the account mechanism: providers own their sign-in flows.
- Fetching Gemini's model list: the documented Gemma list is what Kai's
  Gemini path supports; Test in Model & server checks the key.

## Tests

`crates/kai-core/tests/catalog.rs` (pairing, origins, the client guard),
the `connections` and `accounts` unit tests in kai-app (stale answers,
switching, cancel, empty/failed/signed-out, disabled empty picker), and
`KAI_CAPTURE_ONLY=accounts` screenshots of each state with made-up data.
