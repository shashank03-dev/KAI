# Kai bench: replay · recorded

Replay: each task's recorded replies executed for real on this desktop (real capture, accessibility tree, input and shell), judged by the task's check command. Proves Kai's hands and the tasks' setup/check, not a model. Tasks marked `replay: terminal` reach the end state from the terminal; `replay: skip` tasks are skipped.

Command: `kai bench --backend replay --yes`

| Metric | Value |
|---|---|
| Date | 2026-10-09 |
| Tasks passed | 25 / 26 (96%) |
| Skipped | 4 |
| Step p50 / p95 (ms) | 628 / 2159 |
| Model p50 / p95 (ms) | 0 / 0 |
| Observe p50 / p95 (ms) | 560 / 736 |
| Act p50 / p95 (ms) | 6 / 1647 |
| Seconds per task (mean) | 2.91 |
| Steps per task (mean) | 2.85 |
| Model calls per task (mean) | 2.77 |
| Prompt / completion tokens | — / — |
| Cost (US$) | — |
| Total time (s) | 75.6 |
| A/B flags | all on (engine has no switches; defaults) |
| Machine | Intel(R) Xeon(R) Processor @ 2.10GHz · 4 cores · Ubuntu 24.04.5 LTS · x11 :98 1920x1080 |

| Task | Kind | Result | Judged by | Steps | Seconds | Step p50 ms | Model calls | Tokens in/out |
|---|---|---|---|---|---|---|---|---|
| search-web | Gui | skipped (not replayable (see `replay: skip` in tasks.yaml)) | skipped | 0 | 0.00 | 0 | 0 | — / — |
| open-site-find | Gui | skipped (not replayable (see `replay: skip` in tasks.yaml)) | skipped | 0 | 0.00 | 0 | 0 | — / — |
| play-video | Gui | skipped (not replayable (see `replay: skip` in tasks.yaml)) | skipped | 0 | 0.00 | 0 | 0 | — / — |
| fill-form | Gui | skipped (not replayable (see `replay: skip` in tasks.yaml)) | skipped | 0 | 0.00 | 0 | 0 | — / — |
| web-form-submit | Gui | pass | check | 4 | 3.04 | 591 | 3 | — / — |
| local-price-lookup | Mixed | pass | check | 3 | 3.16 | 718 | 3 | — / — |
| download-file | Mixed | pass | check | 2 | 1.89 | 729 | 2 | — / — |
| write-note | Terminal | pass | check | 2 | 1.98 | 752 | 2 | — / — |
| rename-files | Terminal | pass | check | 3 | 3.06 | 735 | 3 | — / — |
| find-largest | Terminal | pass | check | 3 | 2.50 | 557 | 3 | — / — |
| sort-downloads | Terminal | pass | check | 2 | 1.87 | 744 | 2 | — / — |
| search-in-files | Terminal | pass | check | 3 | 2.65 | 565 | 3 | — / — |
| zip-folder | Terminal | pass | check | 2 | 1.34 | 494 | 2 | — / — |
| extract-archive | Terminal | pass | check | 3 | 2.30 | 447 | 3 | — / — |
| open-folder | Gui | pass | check | 2 | 3.13 | 2420 | 2 | — / — |
| mousepad-write | Gui | pass | check | 7 | 9.05 | 530 | 7 | — / — |
| close-app | Gui | pass | check | 3 | 2.88 | 670 | 3 | — / — |
| mousepad-line-numbers | Gui | pass | check | 2 | 1.73 | 721 | 2 | — / — |
| spreadsheet-budget | Gui | FAIL | check | 8 | 15.91 | 1187 | 8 | — / — |
| csv-total | Mixed | pass | check | 3 | 1.23 | 117 | 3 | — / — |
| tip-calculator | Mixed | pass | check | 2 | 0.67 | 119 | 2 | — / — |
| email-draft | Mixed | pass | check | 3 | 1.19 | 146 | 2 | — / — |
| calendar-event | Mixed | pass | check | 2 | 1.31 | 704 | 2 | — / — |
| resize-image | Mixed | pass | check | 2 | 1.97 | 811 | 2 | — / — |
| screenshot | Mixed | pass | check | 2 | 2.02 | 881 | 2 | — / — |
| dark-mode | Mixed | pass | check | 2 | 1.88 | 746 | 2 | — / — |
| disk-space | Terminal | pass | check | 3 | 3.04 | 703 | 3 | — / — |
| count-errors | Terminal | pass | check | 2 | 1.66 | 591 | 2 | — / — |
| run-script | Terminal | pass | check | 2 | 2.52 | 1474 | 2 | — / — |
| pc-checkup | Terminal | pass | outcome | 2 | 1.60 | 628 | 2 | — / — |

Not measured on this run:

- tokens and cost: none (recorded replies, no model was called)
