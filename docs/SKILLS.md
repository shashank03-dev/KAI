# Skills

Kai's skills follow the [Agent Skill Open Standard](https://agentskills.io/specification)
exactly. Any agent that reads the standard can use them; Kai adds one optional
extension that stays inside the spec.

## Layout

```
skills/<name>/
├── SKILL.md        # required: YAML frontmatter + Markdown instructions
├── scripts/        # optional
├── references/     # optional, loaded on demand
└── assets/         # optional
```

Frontmatter rules Kai enforces (`kai skill validate`):

| Field | Rule |
| --- | --- |
| `name` | required; 1–64 chars; `a-z`, `0-9`, `-`; no leading, trailing or double hyphen; **must equal the folder name** |
| `description` | required; 1–1024 chars; say what it does *and when to use it* |
| `license` | optional |
| `compatibility` | optional; ≤ 500 chars |
| `metadata` | optional; string → string map |
| `allowed-tools` | optional; space-separated (experimental in the spec) |

## Progressive disclosure

1. Only each skill's `name` and `description` go into Kai's system prompt.
2. When a task matches a skill (keyword match on name and description,
   ignoring filler words), its full `SKILL.md` body is added to the next call.
3. Files in `references/` are read only when the body points to them.

## Kai's extension: deterministic steps

A skill may list steps Kai runs **without asking the model**, stored as a JSON
array in the standard `metadata` map under the key `kai`:

```yaml
---
name: write-email-gmail
description: Compose an email in Gmail in the browser. Use when the user asks to write or send an email.
metadata:
  kai: '[{"do":"open_url","url":"https://mail.google.com/mail/u/0/#inbox?compose=new"}]'
---
```

Step kinds: `open_url {url}`, `open_app {app}`, `shortcut {keys}`, `type {text}`.
They run first, in order, each followed by a settle; then the model takes over
with the skill's instructions. Because it is a plain metadata string, other
agents ignore it and the skill stays valid everywhere.

## Shipped skills

| Skill | What it does |
| --- | --- |
| `open-and-search-web` | Opens DuckDuckGo (deterministic step), searches, opens the best result |
| `write-email-gmail` | Opens a Gmail draft (deterministic step), fills it; Send is high risk |
| `rename-files-in-folder` | Batch renames from the terminal with a dry run first; never deletes |
| `fill-form-from-text` | Fills a form from pasted details; never fills passwords or payments, never submits |
| `spotify-or-youtube-play` | Goes straight to search results by URL and plays the match |
| `system-settings-dark-mode` | Switches dark/light mode on Windows, GNOME or KDE (high risk, approved) |
| `run-terminal-task` | General terminal work: look first, dry runs, destructive commands marked high |
| `own-site-security-check` | Read-only review of a site **you own**: certificate, redirects, security headers, SPF/DMARC/CAA; confirms ownership first |
| `pc-security-checkup` | Read-only check of this PC: firewall, updates, listening services, encryption; asks before any fix |

## Write your own

- In the app: finish a run, press **Save as skill**. Kai drafts a `SKILL.md`
  from what worked (leading URL/app/shortcut steps become deterministic steps),
  you edit it beside a live preview, and it validates as you type.
- By hand: create `~/.config/kai/skills/<name>/SKILL.md` (Linux) or
  `%APPDATA%\Kai\skills\<name>\SKILL.md` (Windows), or drop a folder on the
  Skills page.
- Check it: `kai skill validate path/to/skill` (or a folder of skills).
