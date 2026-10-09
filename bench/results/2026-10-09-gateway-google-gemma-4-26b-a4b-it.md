# Kai bench: gateway · google/gemma-4-26b-a4b-it

Live run on the real desktop. Pass is decided by each task's check command where it has one, otherwise by the run's own outcome.

Command: `kai bench --backend gateway --model google/gemma-4-26b-a4b-it --yes --only find-largest --only write-note`

| Metric | Value |
|---|---|
| Date | 2026-10-09 |
| Tasks passed | 2 / 2 (100%) |
| Skipped | 0 |
| Step p50 / p95 (ms) | 2476 / 3041 |
| Model p50 / p95 (ms) | 2422 / 2995 |
| Observe p50 / p95 (ms) | 43 / 58 |
| Act p50 / p95 (ms) | 3 / 4 |
| Seconds per task (mean) | 7.72 |
| Steps per task (mean) | 3.00 |
| Model calls per task (mean) | 3.00 |
| Prompt / completion tokens | 9517 / 382 |
| Cost (US$) | — |
| Total time (s) | 15.4 |
| A/B flags | all on |
| Machine | Intel(R) Xeon(R) Processor @ 2.10GHz · 4 cores · Ubuntu 24.04.5 LTS · x11 :97 1920x1080 |

| Task | Kind | Result | Judged by | Steps | Seconds | Step p50 ms | Model calls | Tokens in/out |
|---|---|---|---|---|---|---|---|---|
| write-note | Terminal | pass | check | 2 | 4.03 | 2517 | 2 | 3100 / 109 |
| find-largest | Terminal | pass | check | 4 | 11.41 | 2476 | 4 | 6417 / 273 |

Not measured on this run:

- cost: no prices given (--usd-per-mtok-in/--usd-per-mtok-out)
