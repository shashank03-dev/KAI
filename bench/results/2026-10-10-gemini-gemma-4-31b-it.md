# Kai bench: gemini · gemma-4-31b-it

Live run on the real desktop. Pass is decided by each task's check command where it has one, otherwise by the run's own outcome.

Command: `kai bench --yes --backend gemini --model gemma-4-31b-it --only search-web --only fill-form --only web-form-submit --only download-file --only rename-files --only find-largest --only zip-folder --only open-folder --only mousepad-write --only close-app --only tip-calculator --only calendar-event --only resize-image --only screenshot`

| Metric | Value |
|---|---|
| Date | 2026-10-10 |
| Tasks passed | 11 / 14 (79%) |
| Skipped | 0 |
| Step p50 / p95 (ms) | 27561 / 43290 |
| Model p50 / p95 (ms) | 27210 / 43235 |
| Observe p50 / p95 (ms) | 76 / 641 |
| Act p50 / p95 (ms) | 4 / 323 |
| Seconds per task (mean) | 218.29 |
| Steps per task (mean) | 4.29 |
| Model calls per task (mean) | 4.86 |
| Prompt / completion tokens | 112959 / 7267 |
| Cost (US$) | — |
| Total time (s) | 3056.1 |
| A/B flags | all on |
| Machine | Intel(R) Xeon(R) Processor @ 2.10GHz · 4 cores · Ubuntu 24.04.5 LTS · x11 :97 1920x1080 |

| Task | Kind | Result | Judged by | Steps | Seconds | Step p50 ms | Model calls | Tokens in/out |
|---|---|---|---|---|---|---|---|---|
| search-web | Gui | pass | check | 7 | 374.62 | 22483 | 7 | 14966 / 841 |
| fill-form | Gui | pass | check | 4 | 72.80 | 18974 | 3 | 5311 / 300 |
| web-form-submit | Gui | FAIL | check | 2 | 265.88 | 29371 | 7 | 11640 / 1050 |
| download-file | Mixed | pass | check | 2 | 50.55 | 29693 | 2 | 3866 / 177 |
| rename-files | Terminal | pass | check | 5 | 116.14 | 28446 | 4 | 7684 / 410 |
| find-largest | Terminal | pass | check | 2 | 152.56 | 38630 | 2 | 3170 / 162 |
| zip-folder | Terminal | pass | check | 2 | 90.84 | 38378 | 2 | 3786 / 140 |
| open-folder | Gui | pass | check | 13 | 612.61 | 32258 | 11 | 17286 / 1072 |
| mousepad-write | Gui | pass | check | 10 | 618.87 | 32788 | 12 | 17614 / 1066 |
| close-app | Gui | FAIL | check | 3 | 161.18 | 27561 | 6 | 9894 / 978 |
| tip-calculator | Mixed | pass | check | 2 | 92.61 | 23464 | 2 | 3181 / 181 |
| calendar-event | Mixed | FAIL | check | 2 | 194.81 | 60884 | 4 | 4919 / 428 |
| resize-image | Mixed | pass | check | 2 | 104.40 | 39869 | 2 | 3177 / 144 |
| screenshot | Mixed | pass | check | 4 | 148.20 | 24173 | 4 | 6465 / 318 |

Not measured on this run:

- tokens: only 63 of 68 calls reported usage; sums cover those
- cost: no prices given (--usd-per-mtok-in/--usd-per-mtok-out)
