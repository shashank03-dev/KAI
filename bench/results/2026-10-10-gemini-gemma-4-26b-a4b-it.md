# Kai bench: gemini · gemma-4-26b-a4b-it

Live run on the real desktop. Pass is decided by each task's check command where it has one, otherwise by the run's own outcome.

Command: `kai bench --yes --backend gemini --model gemma-4-26b-a4b-it --only web-form-submit`

| Metric | Value |
|---|---|
| Date | 2026-10-10 |
| Tasks passed | 0 / 1 (0%) |
| Skipped | 0 |
| Step p50 / p95 (ms) | 2368 / 4797 |
| Model p50 / p95 (ms) | 2289 / 4750 |
| Observe p50 / p95 (ms) | 56 / 91 |
| Act p50 / p95 (ms) | 57 / 476 |
| Seconds per task (mean) | 48.81 |
| Steps per task (mean) | 9.00 |
| Model calls per task (mean) | 11.00 |
| Prompt / completion tokens | 20091 / 714 |
| Cost (US$) | — |
| Total time (s) | 48.8 |
| A/B flags | all on |
| Machine | Intel(R) Xeon(R) Processor @ 2.10GHz · 4 cores · Ubuntu 24.04.5 LTS · x11 :97 1920x1080 |

| Task | Kind | Result | Judged by | Steps | Seconds | Step p50 ms | Model calls | Tokens in/out |
|---|---|---|---|---|---|---|---|---|
| web-form-submit | Gui | FAIL | check | 9 | 48.81 | 2368 | 11 | 20091 / 714 |

Not measured on this run:

- cost: no prices given (--usd-per-mtok-in/--usd-per-mtok-out)
