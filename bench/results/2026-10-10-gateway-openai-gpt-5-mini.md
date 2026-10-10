# Kai bench: gateway · openai/gpt-5-mini

Live run on the real desktop. Pass is decided by each task's check command where it has one, otherwise by the run's own outcome.

Command: `kai bench --yes --backend gateway --model openai/gpt-5-mini --usd-per-mtok-in 0.25 --usd-per-mtok-out 2.0 --only search-web --only fill-form --only web-form-submit --only download-file --only rename-files --only find-largest --only zip-folder --only open-folder --only mousepad-write --only close-app --only tip-calculator --only calendar-event --only resize-image --only screenshot`

| Metric | Value |
|---|---|
| Date | 2026-10-10 |
| Tasks passed | 12 / 14 (86%) |
| Skipped | 0 |
| Step p50 / p95 (ms) | 2259 / 4743 |
| Model p50 / p95 (ms) | 2165 / 4605 |
| Observe p50 / p95 (ms) | 46 / 193 |
| Act p50 / p95 (ms) | 7 / 186 |
| Seconds per task (mean) | 164.62 |
| Steps per task (mean) | 15.14 |
| Model calls per task (mean) | 11.64 |
| Prompt / completion tokens | 347740 / 25490 |
| Cost (US$) | $0.1379 |
| Total time (s) | 2304.6 |
| A/B flags | all on |
| Machine | Intel(R) Xeon(R) Processor @ 2.10GHz · 4 cores · Ubuntu 24.04.5 LTS · x11 :97 1920x1080 |

| Task | Kind | Result | Judged by | Steps | Seconds | Step p50 ms | Model calls | Tokens in/out |
|---|---|---|---|---|---|---|---|---|
| search-web | Gui | pass | check | 29 | 132.03 | 1575 | 18 | 38867 / 2428 |
| fill-form | Gui | pass | check | 1 | 5.13 | 2291 | 2 | 3696 / 364 |
| web-form-submit | Gui | FAIL | check | 24 | 174.76 | 2004 | 18 | 37702 / 2489 |
| download-file | Mixed | pass | check | 16 | 118.60 | 1974 | 11 | 25787 / 1712 |
| rename-files | Terminal | pass | check | 11 | 302.97 | 2529 | 12 | 27993 / 2036 |
| find-largest | Terminal | pass | check | 5 | 15.19 | 2427 | 5 | 9879 / 711 |
| zip-folder | Terminal | pass | check | 40 | 561.62 | 3157 | 36 | 94410 / 7412 |
| open-folder | Gui | pass | check | 9 | 154.13 | 1718 | 6 | 13259 / 823 |
| mousepad-write | Gui | FAIL | check | 9 | 181.77 | 3034 | 9 | 18866 / 1341 |
| close-app | Gui | pass | check | 10 | 103.38 | 2259 | 10 | 18875 / 935 |
| tip-calculator | Mixed | pass | check | 2 | 5.95 | 3659 | 2 | 3828 / 296 |
| calendar-event | Mixed | pass | check | 12 | 49.68 | 2921 | 8 | 14285 / 1382 |
| resize-image | Mixed | pass | check | 4 | 128.68 | 1860 | 4 | 6433 / 459 |
| screenshot | Mixed | pass | check | 40 | 370.74 | 1808 | 22 | 33860 / 3102 |

Not measured on this run:

- tokens: only 162 of 163 calls reported usage; sums cover those
