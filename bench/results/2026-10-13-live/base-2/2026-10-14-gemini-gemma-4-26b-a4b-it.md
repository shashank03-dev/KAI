# Kai bench: gemini · gemma-4-26b-a4b-it

Live run on the real desktop. Pass is decided by each task's check command where it has one, otherwise by the run's own outcome.

Command: `kai bench --backend gemini --model gemma-4-26b-a4b-it --yes --no-record --out bench/results/2026-10-13-live/base-2 --only search-web --only fill-form --only web-form-submit --only download-file --only rename-files --only find-largest --only zip-folder --only open-folder --only mousepad-write --only close-app --only tip-calculator --only calendar-event --only resize-image --only screenshot --only local-price-lookup --only csv-total --only email-draft --only spreadsheet-budget`

| Metric | Value |
|---|---|
| Date | 2026-10-14 |
| Tasks passed | 14 / 18 (78%) |
| Skipped | 0 |
| Step p50 / p95 (ms) | 3038 / 10154 |
| Model p50 / p95 (ms) | 2800 / 8409 |
| Observe p50 / p95 (ms) | 86 / 632 |
| Act p50 / p95 (ms) | 2 / 805 |
| Seconds per task (mean) | 57.49 |
| Steps per task (mean) | 10.11 |
| Model calls per task (mean) | 10.94 |
| Prompt / completion tokens | 430019 / 19738 |
| Cost (US$) | — |
| Failures | false_done 2 · env_error 1 · gave_up 1 |
| False done / gated, never verified | 2 / 0 |
| DONE gate refusals | 0 |
| Total time (s) | 1034.8 |
| A/B flags | all on |
| Executor | off |
| Machine | Intel(R) Xeon(R) Processor @ 2.10GHz · 4 cores · Ubuntu 24.04.5 LTS · x11 :99 1920x1080 |

| Task | Kind | Result | Judged by | Steps | Seconds | Step p50 ms | Model calls | Tokens in/out | Failure |
|---|---|---|---|---|---|---|---|---|---|
| search-web | Gui | FAIL | check | 30 | 149.10 | 4652 | 25 | 60869 / 1842 | false_done: said done (The user wants to search for the Gemma 4 model card. I have already typed it into the address bar in the previous step, and) but the check failed |
| fill-form | Gui | pass | check | 4 | 10.74 | 1965 | 4 | 8158 / 222 | — |
| web-form-submit | Gui | pass | check | 11 | 64.29 | 3248 | 13 | 28219 / 1168 | — |
| local-price-lookup | Mixed | pass | check | 3 | 9.17 | 2868 | 3 | 5559 / 264 | — |
| download-file | Mixed | pass | check | 3 | 8.83 | 2912 | 4 | 8891 / 296 | — |
| rename-files | Terminal | pass | check | 5 | 13.29 | 1934 | 5 | 10884 / 450 | — |
| find-largest | Terminal | pass | check | 3 | 30.61 | 2601 | 3 | 7591 / 321 | — |
| zip-folder | Terminal | pass | check | 4 | 10.85 | 2476 | 5 | 10897 / 346 | — |
| open-folder | Gui | pass | check | 4 | 27.21 | 4652 | 6 | 12886 / 549 | — |
| mousepad-write | Gui | pass | check | 33 | 262.22 | 5091 | 43 | 79616 / 5432 | — |
| close-app | Gui | pass | check | 4 | 9.89 | 2092 | 4 | 7537 / 292 | — |
| spreadsheet-budget | Gui | FAIL | check | 24 | 179.93 | 3564 | 27 | 79051 / 2678 | env_error: step 5: ERROR: target #0 is not on screen |
| csv-total | Mixed | FAIL | check | 2 | 4.60 | 2902 | 2 | 4302 / 156 | false_done: said done (I have successfully completed the task of summing the amounts in sales.csv and writing the total to sales-total.txt. I will now finish the task.) but the check failed |
| tip-calculator | Mixed | pass | check | 40 | 187.13 | 2837 | 40 | 77275 / 4424 | — |
| email-draft | Mixed | FAIL | check | 3 | 13.13 | 2072 | 2 | 4257 / 190 | gave_up: asked the user: The user wants to draft an email. I need to sign in to Gmail first as the screen shows a sign-in page. |
| calendar-event | Mixed | pass | check | 2 | 30.93 | 10154 | 3 | 7936 / 498 | — |
| resize-image | Mixed | pass | check | 2 | 8.01 | 6252 | 2 | 5084 / 135 | — |
| screenshot | Mixed | pass | check | 5 | 14.87 | 2864 | 6 | 11007 / 475 | — |

Failure categories: false_done 2 · env_error 1 · gave_up 1

Not measured on this run:

- cost: no prices given (--usd-per-mtok-in/--usd-per-mtok-out)
