# Kai bench: mock · recorded

Mock backend: recorded replies on a scripted screen with a dry-run actuator. Measures harness overhead only. No model was called and nothing on the machine was touched.

Command: `kai bench --backend mock`

| Metric | Value |
|---|---|
| Date | 2026-10-09 |
| Tasks passed | 30 / 30 (100%) |
| Skipped | 0 |
| Step p50 / p95 (ms) | 0 / 0 |
| Model p50 / p95 (ms) | 0 / 0 |
| Observe p50 / p95 (ms) | 0 / 0 |
| Act p50 / p95 (ms) | 0 / 0 |
| Seconds per task (mean) | 0.00 |
| Steps per task (mean) | 2.97 |
| Model calls per task (mean) | 2.83 |
| Prompt / completion tokens | — / — |
| Cost (US$) | — |
| Total time (s) | 0.0 |
| A/B flags | all on (engine has no switches; defaults) |
| Machine | Intel(R) Xeon(R) Processor @ 2.10GHz · 4 cores · Ubuntu 24.04.5 LTS · x11 none |

| Task | Kind | Result | Judged by | Steps | Seconds | Step p50 ms | Model calls | Tokens in/out |
|---|---|---|---|---|---|---|---|---|
| search-web | Gui | pass | outcome | 5 | 0.00 | 0 | 4 | — / — |
| open-site-find | Gui | pass | outcome | 4 | 0.00 | 0 | 3 | — / — |
| play-video | Gui | pass | outcome | 3 | 0.00 | 0 | 3 | — / — |
| fill-form | Gui | pass | outcome | 3 | 0.00 | 0 | 3 | — / — |
| web-form-submit | Gui | pass | outcome | 4 | 0.00 | 0 | 3 | — / — |
| local-price-lookup | Mixed | pass | outcome | 3 | 0.00 | 0 | 3 | — / — |
| download-file | Mixed | pass | outcome | 2 | 0.02 | 0 | 2 | — / — |
| write-note | Terminal | pass | outcome | 2 | 0.00 | 0 | 2 | — / — |
| rename-files | Terminal | pass | outcome | 3 | 0.00 | 0 | 3 | — / — |
| find-largest | Terminal | pass | outcome | 3 | 0.00 | 0 | 3 | — / — |
| sort-downloads | Terminal | pass | outcome | 2 | 0.00 | 0 | 2 | — / — |
| search-in-files | Terminal | pass | outcome | 3 | 0.00 | 0 | 3 | — / — |
| zip-folder | Terminal | pass | outcome | 2 | 0.00 | 0 | 2 | — / — |
| extract-archive | Terminal | pass | outcome | 3 | 0.00 | 0 | 3 | — / — |
| open-folder | Gui | pass | outcome | 2 | 0.00 | 0 | 2 | — / — |
| mousepad-write | Gui | pass | outcome | 7 | 0.00 | 0 | 7 | — / — |
| close-app | Gui | pass | outcome | 3 | 0.00 | 0 | 3 | — / — |
| mousepad-line-numbers | Gui | pass | outcome | 2 | 0.00 | 0 | 2 | — / — |
| spreadsheet-budget | Gui | pass | outcome | 8 | 0.00 | 0 | 8 | — / — |
| csv-total | Mixed | pass | outcome | 3 | 0.00 | 0 | 3 | — / — |
| tip-calculator | Mixed | pass | outcome | 2 | 0.00 | 0 | 2 | — / — |
| email-draft | Mixed | pass | outcome | 3 | 0.00 | 0 | 2 | — / — |
| calendar-event | Mixed | pass | outcome | 2 | 0.00 | 0 | 2 | — / — |
| resize-image | Mixed | pass | outcome | 2 | 0.00 | 0 | 2 | — / — |
| screenshot | Mixed | pass | outcome | 2 | 0.00 | 0 | 2 | — / — |
| dark-mode | Mixed | pass | outcome | 2 | 0.00 | 0 | 2 | — / — |
| disk-space | Terminal | pass | outcome | 3 | 0.00 | 0 | 3 | — / — |
| count-errors | Terminal | pass | outcome | 2 | 0.00 | 0 | 2 | — / — |
| run-script | Terminal | pass | outcome | 2 | 0.00 | 0 | 2 | — / — |
| pc-checkup | Terminal | pass | outcome | 2 | 0.00 | 0 | 2 | — / — |

Not measured on this run:

- tokens and cost: none (recorded replies, no model was called)
