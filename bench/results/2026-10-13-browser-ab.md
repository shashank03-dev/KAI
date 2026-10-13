# Browser lane vs desktop, and Holo3.1 4B locally (2026-10-13)

**Setup.** This repo's Linux container: 4 vCPU Xeon (AMX), 16 GB RAM, no GPU,
Xvfb 1920×1080 + openbox + AT-SPI. The bench's four local web tasks
(`fill-form`, `web-form-submit`, `local-price-lookup`, `download-file`), each
checked by its own shell `check` against the local fixture server
(`bench/fixtures/server.py`). `--yes` approves high-risk steps (a person
would be asked).

- **Desktop:** Kai's normal path; Firefox 157 (`BROWSER=/opt/firefox/firefox`),
  screen capture + AT-SPI.
- **Lane:** `--browser --headless`; Chromium 141 (Playwright build), a fresh
  profile per task.
- **Model:** `gemma-4-26b-a4b-it` through the Gemini API (two keys,
  round-robin). Model time includes the free tier's queueing.

Raw results for every round: `browser-ab-2026-10-13/`, Holo:
`holo-2026-10-13/`.

## Current code, two runs per side

| | Desktop (round 4) | Browser lane (round 5) |
| --- | --- | --- |
| Tasks passed | 7 / 8 | **8 / 8** |
| Steps | 70 | **38** |
| Model calls | 76 | **42** |
| Prompt tokens | 162k | **85k** |
| Wall time, all 8 task runs | 409 s | **178 s** |

| Task | Desktop: pass, steps, s | Lane: pass, steps, s |
| --- | --- | --- |
| fill-form | ✓ 4, 13.2 · ✓ 4, 35.0 | ✓ 3, 7.2 · ✓ 3, 8.4 |
| web-form-submit | ✗ 38, 213.2 · ✓ 10, 55.8 | ✓ 10, 27.4 · ✓ 10, 56.8 |
| local-price-lookup | ✓ 4, 36.7 · ✓ 4, 14.7 | ✓ 3, 49.8 · ✓ 3, 12.9 |
| download-file | ✓ 3, 9.7 · ✓ 3, 30.7 | ✓ 3, 8.3 · ✓ 3, 7.0 |

**Read with care.** 8 task runs per side; one task decides the pass-rate
difference, so the pass rate is not statistically solid. Steps, calls and
tokens fall on every task (the lane needs no browser chrome, no address
bar, fewer screenshots per step), which is a more robust signal than the
pass rate.

## How we got here (each round found something, fixed before the next)

| Round | Desktop | Lane | What failed | Fix |
| --- | --- | --- | --- | --- |
| 1–2 | 8/8 | 6/8 | Lane: the model clicked a `<select>`, saw no options (the browser draws the popup outside the page), clicked Send thinking it was "Billing" | Dropdowns open as option marks (`dropdown.rs`) |
| lane 2 | — | 6/8 | The `fill-form` skill ("do not press Send") overrode the user's "send the form" | Skill: send when the user asked |
| 3 | 6/8 | 6/8 | Both: new skill wording ("Kai asks the user first") made Gemma call `ask_user` | Skill: click Send yourself with `risk: high` |
| 4 | 7/8 | 6/8 | Lane: a value typed twice was appended (`ravi@…comravi@…com`), validation blocked Send 20× | Typing replaces a field's value |
| 5 | — | **8/8** | — | — |

Also found on the way, before round 1: the keyword skill picker chose the
Gmail skill for "send the contact form" on 127.0.0.1 and opened Gmail
before the model looked (fixed for both paths).

## Holo3.1 4B, fully local on this CPU

`kai bench --backend local --model holo-3.1-4b-q4 --browser --headless`,
same four tasks, one run, llama.cpp b11429 with Kai's lean server flags.

| Task | Pass | Steps | Seconds |
| --- | --- | --- | --- |
| fill-form | ✓ | 4 | 152 |
| web-form-submit | ✗ (10-min limit; retyped the message 7×) | 15 | 609 |
| local-price-lookup | ✓ | 3 | 129 |
| download-file | ✓ | 2 | 107 |

- **3 / 4 passed**, model call p50 **46 s**, p95 66 s (CPU only, every step
  a new screenshot, so only the system prompt is cached).
- **llama-server RSS: 4.43 GB median, 4.44 GB peak** over the run (sampled
  every 2 s), against 6.51 GB with llama.cpp's defaults.
- In `fill-form` Holo typed a message nobody asked for (the check only
  verifies nothing was submitted, so it passed).
- Not comparable with Gemma above: different hardware path (local CPU vs a
  hosted GPU), one run. It shows Kai can run a computer-use model fully
  offline in ~4.4 GB, and that on a 4-core CPU it is too slow for the
  10-minute task budget on longer tasks. A GPU or the Droplet is the
  practical home for it.
