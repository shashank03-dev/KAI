# Demo script (3 minutes)

Before judges arrive: `deploy/digitalocean/up.sh`, wait for
`curl -s https://<host>/health` → `ready`, open Kai, run one warm-up task.

## 0:00 — The pitch (15 s)
"Kai gives Gemma 4 hands. It looks at your screen, clicks and types one visible
step at a time, and asks before anything risky. Open weights, on our own
DigitalOcean GPU, with Gemini as a fallback."

## 0:15 — The server answers (30 s)
Settings › Model & server › **Save and test**. A real screenshot goes to the
Droplet; the green line shows Gemma 4's latency in ms. Point at the status bar:
connection, model, last step latency.

## 0:45 — A live task (60 s)
Type: *"Open bmsce.ac.in and find the IEEE CS page."* Press Enter.
- The main window steps aside; the mini window shows status, the step in words,
  "Step 2 of 40".
- Expand: steps on the left with capture · tree · encode · model · act timings;
  on the right the frame Kai sent, with the pointer ring where it will click.
- Then a risky one: *"Draft an email to my professor saying I'll be late, and
  send it."* When Kai reaches **Send**, the mini window turns amber and asks.
  Approve (or Skip).

## 1:45 — The kill switch (20 s)
Start any task, press **Ctrl+Alt+Esc** (or flick the mouse to the top-left
corner). Kai stops within one step; the result says "You stopped Kai."

## 2:05 — Save as skill (35 s)
On the Done screen, **Save as skill**. The drafted `SKILL.md` appears beside its
preview; the validator says it's valid against the Agent Skill Open Standard.
Save. Run it from the palette (`Ctrl+K` › Run a skill): the URL step runs with
no model call.

## 2:40 — Benchmarks (20 s)
Benchmarks page: success rate, step p50/p95, the per-task table, and the note
saying exactly how each number was measured.

## Backup plan
- **Venue Wi-Fi fails:** phone hotspot.
- **Droplet down or slow:** Kai falls back to Gemini automatically (status bar
  says "Gemini fallback"); nothing to do.
- **No network at all:** `kai bench --backend replay` runs recorded tasks for
  real on the laptop, and the History page shows earlier runs with their frames.
- **After the demo:** `deploy/digitalocean/down.sh` — the GPU bills until it is
  destroyed.
