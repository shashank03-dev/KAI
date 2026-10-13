# Kai bench: gemini+browser · gemma-4-26b-a4b-it

Live run on the real desktop. Pass is decided by each task's check command where it has one, otherwise by the run's own outcome. Browser lane ON (--browser --headless): each task in a fresh headless Kai browser.

Command: `kai bench --backend gemini --model gemma-4-26b-a4b-it --only fill-form --only web-form-submit --only local-price-lookup --only download-file --yes --no-record --browser --headless --out /tmp/claude-0/-home-user-KAI/0b4430f5-d64a-5ddd-8942-4121eb33d114/scratchpad/ab/lane4-1`

| Metric | Value |
|---|---|
| Date | 2026-10-13 |
| Tasks passed | 3 / 4 (75%) |
| Skipped | 0 |
| Step p50 / p95 (ms) | 3831 / 8800 |
| Model p50 / p95 (ms) | 3692 / 8601 |
| Observe p50 / p95 (ms) | 124 / 158 |
| Act p50 / p95 (ms) | 39 / 52 |
| Seconds per task (mean) | 61.62 |
| Steps per task (mean) | 12.25 |
| Model calls per task (mean) | 14.00 |
| Prompt / completion tokens | 117536 / 4418 |
| Cost (US$) | — |
| Failures | loop 1 |
| False done / gated, never verified | 0 / 0 |
| DONE gate refusals | 0 |
| Total time (s) | 246.5 |
| A/B flags | all on |
| Executor | off |
| Machine | Intel(R) Xeon(R) Processor @ 2.10GHz · 4 cores · Ubuntu 24.04.5 LTS · x11 :91 1920x1080 |

| Task | Kind | Result | Judged by | Steps | Seconds | Step p50 ms | Model calls | Tokens in/out | Failure |
|---|---|---|---|---|---|---|---|---|---|
| fill-form | Gui | pass | check | 3 | 9.58 | 1642 | 3 | 6092 / 145 | — |
| web-form-submit | Gui | FAIL | check | 40 | 208.56 | 4020 | 44 | 93567 / 3485 | loop: click #5 repeated 13 times |
| local-price-lookup | Mixed | pass | check | 3 | 18.68 | 3903 | 5 | 9050 / 465 | — |
| download-file | Mixed | pass | check | 3 | 9.67 | 2802 | 4 | 8827 / 323 | — |

Failure categories: loop 1

Not measured on this run:

- cost: no prices given (--usd-per-mtok-in/--usd-per-mtok-out)
