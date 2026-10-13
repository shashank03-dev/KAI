# Kai bench: gemini · gemma-4-26b-a4b-it

Live run on the real desktop. Pass is decided by each task's check command where it has one, otherwise by the run's own outcome.

Command: `kai bench --backend gemini --model gemma-4-26b-a4b-it --only fill-form --only web-form-submit --only local-price-lookup --only download-file --yes --no-record --out /tmp/claude-0/-home-user-KAI/0b4430f5-d64a-5ddd-8942-4121eb33d114/scratchpad/ab/desktop3-1`

| Metric | Value |
|---|---|
| Date | 2026-10-13 |
| Tasks passed | 3 / 4 (75%) |
| Skipped | 0 |
| Step p50 / p95 (ms) | 2319 / 4644 |
| Model p50 / p95 (ms) | 2191 / 4529 |
| Observe p50 / p95 (ms) | 90 / 133 |
| Act p50 / p95 (ms) | 2 / 159 |
| Seconds per task (mean) | 32.70 |
| Steps per task (mean) | 5.00 |
| Model calls per task (mean) | 5.75 |
| Prompt / completion tokens | 47387 / 1967 |
| Cost (US$) | — |
| Failures | gave_up 1 |
| False done / gated, never verified | 0 / 0 |
| DONE gate refusals | 0 |
| Total time (s) | 130.8 |
| A/B flags | all on |
| Executor | off |
| Machine | Intel(R) Xeon(R) Processor @ 2.10GHz · 4 cores · Ubuntu 24.04.5 LTS · x11 :91 1920x1080 |

| Task | Kind | Result | Judged by | Steps | Seconds | Step p50 ms | Model calls | Tokens in/out | Failure |
|---|---|---|---|---|---|---|---|---|---|
| fill-form | Gui | pass | check | 4 | 13.11 | 3728 | 4 | 7986 / 343 | — |
| web-form-submit | Gui | FAIL | check | 9 | 64.20 | 2706 | 9 | 19021 / 800 | gave_up: asked the user: The form is already partially filled. I need to select 'Billing' in the Topic dropdown, but it seems to be already set to 'Billing'. I |
| local-price-lookup | Mixed | pass | check | 4 | 19.79 | 2319 | 6 | 11383 / 469 | — |
| download-file | Mixed | pass | check | 3 | 33.70 | 1994 | 4 | 8997 / 355 | — |

Failure categories: gave_up 1

Not measured on this run:

- cost: no prices given (--usd-per-mtok-in/--usd-per-mtok-out)
