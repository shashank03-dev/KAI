# Kai over MCP: the desktop as a tool for other agents

Ranked plan #8, first part ([RESEARCH_2026_10_FOUNDATIONS.md](RESEARCH_2026_10_FOUNDATIONS.md)
§4, "desktop as a tool inside a general agent"). `kai mcp` serves Kai's
eyes and hands as an MCP server on stdio. Claude Code, Codex or a local agent
can then read the screen, act on it through Kai's safety rules, run Kai's
read-only checks, or hand Kai a whole task.

## Set up

Claude Code:

```sh
claude mcp add kai -- kai mcp
```

Any client that reads `.mcp.json`:

```json
{ "mcpServers": { "kai": { "command": "kai", "args": ["mcp"] } } }
```

`kai mcp` takes the same backend options as `kai run` (`--backend`,
`--model`, the A/B flags). They are only used by `kai_run_task`. Without a
model key, the other three tools still work and `kai_run_task` is not
offered.

## Tools

| Tool | What it does |
|---|---|
| `kai_observe {screenshot?}` | The window title and the numbered element list (role and name), the same list the model sees. Says so when a CAPTCHA or code check, or text that looks like a prompt injection or phishing, is on screen. With `screenshot: true`, also a JPEG. |
| `kai_act {action, element? or target?, text?, keys?, direction?, amount?}` | One action: `click`, `double_click`, `right_click`, `type`, `key`, `scroll`, `open_app`, `open_url`. `element: {role?, name}` is found on a fresh tree read and must match exactly one enabled element. `target` is a number from the **last** `kai_observe`, never from a read the caller did not see. Accessibility first, then pointer and keyboard. |
| `kai_probe {kind, …}` | One read-only check from Kai's menu: `file_exists`, `file_contains`, `a11y_has`, `window_title`, `url_is`, `shell_ro` (strict allowlist), with a 2 s limit. |
| `kai_run_task {task}` | Kai's own loop runs the whole task and reports how it ended. Steps that need approval are **skipped** (nobody can approve over MCP), and the report says how many were. Autopilot is treated as "high risk only" here, so high-risk steps are always skipped too. |

## Stricter safety than in Kai's own loop

Over MCP nobody is there to approve, so `kai_act` **refuses**, with a reason
the caller can show the user:

- anything Kai classifies as high risk: sending, buying, deleting,
  submitting, system settings;
- any password or payment field;
- a CAPTCHA or one-time code (only the user does those);
- `shell`, `wait`, `ask_user`, `done`. The calling agent has its own shell;
  an `open_app` name with shell syntax (`;`, `&`, `|`, `$`, quotes, …) is
  refused too, everywhere in Kai, since the name can reach `sh -c` or
  `cmd /C start`.

Tested in `crates/kai-core/tests/mcp_e2e.rs`: refused actions never reach the
hands, bad probes are refused, numbers need an observe first, and a lookup by
name does not change what numbers refer to. A smoke run of the real binary on
the Xvfb desktop answered `initialize`, `tools/list`, `kai_observe` (grid
fallback plus a 190 KB screenshot, since this container has no
accessibility bus), `kai_probe` and the refusals as expected.

## Not done yet (the rest of §4)

- **Background, window-scoped driving.** The pointer fallback still moves
  the real cursor and can raise windows. Per-window capture and a session
  per sub-agent, so Team subtasks run side by side, need OS work that can't
  be tested in this container.
- **A driver regression matrix** (per-toolkit fixture apps that assert an
  accessibility action really changed the widget).
