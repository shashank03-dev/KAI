# Kai bench: gemini · gemma-4-26b-a4b-it

Live run on the real desktop. Pass is decided by each task's check command where it has one, otherwise by the run's own outcome.

Command: `kai bench --yes --backend gemini --model gemma-4-26b-a4b-it --executor --only web-form-submit --no-record`

| Metric | Value |
|---|---|
| Date | 2026-10-10 |
| Tasks passed | 0 / 1 (0%) |
| Skipped | 0 |
| Step p50 / p95 (ms) | 3780 / 9577 |
| Model p50 / p95 (ms) | 3561 / 9523 |
| Observe p50 / p95 (ms) | 71 / 533 |
| Act p50 / p95 (ms) | 59 / 414 |
| Seconds per task (mean) | 85.87 |
| Steps per task (mean) | 11.00 |
| Model calls per task (mean) | 11.00 |
| Prompt / completion tokens | 23329 / 1062 |
| Cost (US$) | — |
| Failures | false_done 1 |
| Total time (s) | 85.9 |
| A/B flags | --executor |
| Executor | on · 2 decisions · 1 escalations · 461 / 461 ms (p50 / p95) |
| Machine | Intel(R) Xeon(R) Processor @ 2.10GHz · 4 cores · Ubuntu 24.04.5 LTS · x11 :97 1920x1080 |

| Task | Kind | Result | Judged by | Steps | Seconds | Step p50 ms | Model calls | Tokens in/out | Failure |
|---|---|---|---|---|---|---|---|---|---|
| web-form-submit | Gui | FAIL | check | 11 | 85.87 | 3780 | 11 | 23329 / 1062 | false_done: said done (The contact form has been submitted successfully, as indicated by the 'Thank you, message received' text on the screen. The task is complete.) but the check failed |

Failure categories: false_done 1

Not measured on this run:

- cost: no prices given (--usd-per-mtok-in/--usd-per-mtok-out)
