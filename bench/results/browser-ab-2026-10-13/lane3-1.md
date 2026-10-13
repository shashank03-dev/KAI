# Kai bench: gemini+browser · gemma-4-26b-a4b-it

Live run on the real desktop. Pass is decided by each task's check command where it has one, otherwise by the run's own outcome. Browser lane ON (--browser --headless): each task in a fresh headless Kai browser.

Command: `kai bench --backend gemini --model gemma-4-26b-a4b-it --only fill-form --only web-form-submit --only local-price-lookup --only download-file --yes --no-record --browser --headless --out /tmp/claude-0/-home-user-KAI/0b4430f5-d64a-5ddd-8942-4121eb33d114/scratchpad/ab/lane3-1`

| Metric | Value |
|---|---|
| Date | 2026-10-13 |
| Tasks passed | 3 / 4 (75%) |
| Skipped | 0 |
| Step p50 / p95 (ms) | 2242 / 4242 |
| Model p50 / p95 (ms) | 2127 / 4059 |
| Observe p50 / p95 (ms) | 115 / 155 |
| Act p50 / p95 (ms) | 9 / 40 |
| Seconds per task (mean) | 17.47 |
| Steps per task (mean) | 4.00 |
| Model calls per task (mean) | 4.00 |
| Prompt / completion tokens | 31763 / 1244 |
| Cost (US$) | — |
| Failures | gave_up 1 |
| False done / gated, never verified | 0 / 0 |
| DONE gate refusals | 0 |
| Total time (s) | 69.9 |
| A/B flags | all on |
| Executor | off |
| Machine | Intel(R) Xeon(R) Processor @ 2.10GHz · 4 cores · Ubuntu 24.04.5 LTS · x11 :91 1920x1080 |

| Task | Kind | Result | Judged by | Steps | Seconds | Step p50 ms | Model calls | Tokens in/out | Failure |
|---|---|---|---|---|---|---|---|---|---|
| fill-form | Gui | pass | check | 3 | 10.71 | 1710 | 3 | 5963 / 156 | — |
| web-form-submit | Gui | FAIL | check | 7 | 34.24 | 1977 | 5 | 10217 / 390 | gave_up: asked the user: The form is already filled out based on the previous steps. I need to ask the user for permission to submit the form. |
| local-price-lookup | Mixed | pass | check | 3 | 16.50 | 4242 | 5 | 9050 / 465 | — |
| download-file | Mixed | pass | check | 3 | 8.43 | 2242 | 3 | 6533 / 233 | — |

Failure categories: gave_up 1

Not measured on this run:

- cost: no prices given (--usd-per-mtok-in/--usd-per-mtok-out)
