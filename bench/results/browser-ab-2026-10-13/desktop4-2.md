# Kai bench: gemini · gemma-4-26b-a4b-it

Live run on the real desktop. Pass is decided by each task's check command where it has one, otherwise by the run's own outcome.

Command: `kai bench --backend gemini --model gemma-4-26b-a4b-it --only fill-form --only web-form-submit --only local-price-lookup --only download-file --yes --no-record --out /tmp/claude-0/-home-user-KAI/0b4430f5-d64a-5ddd-8942-4121eb33d114/scratchpad/ab/desktop4-2`

| Metric | Value |
|---|---|
| Date | 2026-10-13 |
| Tasks passed | 4 / 4 (100%) |
| Skipped | 0 |
| Step p50 / p95 (ms) | 2744 / 5990 |
| Model p50 / p95 (ms) | 2619 / 5839 |
| Observe p50 / p95 (ms) | 109 / 141 |
| Act p50 / p95 (ms) | 2 / 375 |
| Seconds per task (mean) | 34.05 |
| Steps per task (mean) | 5.25 |
| Model calls per task (mean) | 6.75 |
| Prompt / completion tokens | 56293 / 2210 |
| Cost (US$) | — |
| Failures | none |
| False done / gated, never verified | 0 / 0 |
| DONE gate refusals | 0 |
| Total time (s) | 136.2 |
| A/B flags | all on |
| Executor | off |
| Machine | Intel(R) Xeon(R) Processor @ 2.10GHz · 4 cores · Ubuntu 24.04.5 LTS · x11 :92 1920x1080 |

| Task | Kind | Result | Judged by | Steps | Seconds | Step p50 ms | Model calls | Tokens in/out | Failure |
|---|---|---|---|---|---|---|---|---|---|
| fill-form | Gui | pass | check | 4 | 35.03 | 2468 | 4 | 8150 / 287 | — |
| web-form-submit | Gui | pass | check | 10 | 55.75 | 4230 | 13 | 27922 / 1096 | — |
| local-price-lookup | Mixed | pass | check | 4 | 14.69 | 4708 | 6 | 11224 / 469 | — |
| download-file | Mixed | pass | check | 3 | 30.74 | 2055 | 4 | 8997 / 358 | — |

Failure categories: none

Not measured on this run:

- cost: no prices given (--usd-per-mtok-in/--usd-per-mtok-out)
