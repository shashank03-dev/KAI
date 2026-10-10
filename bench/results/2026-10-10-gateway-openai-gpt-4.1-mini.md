# Kai bench: gateway · openai/gpt-4.1-mini

Live run on the real desktop. Pass is decided by each task's check command where it has one, otherwise by the run's own outcome.

Command: `kai bench --yes --backend gateway --model openai/gpt-4.1-mini --usd-per-mtok-in 0.4 --usd-per-mtok-out 1.6 --only search-web --only fill-form --only web-form-submit --only download-file --only rename-files --only find-largest --only zip-folder --only open-folder --only mousepad-write --only close-app --only tip-calculator --only calendar-event --only resize-image --only screenshot`

| Metric | Value |
|---|---|
| Date | 2026-10-10 |
| Tasks passed | 8 / 14 (57%) |
| Skipped | 0 |
| Step p50 / p95 (ms) | 1888 / 3691 |
| Model p50 / p95 (ms) | 1770 / 3385 |
| Observe p50 / p95 (ms) | 61 / 632 |
| Act p50 / p95 (ms) | 3 / 328 |
| Seconds per task (mean) | 123.19 |
| Steps per task (mean) | 11.07 |
| Model calls per task (mean) | 8.07 |
| Prompt / completion tokens | 251375 / 10534 |
| Cost (US$) | $0.1174 |
| Total time (s) | 1724.7 |
| A/B flags | all on |
| Machine | Intel(R) Xeon(R) Processor @ 2.10GHz · 4 cores · Ubuntu 24.04.5 LTS · x11 :97 1920x1080 |

| Task | Kind | Result | Judged by | Steps | Seconds | Step p50 ms | Model calls | Tokens in/out |
|---|---|---|---|---|---|---|---|---|
| search-web | Gui | FAIL | check | 40 | 125.75 | 393 | 19 | 53817 / 2236 |
| fill-form | Gui | pass | check | 40 | 522.11 | 1597 | 27 | 52899 / 2195 |
| web-form-submit | Gui | FAIL | check | 5 | 10.20 | 1651 | 4 | 9791 / 348 |
| download-file | Mixed | pass | check | 5 | 10.69 | 2395 | 4 | 8412 / 368 |
| rename-files | Terminal | FAIL | check | 2 | 154.67 | 2567 | 2 | 5050 / 160 |
| find-largest | Terminal | pass | check | 4 | 5.55 | 1217 | 3 | 4850 / 282 |
| zip-folder | Terminal | pass | check | 4 | 5.70 | 1196 | 3 | 5734 / 256 |
| open-folder | Gui | FAIL | check | 3 | 7.80 | 2304 | 3 | 7598 / 221 |
| mousepad-write | Gui | FAIL | check | 21 | 373.47 | 2824 | 22 | 49105 / 2180 |
| close-app | Gui | pass | check | 17 | 330.48 | 1621 | 15 | 29867 / 1159 |
| tip-calculator | Mixed | pass | check | 4 | 7.40 | 2171 | 3 | 6588 / 311 |
| calendar-event | Mixed | FAIL | check | 5 | 13.12 | 2626 | 4 | 9019 / 452 |
| resize-image | Mixed | pass | check | 3 | 154.25 | 2190 | 2 | 4369 / 203 |
| screenshot | Mixed | pass | check | 2 | 3.51 | 1705 | 2 | 4276 / 163 |
