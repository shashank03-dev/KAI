# Kai bench: gemini · gemma-4-26b-a4b-it

Live run on the real desktop. Pass is decided by each task's check command where it has one, otherwise by the run's own outcome.

Command: `kai bench --backend gemini --model gemma-4-26b-a4b-it --only fill-form --only web-form-submit --only local-price-lookup --only download-file --yes --no-record --out /tmp/claude-0/-home-user-KAI/0b4430f5-d64a-5ddd-8942-4121eb33d114/scratchpad/ab/desktop3-2`

| Metric | Value |
|---|---|
| Date | 2026-10-13 |
| Tasks passed | 3 / 4 (75%) |
| Skipped | 0 |
| Step p50 / p95 (ms) | 2363 / 7099 |
| Model p50 / p95 (ms) | 2198 / 6417 |
| Observe p50 / p95 (ms) | 107 / 154 |
| Act p50 / p95 (ms) | 11 / 412 |
| Seconds per task (mean) | 26.58 |
| Steps per task (mean) | 5.00 |
| Model calls per task (mean) | 5.75 |
| Prompt / completion tokens | 47282 / 2026 |
| Cost (US$) | — |
| Failures | gave_up 1 |
| False done / gated, never verified | 0 / 0 |
| DONE gate refusals | 0 |
| Total time (s) | 106.3 |
| A/B flags | all on |
| Executor | off |
| Machine | Intel(R) Xeon(R) Processor @ 2.10GHz · 4 cores · Ubuntu 24.04.5 LTS · x11 :92 1920x1080 |

| Task | Kind | Result | Judged by | Steps | Seconds | Step p50 ms | Model calls | Tokens in/out | Failure |
|---|---|---|---|---|---|---|---|---|---|
| fill-form | Gui | pass | check | 4 | 14.82 | 2972 | 4 | 7989 / 359 | — |
| web-form-submit | Gui | FAIL | check | 9 | 38.88 | 2496 | 9 | 18913 / 826 | gave_up: asked the user: The form is already partially filled. I need to select 'Billing' in the Topic dropdown, but it seems to be already set to 'Billing'. I |
| local-price-lookup | Mixed | pass | check | 4 | 40.80 | 2363 | 6 | 11383 / 485 | — |
| download-file | Mixed | pass | check | 3 | 11.81 | 2039 | 4 | 8997 / 356 | — |

Failure categories: gave_up 1

Not measured on this run:

- cost: no prices given (--usd-per-mtok-in/--usd-per-mtok-out)
