# Kai bench: gemini · gemma-4-26b-a4b-it

Live run on the real desktop. Pass is decided by each task's check command where it has one, otherwise by the run's own outcome.

Command: `kai bench --backend gemini --model gemma-4-26b-a4b-it --only fill-form --only web-form-submit --only local-price-lookup --only download-file --yes --no-record --out /tmp/claude-0/-home-user-KAI/0b4430f5-d64a-5ddd-8942-4121eb33d114/scratchpad/ab/desktop-2`

| Metric | Value |
|---|---|
| Date | 2026-10-13 |
| Tasks passed | 4 / 4 (100%) |
| Skipped | 0 |
| Step p50 / p95 (ms) | 2395 / 6564 |
| Model p50 / p95 (ms) | 2214 / 6430 |
| Observe p50 / p95 (ms) | 105 / 137 |
| Act p50 / p95 (ms) | 56 / 393 |
| Seconds per task (mean) | 35.18 |
| Steps per task (mean) | 5.50 |
| Model calls per task (mean) | 7.75 |
| Prompt / completion tokens | 63627 / 2796 |
| Cost (US$) | — |
| Failures | none |
| False done / gated, never verified | 0 / 0 |
| DONE gate refusals | 0 |
| Total time (s) | 140.7 |
| A/B flags | all on |
| Executor | off |
| Machine | Intel(R) Xeon(R) Processor @ 2.10GHz · 4 cores · Ubuntu 24.04.5 LTS · x11 :92 1920x1080 |

| Task | Kind | Result | Judged by | Steps | Seconds | Step p50 ms | Model calls | Tokens in/out | Failure |
|---|---|---|---|---|---|---|---|---|---|
| fill-form | Gui | pass | check | 5 | 28.36 | 3968 | 6 | 11991 / 719 | — |
| web-form-submit | Gui | pass | check | 10 | 62.46 | 1856 | 14 | 29154 / 1172 | — |
| local-price-lookup | Mixed | pass | check | 4 | 16.37 | 5278 | 6 | 11224 / 469 | — |
| download-file | Mixed | pass | check | 3 | 33.53 | 4101 | 5 | 11258 / 436 | — |

Failure categories: none

Not measured on this run:

- cost: no prices given (--usd-per-mtok-in/--usd-per-mtok-out)
