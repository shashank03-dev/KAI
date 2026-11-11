# Security policy

Kai drives a real computer: it clicks, types and runs shell commands for a
model. A bug that lets a web page, a document or a model reply get past its
guards is a security bug, and we want to hear about it.

## Reporting a vulnerability

Report privately through GitHub: **Security › Report a vulnerability** on
this repository. Please do not open a public issue for anything that could
be exploited before a fix ships.

Include what you ran (OS, Kai version or commit, model backend), the task
or page that triggered it, and what Kai did. The run's saved trace (the
`traces` folder in Kai's settings directory) helps; check it for personal
data before you attach it.

You will get an answer within a week. Fixes go into the next release, and
the advisory credits you unless you ask otherwise.

## In scope

- Prompt injection: text on screen, in a page or in command output that
  makes Kai act on it as an instruction (see [docs/SAFETY.md](docs/SAFETY.md)).
- A risky action (deleting files, sending, paying, changing settings) that
  runs without the confirmation the chosen care level requires.
- Typing into password fields, or a secret reaching a model, log, trace or
  file.
- The stop shortcut or kill switch failing to stop a run.
- Keys stored anywhere but the OS keychain or environment variables.
- The model store accepting a download whose hash does not match.

## Out of scope

- What a model you chose says or decides, when every guard worked as
  documented.
- Running Kai with every confirmation turned off and it then doing what you
  asked.
- The DigitalOcean deployment scripts on infrastructure you changed.

## Supported versions

Only the latest release gets security fixes.
