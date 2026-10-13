# Kai bench: gemini+browser · gemma-4-26b-a4b-it

Live run on the real desktop. Pass is decided by each task's check command where it has one, otherwise by the run's own outcome. Browser lane ON (--browser --headless): each task in a fresh headless Kai browser.

Command: `kai bench --backend gemini --model gemma-4-26b-a4b-it --only fill-form --only web-form-submit --only local-price-lookup --only download-file --yes --no-record --browser --headless --out /tmp/claude-0/-home-user-KAI/0b4430f5-d64a-5ddd-8942-4121eb33d114/scratchpad/ab/lane3-2`

| Metric | Value |
|---|---|
| Date | 2026-10-13 |
| Tasks passed | 3 / 4 (75%) |
| Skipped | 0 |
| Step p50 / p95 (ms) | 1940 / 3942 |
| Model p50 / p95 (ms) | 1745 / 3778 |
| Observe p50 / p95 (ms) | 102 / 146 |
| Act p50 / p95 (ms) | 3 / 38 |
| Seconds per task (mean) | 17.05 |
| Steps per task (mean) | 4.25 |
| Model calls per task (mean) | 4.25 |
| Prompt / completion tokens | 33976 / 1265 |
| Cost (US$) | — |
| Failures | gave_up 1 |
| False done / gated, never verified | 0 / 0 |
| DONE gate refusals | 0 |
| Total time (s) | 68.2 |
| A/B flags | all on |
| Executor | off |
| Machine | Intel(R) Xeon(R) Processor @ 2.10GHz · 4 cores · Ubuntu 24.04.5 LTS · x11 :92 1920x1080 |

| Task | Kind | Result | Judged by | Steps | Seconds | Step p50 ms | Model calls | Tokens in/out | Failure |
|---|---|---|---|---|---|---|---|---|---|
| fill-form | Gui | pass | check | 3 | 10.16 | 1826 | 3 | 5963 / 156 | — |
| web-form-submit | Gui | FAIL | check | 8 | 36.98 | 1802 | 6 | 12406 / 420 | gave_up: asked the user: The form is already filled with the correct information. I need to ask the user for permission to submit the form. |
| local-price-lookup | Mixed | pass | check | 3 | 14.19 | 3942 | 5 | 9050 / 465 | — |
| download-file | Mixed | pass | check | 3 | 6.88 | 2260 | 3 | 6557 / 224 | — |

Failure categories: gave_up 1

Not measured on this run:

- cost: no prices given (--usd-per-mtok-in/--usd-per-mtok-out)
