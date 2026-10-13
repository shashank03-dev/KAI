# Kai bench: gemini+browser · gemma-4-26b-a4b-it

Live run on the real desktop. Pass is decided by each task's check command where it has one, otherwise by the run's own outcome. Browser lane ON (--browser --headless): each task in a fresh headless Kai browser.

Command: `kai bench --backend gemini --model gemma-4-26b-a4b-it --only fill-form --only web-form-submit --only local-price-lookup --only download-file --yes --no-record --browser --headless --out /tmp/claude-0/-home-user-KAI/0b4430f5-d64a-5ddd-8942-4121eb33d114/scratchpad/ab/lane5-2`

| Metric | Value |
|---|---|
| Date | 2026-10-13 |
| Tasks passed | 4 / 4 (100%) |
| Skipped | 0 |
| Step p50 / p95 (ms) | 1965 / 4938 |
| Model p50 / p95 (ms) | 1820 / 4748 |
| Observe p50 / p95 (ms) | 114 / 166 |
| Act p50 / p95 (ms) | 23 / 41 |
| Seconds per task (mean) | 21.27 |
| Steps per task (mean) | 4.75 |
| Model calls per task (mean) | 5.25 |
| Prompt / completion tokens | 42539 / 1488 |
| Cost (US$) | — |
| Failures | none |
| False done / gated, never verified | 0 / 0 |
| DONE gate refusals | 0 |
| Total time (s) | 85.1 |
| A/B flags | all on |
| Executor | off |
| Machine | Intel(R) Xeon(R) Processor @ 2.10GHz · 4 cores · Ubuntu 24.04.5 LTS · x11 :92 1920x1080 |

| Task | Kind | Result | Judged by | Steps | Seconds | Step p50 ms | Model calls | Tokens in/out | Failure |
|---|---|---|---|---|---|---|---|---|---|
| fill-form | Gui | pass | check | 3 | 8.44 | 1652 | 3 | 6092 / 145 | — |
| web-form-submit | Gui | pass | check | 10 | 56.77 | 1940 | 10 | 20864 / 653 | — |
| local-price-lookup | Mixed | pass | check | 3 | 12.90 | 3970 | 5 | 9050 / 465 | — |
| download-file | Mixed | pass | check | 3 | 6.97 | 2249 | 3 | 6533 / 225 | — |

Failure categories: none

Not measured on this run:

- cost: no prices given (--usd-per-mtok-in/--usd-per-mtok-out)
