# Kai

**Give Gemma a hand.** Kai is an open-source desktop app for Linux and Windows
that lets Gemma 4 use your computer: it looks at the screen, decides, clicks,
types and runs commands one visible step at a time, while you watch and can
stop it instantly.

Gemma 4 runs on your own DigitalOcean GPU Droplet (open weights, served by
vLLM), with Google's Gemini API as an automatic fallback.

MIT licensed · Rust · GPUI + [Ely GPUI Components](https://github.com/ZacharyZhang-NY/Ely-GPUI-Components)

![What Kai sends the model: a real frame with numbered marks](docs/img/set-of-marks-gtk.jpg)

*A real frame Kai sent while testing on Linux: every button and field from the
accessibility tree, numbered, so Gemma answers "click #14" instead of guessing
pixels.*

## What it does

- **Point-and-click tasks:** "Open bmsce.ac.in and find the IEEE CS page",
  "Play lofi hip hop on YouTube", "Fill this form with my details".
- **Terminal tasks:** "Find the 5 biggest files in Downloads", "Rename every
  photo in ~/Pictures/trip with today's date". Kai runs the command and reads
  the output.
- **Security check-ups of your own things:** a read-only review of a website
  you own (certificate, redirects, security headers, SPF/DMARC) and of your own
  PC (firewall, updates, listening services). Kai confirms ownership first and
  asks before changing anything.
- **Sub-agents:** turn on *Team* and Kai splits a task into parallel
  sub-agents — terminal subtasks run at the same time, screen subtasks take
  turns — then writes one report.
- **Skills:** reusable know-how in the Agent Skill Open Standard. Finish a run,
  press **Save as skill**, and the next run of that task skips steps the model
  no longer needs to think about.

## Architecture

```
crates/
  kai-core   the harness: loop, schema, risk, prompts, skills, sub-agents,
             model backends (vLLM, Gemini, failover, mock). OS-free, tested.
  kai-os     eyes and hands: capture, AT-SPI / UI Automation, Set-of-Marks,
             input, launcher, shell, kill switch, keychain. Linux + Windows.
  kai-cli    `kai`: run tasks headless, validate skills, benchmark, doctor.
  kai-app    the desktop app (GPUI + Ely).
deploy/digitalocean   one-command GPU Droplet with vLLM + Caddy TLS.
skills/               9 shipped skills.   bench/   10 benchmark tasks.
```

The loop is **observe → ground → decide → act → verify**, one action per step.
Full explanation with a diagram: [docs/HARNESS.md](docs/HARNESS.md).

## Why it's fast

- **MoE by default.** `gemma-4-26b-a4b-it` (~4B active parameters) for every
  step; the dense 31B only for the one step after a failure.
- **Prefix caching.** Stable system prompt and schema first, the changing
  screenshot and last 3 steps last, so vLLM reuses the cached prefix.
- **Small replies.** Thought ≤ 25 words, 192 max tokens, temperature 0,
  schema-constrained output on vLLM (no retries).
- **Skip the model when possible.** `open_app`, `open_url` and skills'
  deterministic steps go straight to the OS.
- **Cheap eyes.** Capture and accessibility tree in parallel, SIMD downscale to
  Gemma 4's 280-token budget (1070x602 for 16:9), a built-in bitmap font for
  the marks.
- **Parallel sub-agents** for independent terminal work.

**Measured** (this repo's Linux CI container, 4 vCPU, no GPU, virtual display):

| What | p50 | p95 |
| --- | --- | --- |
| Kai's own overhead per step (capture, tree, encode, act; model excluded) — `kai bench --backend replay`, 5 real tasks, 5/5 passed | 65 ms | 93 ms |
| Observe (capture ‖ tree, then encode), 81-element GTK window | ~200 ms | — |

**Not measured yet:** end-to-end seconds per step and success rate with Gemma 4
on the Droplet or on Gemini — no GPU or API key was available where this was
built. Run `kai bench --backend vllm` and `--backend gemini` on your machine;
results land in `bench/results/` and on the Benchmarks page. **No comparison
with OpenAI's computer use was run** (no key supplied), so this README makes no
claim about it.

## Why you can trust it

- **You see everything.** The mini window shows what Kai is doing right now;
  the run view shows every frame it sent and every action it took, with
  timings.
- **Risky actions ask first.** Sending, buying, deleting, submitting, changing
  settings, destructive commands — and typing into a password field, which is
  always high risk whatever the model says. Autopilot still always asks for
  payments and passwords.
- **Three ways to stop:** `Ctrl+Alt+Esc` from any app, push the mouse into the
  top-left corner, or press Stop. Kai stops within one step and kills any
  command it started.
- **Your screen stays yours.** Screenshots go only to the model endpoint you
  configure. Traces stay on your computer. API keys live in the OS keychain.
  Kai's own windows are kept out of every screenshot.
- **Limits:** 40 steps and 10 minutes per task by default.

## Quick start

### Download
Grab `kai-linux-x86_64.tar.gz` or `kai-windows-x86_64.zip` from Releases,
unpack, and run `kai-app` (the `kai` command-line tool is next to it).
The Windows build is unsigned: on the SmartScreen prompt choose
*More info › Run anyway*.

### Build
```sh
# Linux build deps (Ubuntu/Debian)
sudo apt install libxkbcommon-dev libxkbcommon-x11-dev libwayland-dev libvulkan-dev \
  libx11-xcb-dev libfontconfig-dev libdbus-1-dev libxdo-dev
cargo run --release -p kai-app          # the app
cargo run --release -p kai-cli -- --help  # the CLI
```
Windows: install Rust (MSVC) and run the same `cargo` commands.

**Linux note:** use an Xorg session for full control. On Wayland, Kai can
capture the screen (through the portal) but can only click and type into
XWayland apps; onboarding tells you this.

### First run
Onboarding walks you through: connect a model (a real image request tests it),
grant access (each check turns green by itself), choose how careful Kai is and
your stop shortcut, then one safe demo task.

### From the terminal
```sh
kai key gemini                      # store a key in the OS keychain
kai doctor                          # screen / accessibility / input checks
kai run "find the 5 biggest files in Downloads"
kai run --team "check my PC's firewall, updates and open ports"
kai skill validate skills
kai bench --backend replay --yes    # real desktop, recorded replies
kai bench --backend vllm            # the real thing
```

## Deploy Gemma 4 on DigitalOcean

```sh
export HF_TOKEN=hf_...  KAI_SSH_KEY=my-laptop
deploy/digitalocean/up.sh      # 1x H100, vLLM 0.25+, Caddy TLS, firewall
deploy/digitalocean/smoke.sh   # 20 real image requests, p50/p95
deploy/digitalocean/down.sh    # destroy it: GPU Droplets bill until destroyed
```
Details, cost and teardown: [deploy/digitalocean/README.md](deploy/digitalocean/README.md).

## Write a skill
See [docs/SKILLS.md](docs/SKILLS.md). Short version: a folder with a
`SKILL.md` whose frontmatter has `name` (= folder name) and `description`;
optional deterministic steps in `metadata.kai`. Validate with
`kai skill validate <dir>`.

## Docs
- [docs/HARNESS.md](docs/HARNESS.md) — the loop, speed, safety, sub-agents
- [docs/DESIGN.md](docs/DESIGN.md) — screens, states and components
- [docs/SKILLS.md](docs/SKILLS.md) — the skill format and Kai's extension
- [docs/DEMO.md](docs/DEMO.md) — the 3-minute demo
- [DECISIONS.md](DECISIONS.md) — every assumption and deviation, and what was not run here

## Credits and licences
Kai: MIT. Gemma 4 by Google DeepMind (Gemma Terms of Use). Ely GPUI Components
(MIT) on Zed's GPUI. Lucide icons (ISC). Inter, JetBrains Mono, IBM Plex Sans
(OFL). vLLM (Apache-2.0).
