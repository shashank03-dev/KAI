# Kai bench: local+browser · holo-3.1-4b-q4

Live run on the real desktop. Pass is decided by each task's check command where it has one, otherwise by the run's own outcome. Browser lane ON (--browser --headless): each task in a fresh headless Kai browser.

Command: `kai bench --backend local --model holo-3.1-4b-q4 --only fill-form --only web-form-submit --only local-price-lookup --only download-file --yes --no-record --browser --headless --out /tmp/claude-0/-home-user-KAI/0b4430f5-d64a-5ddd-8942-4121eb33d114/scratchpad/ab/holo-lane`

| Metric | Value |
|---|---|
| Date | 2026-10-13 |
| Tasks passed | 3 / 4 (75%) |
| Skipped | 0 |
| Step p50 / p95 (ms) | 46128 / 66187 |
| Model p50 / p95 (ms) | 45961 / 65931 |
| Observe p50 / p95 (ms) | 109 / 188 |
| Act p50 / p95 (ms) | 40 / 53 |
| Seconds per task (mean) | 249.39 |
| Steps per task (mean) | 6.00 |
| Model calls per task (mean) | 5.00 |
| Prompt / completion tokens | 48213 / 2882 |
| Cost (US$) | — |
| Failures | loop 1 |
| False done / gated, never verified | 0 / 0 |
| DONE gate refusals | 0 |
| Total time (s) | 997.6 |
| A/B flags | all on |
| Executor | off |
| Machine | Intel(R) Xeon(R) Processor @ 2.10GHz · 4 cores · Ubuntu 24.04.5 LTS · x11 :95 1920x1080 |

| Task | Kind | Result | Judged by | Steps | Seconds | Step p50 ms | Model calls | Tokens in/out | Failure |
|---|---|---|---|---|---|---|---|---|---|
| fill-form | Gui | pass | check | 4 | 152.08 | 42798 | 3 | 7185 / 351 | — |
| web-form-submit | Gui | FAIL | check | 15 | 609.27 | 46128 | 12 | 29524 / 1849 | loop: type #4 please send a copy of my march invoice repeated 7 times |
| local-price-lookup | Mixed | pass | check | 3 | 129.17 | 42425 | 3 | 6466 / 382 | — |
| download-file | Mixed | pass | check | 2 | 107.04 | 54804 | 2 | 5038 / 300 | — |

Failure categories: loop 1

Not measured on this run:

- cost: no prices given (--usd-per-mtok-in/--usd-per-mtok-out)
