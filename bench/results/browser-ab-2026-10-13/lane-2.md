# Kai bench: gemini+browser · gemma-4-26b-a4b-it

Live run on the real desktop. Pass is decided by each task's check command where it has one, otherwise by the run's own outcome. Browser lane ON (--browser --headless): each task in a fresh headless Kai browser.

Command: `kai bench --backend gemini --model gemma-4-26b-a4b-it --only fill-form --only web-form-submit --only local-price-lookup --only download-file --yes --no-record --browser --headless --out /tmp/claude-0/-home-user-KAI/0b4430f5-d64a-5ddd-8942-4121eb33d114/scratchpad/ab/lane-2`

| Metric | Value |
|---|---|
| Date | 2026-10-13 |
| Tasks passed | 3 / 4 (75%) |
| Skipped | 0 |
| Step p50 / p95 (ms) | 1971 / 5767 |
| Model p50 / p95 (ms) | 1773 / 5649 |
| Observe p50 / p95 (ms) | 123 / 169 |
| Act p50 / p95 (ms) | 39 / 49 |
| Seconds per task (mean) | 18.88 |
| Steps per task (mean) | 3.75 |
| Model calls per task (mean) | 4.50 |
| Prompt / completion tokens | 35727 / 1307 |
| Cost (US$) | — |
| Failures | false_done 1 |
| False done / gated, never verified | 1 / 0 |
| DONE gate refusals | 0 |
| Total time (s) | 75.5 |
| A/B flags | all on |
| Executor | off |
| Machine | Intel(R) Xeon(R) Processor @ 2.10GHz · 4 cores · Ubuntu 24.04.5 LTS · x11 :92 1920x1080 |

| Task | Kind | Result | Judged by | Steps | Seconds | Step p50 ms | Model calls | Tokens in/out | Failure |
|---|---|---|---|---|---|---|---|---|---|
| fill-form | Gui | pass | check | 3 | 8.59 | 2751 | 3 | 5855 / 201 | — |
| web-form-submit | Gui | FAIL | check | 6 | 12.89 | 1808 | 6 | 11995 / 320 | false_done: said done (The form seems to have been submitted successfully as the screen says 'Thank you, message received'. I should check if there's anything else to do,) but the check failed |
| local-price-lookup | Mixed | pass | check | 3 | 37.42 | 4063 | 5 | 9050 / 465 | — |
| download-file | Mixed | pass | check | 3 | 16.62 | 3050 | 4 | 8827 / 321 | — |

Failure categories: false_done 1

Not measured on this run:

- cost: no prices given (--usd-per-mtok-in/--usd-per-mtok-out)
