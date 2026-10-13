# Kai bench: gemini+browser · gemma-4-26b-a4b-it

Live run on the real desktop. Pass is decided by each task's check command where it has one, otherwise by the run's own outcome. Browser lane ON (--browser --headless): each task in a fresh headless Kai browser.

Command: `kai bench --backend gemini --model gemma-4-26b-a4b-it --only fill-form --only web-form-submit --only local-price-lookup --only download-file --yes --no-record --browser --headless --out /tmp/claude-0/-home-user-KAI/0b4430f5-d64a-5ddd-8942-4121eb33d114/scratchpad/ab/lane4-2`

| Metric | Value |
|---|---|
| Date | 2026-10-13 |
| Tasks passed | 3 / 4 (75%) |
| Skipped | 0 |
| Step p50 / p95 (ms) | 3527 / 5965 |
| Model p50 / p95 (ms) | 3392 / 5887 |
| Observe p50 / p95 (ms) | 117 / 151 |
| Act p50 / p95 (ms) | 37 / 50 |
| Seconds per task (mean) | 58.92 |
| Steps per task (mean) | 12.25 |
| Model calls per task (mean) | 13.25 |
| Prompt / completion tokens | 111308 / 4142 |
| Cost (US$) | — |
| Failures | loop 1 |
| False done / gated, never verified | 0 / 0 |
| DONE gate refusals | 0 |
| Total time (s) | 235.7 |
| A/B flags | all on |
| Executor | off |
| Machine | Intel(R) Xeon(R) Processor @ 2.10GHz · 4 cores · Ubuntu 24.04.5 LTS · x11 :92 1920x1080 |

| Task | Kind | Result | Judged by | Steps | Seconds | Step p50 ms | Model calls | Tokens in/out | Failure |
|---|---|---|---|---|---|---|---|---|---|
| fill-form | Gui | pass | check | 3 | 9.52 | 1706 | 3 | 6092 / 137 | — |
| web-form-submit | Gui | FAIL | check | 40 | 194.46 | 3599 | 42 | 89633 / 3315 | loop: click #5 repeated 9 times |
| local-price-lookup | Mixed | pass | check | 3 | 12.98 | 3955 | 5 | 9050 / 465 | — |
| download-file | Mixed | pass | check | 3 | 18.71 | 2181 | 3 | 6533 / 225 | — |

Failure categories: loop 1

Not measured on this run:

- cost: no prices given (--usd-per-mtok-in/--usd-per-mtok-out)
