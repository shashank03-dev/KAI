# Kai bench: gemini · gemma-4-26b-a4b-it

Live run on the real desktop. Pass is decided by each task's check command where it has one, otherwise by the run's own outcome.

Command: `kai bench --yes --backend gemini --model gemma-4-26b-a4b-it`

| Metric | Value |
|---|---|
| Date | 2026-10-10 |
| Tasks passed | 23 / 30 (77%) |
| Skipped | 0 |
| Step p50 / p95 (ms) | 3108 / 8057 |
| Model p50 / p95 (ms) | 2852 / 6467 |
| Observe p50 / p95 (ms) | 83 / 305 |
| Act p50 / p95 (ms) | 1 / 366 |
| Seconds per task (mean) | 105.46 |
| Steps per task (mean) | 6.80 |
| Model calls per task (mean) | 10.13 |
| Prompt / completion tokens | 568889 / 26276 |
| Cost (US$) | — |
| Total time (s) | 3163.8 |
| A/B flags | all on |
| Machine | Intel(R) Xeon(R) Processor @ 2.10GHz · 4 cores · Ubuntu 24.04.5 LTS · x11 :97 1920x1080 |

| Task | Kind | Result | Judged by | Steps | Seconds | Step p50 ms | Model calls | Tokens in/out |
|---|---|---|---|---|---|---|---|---|
| search-web | Gui | pass | check | 12 | 80.28 | 3385 | 12 | 26607 / 1119 |
| open-site-find | Gui | pass | check | 37 | 303.34 | 2819 | 40 | 82972 / 3701 |
| play-video | Gui | pass | check | 37 | 367.41 | 3495 | 41 | 84476 / 3475 |
| fill-form | Gui | pass | check | 5 | 25.02 | 4442 | 5 | 8834 / 369 |
| web-form-submit | Gui | pass | check | 10 | 75.24 | 2421 | 12 | 22202 / 793 |
| local-price-lookup | Mixed | FAIL | check | 1 | 65.25 | 6005 | 7 | 10630 / 873 |
| download-file | Mixed | pass | check | 3 | 9.36 | 2551 | 4 | 8036 / 315 |
| write-note | Terminal | pass | check | 2 | 37.49 | 4911 | 3 | 5016 / 230 |
| rename-files | Terminal | pass | check | 5 | 14.17 | 1915 | 6 | 11653 / 468 |
| find-largest | Terminal | pass | check | 3 | 50.97 | 2683 | 5 | 6964 / 304 |
| sort-downloads | Terminal | pass | check | 5 | 57.93 | 1653 | 7 | 12122 / 563 |
| search-in-files | Terminal | pass | check | 4 | 67.77 | 4649 | 12 | 18956 / 907 |
| zip-folder | Terminal | pass | check | 2 | 3.78 | 2133 | 2 | 3910 / 111 |
| extract-archive | Terminal | pass | check | 3 | 51.20 | 1619 | 6 | 9688 / 289 |
| open-folder | Gui | pass | check | 7 | 66.93 | 2579 | 10 | 18186 / 860 |
| mousepad-write | Gui | pass | check | 11 | 232.70 | 3404 | 14 | 22548 / 1255 |
| close-app | Gui | pass | check | 8 | 63.91 | 3588 | 9 | 14783 / 901 |
| mousepad-line-numbers | Gui | FAIL | check | 6 | 747.88 | 182728 | 9 | 14868 / 774 |
| spreadsheet-budget | Gui | FAIL | check | 12 | 297.63 | 4035 | 21 | 45117 / 2460 |
| csv-total | Mixed | FAIL | check | 3 | 54.43 | 3211 | 6 | 9911 / 386 |
| tip-calculator | Mixed | pass | check | 1 | 63.96 | 8520 | 8 | 13908 / 1311 |
| email-draft | Mixed | FAIL | check | 3 | 8.52 | 3638 | 2 | 3876 / 133 |
| calendar-event | Mixed | pass | check | 3 | 48.29 | 3646 | 8 | 14231 / 585 |
| resize-image | Mixed | pass | check | 2 | 7.58 | 5639 | 3 | 5113 / 264 |
| screenshot | Mixed | pass | check | 3 | 53.89 | 2192 | 6 | 8378 / 404 |
| dark-mode | Mixed | FAIL | check | 3 | 115.72 | 3661 | 16 | 28061 / 1070 |
| disk-space | Terminal | pass | check | 2 | 8.32 | 6675 | 3 | 5961 / 302 |
| count-errors | Terminal | pass | check | 2 | 7.46 | 4025 | 2 | 3873 / 287 |
| run-script | Terminal | pass | check | 3 | 47.84 | 1659 | 4 | 5747 / 140 |
| pc-checkup | Terminal | FAIL | outcome | 6 | 129.60 | 2185 | 21 | 42262 / 1627 |

Not measured on this run:

- tokens: only 287 of 304 calls reported usage; sums cover those
- cost: no prices given (--usd-per-mtok-in/--usd-per-mtok-out)
