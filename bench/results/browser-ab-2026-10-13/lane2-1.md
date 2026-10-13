# Kai bench: gemini+browser · gemma-4-26b-a4b-it

Live run on the real desktop. Pass is decided by each task's check command where it has one, otherwise by the run's own outcome. Browser lane ON (--browser --headless): each task in a fresh headless Kai browser.

Command: `kai bench --backend gemini --model gemma-4-26b-a4b-it --only fill-form --only web-form-submit --only local-price-lookup --only download-file --yes --no-record --browser --headless --out /tmp/claude-0/-home-user-KAI/0b4430f5-d64a-5ddd-8942-4121eb33d114/scratchpad/ab/lane2-1`

| Metric | Value |
|---|---|
| Date | 2026-10-13 |
| Tasks passed | 3 / 4 (75%) |
| Skipped | 0 |
| Step p50 / p95 (ms) | 2320 / 5933 |
| Model p50 / p95 (ms) | 2165 / 5803 |
| Observe p50 / p95 (ms) | 113 / 161 |
| Act p50 / p95 (ms) | 4 / 40 |
| Seconds per task (mean) | 24.64 |
| Steps per task (mean) | 4.50 |
| Model calls per task (mean) | 5.00 |
| Prompt / completion tokens | 39753 / 1472 |
| Cost (US$) | — |
| Failures | false_done 1 |
| False done / gated, never verified | 1 / 0 |
| DONE gate refusals | 0 |
| Total time (s) | 98.5 |
| A/B flags | all on |
| Executor | off |
| Machine | Intel(R) Xeon(R) Processor @ 2.10GHz · 4 cores · Ubuntu 24.04.5 LTS · x11 :91 1920x1080 |

| Task | Kind | Result | Judged by | Steps | Seconds | Step p50 ms | Model calls | Tokens in/out | Failure |
|---|---|---|---|---|---|---|---|---|---|
| fill-form | Gui | pass | check | 3 | 9.73 | 2652 | 3 | 5855 / 202 | — |
| web-form-submit | Gui | FAIL | check | 9 | 63.42 | 1764 | 9 | 18315 / 569 | false_done: said done (The form is already open and partially filled. I need to complete the task by clicking the 'Send message' button. However, the instructions say not) but the check failed |
| local-price-lookup | Mixed | pass | check | 3 | 18.07 | 3809 | 5 | 9050 / 465 | — |
| download-file | Mixed | pass | check | 3 | 7.32 | 2376 | 3 | 6533 / 236 | — |

Failure categories: false_done 1

Not measured on this run:

- cost: no prices given (--usd-per-mtok-in/--usd-per-mtok-out)
