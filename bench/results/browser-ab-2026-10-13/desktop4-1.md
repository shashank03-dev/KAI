# Kai bench: gemini · gemma-4-26b-a4b-it

Live run on the real desktop. Pass is decided by each task's check command where it has one, otherwise by the run's own outcome.

Command: `kai bench --backend gemini --model gemma-4-26b-a4b-it --only fill-form --only web-form-submit --only local-price-lookup --only download-file --yes --no-record --out /tmp/claude-0/-home-user-KAI/0b4430f5-d64a-5ddd-8942-4121eb33d114/scratchpad/ab/desktop4-1`

| Metric | Value |
|---|---|
| Date | 2026-10-13 |
| Tasks passed | 3 / 4 (75%) |
| Skipped | 0 |
| Step p50 / p95 (ms) | 2424 / 5611 |
| Model p50 / p95 (ms) | 2371 / 5455 |
| Observe p50 / p95 (ms) | 115 / 160 |
| Act p50 / p95 (ms) | 2 / 173 |
| Seconds per task (mean) | 68.21 |
| Steps per task (mean) | 12.25 |
| Model calls per task (mean) | 12.25 |
| Prompt / completion tokens | 105560 / 3784 |
| Cost (US$) | — |
| Failures | malformed 1 |
| False done / gated, never verified | 0 / 0 |
| DONE gate refusals | 0 |
| Total time (s) | 272.8 |
| A/B flags | all on |
| Executor | off |
| Machine | Intel(R) Xeon(R) Processor @ 2.10GHz · 4 cores · Ubuntu 24.04.5 LTS · x11 :91 1920x1080 |

| Task | Kind | Result | Judged by | Steps | Seconds | Step p50 ms | Model calls | Tokens in/out | Failure |
|---|---|---|---|---|---|---|---|---|---|
| fill-form | Gui | pass | check | 4 | 13.20 | 2578 | 4 | 8158 / 311 | — |
| web-form-submit | Gui | FAIL | check | 38 | 213.18 | 2547 | 38 | 83400 / 2940 | malformed: 2 undecodable replies; last: malformed reply at step 35: action `open_url` requires a URL in `text` |
| local-price-lookup | Mixed | pass | check | 4 | 36.74 | 4703 | 4 | 7338 / 310 | — |
| download-file | Mixed | pass | check | 3 | 9.73 | 2006 | 3 | 6664 / 223 | — |

Failure categories: malformed 1

Not measured on this run:

- cost: no prices given (--usd-per-mtok-in/--usd-per-mtok-out)
