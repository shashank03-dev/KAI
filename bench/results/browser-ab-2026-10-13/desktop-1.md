# Kai bench: gemini · gemma-4-26b-a4b-it

Live run on the real desktop. Pass is decided by each task's check command where it has one, otherwise by the run's own outcome.

Command: `kai bench --backend gemini --model gemma-4-26b-a4b-it --only fill-form --only web-form-submit --only local-price-lookup --only download-file --yes --no-record --out /tmp/claude-0/-home-user-KAI/0b4430f5-d64a-5ddd-8942-4121eb33d114/scratchpad/ab/desktop-1`

| Metric | Value |
|---|---|
| Date | 2026-10-13 |
| Tasks passed | 4 / 4 (100%) |
| Skipped | 0 |
| Step p50 / p95 (ms) | 3229 / 8289 |
| Model p50 / p95 (ms) | 3097 / 8156 |
| Observe p50 / p95 (ms) | 102 / 140 |
| Act p50 / p95 (ms) | 3 / 378 |
| Seconds per task (mean) | 48.41 |
| Steps per task (mean) | 8.75 |
| Model calls per task (mean) | 11.75 |
| Prompt / completion tokens | 96904 / 3787 |
| Cost (US$) | — |
| Failures | none |
| False done / gated, never verified | 0 / 0 |
| DONE gate refusals | 0 |
| Total time (s) | 193.6 |
| A/B flags | all on |
| Executor | off |
| Machine | Intel(R) Xeon(R) Processor @ 2.10GHz · 4 cores · Ubuntu 24.04.5 LTS · x11 :91 1920x1080 |

| Task | Kind | Result | Judged by | Steps | Seconds | Step p50 ms | Model calls | Tokens in/out | Failure |
|---|---|---|---|---|---|---|---|---|---|
| fill-form | Gui | pass | check | 4 | 15.16 | 3748 | 5 | 9921 / 407 | — |
| web-form-submit | Gui | pass | check | 24 | 132.16 | 3725 | 32 | 66762 / 2591 | — |
| local-price-lookup | Mixed | pass | check | 4 | 33.83 | 5754 | 6 | 11224 / 434 | — |
| download-file | Mixed | pass | check | 3 | 12.49 | 1997 | 4 | 8997 / 355 | — |

Failure categories: none

Not measured on this run:

- cost: no prices given (--usd-per-mtok-in/--usd-per-mtok-out)
