# Kai bench: gemini+browser · gemma-4-26b-a4b-it

Live run on the real desktop. Pass is decided by each task's check command where it has one, otherwise by the run's own outcome. Browser lane ON (--browser --headless): each task in a fresh headless Kai browser.

Command: `kai bench --backend gemini --model gemma-4-26b-a4b-it --only fill-form --only web-form-submit --only local-price-lookup --only download-file --yes --no-record --browser --headless --out /tmp/claude-0/-home-user-KAI/0b4430f5-d64a-5ddd-8942-4121eb33d114/scratchpad/ab/lane5-1`

| Metric | Value |
|---|---|
| Date | 2026-10-13 |
| Tasks passed | 4 / 4 (100%) |
| Skipped | 0 |
| Step p50 / p95 (ms) | 2002 / 3950 |
| Model p50 / p95 (ms) | 1880 / 3781 |
| Observe p50 / p95 (ms) | 110 / 154 |
| Act p50 / p95 (ms) | 24 / 42 |
| Seconds per task (mean) | 23.16 |
| Steps per task (mean) | 4.75 |
| Model calls per task (mean) | 5.25 |
| Prompt / completion tokens | 42539 / 1488 |
| Cost (US$) | — |
| Failures | none |
| False done / gated, never verified | 0 / 0 |
| DONE gate refusals | 0 |
| Total time (s) | 92.6 |
| A/B flags | all on |
| Executor | off |
| Machine | Intel(R) Xeon(R) Processor @ 2.10GHz · 4 cores · Ubuntu 24.04.5 LTS · x11 :91 1920x1080 |

| Task | Kind | Result | Judged by | Steps | Seconds | Step p50 ms | Model calls | Tokens in/out | Failure |
|---|---|---|---|---|---|---|---|---|---|
| fill-form | Gui | pass | check | 3 | 7.16 | 1968 | 3 | 6092 / 136 | — |
| web-form-submit | Gui | pass | check | 10 | 27.37 | 1973 | 10 | 20864 / 653 | — |
| local-price-lookup | Mixed | pass | check | 3 | 49.76 | 3950 | 5 | 9050 / 465 | — |
| download-file | Mixed | pass | check | 3 | 8.34 | 2176 | 3 | 6533 / 234 | — |

Failure categories: none

Not measured on this run:

- cost: no prices given (--usd-per-mtok-in/--usd-per-mtok-out)
