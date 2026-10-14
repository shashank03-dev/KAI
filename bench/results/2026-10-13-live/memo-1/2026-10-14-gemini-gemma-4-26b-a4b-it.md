# Kai bench: gemini · gemma-4-26b-a4b-it

Live run on the real desktop. Pass is decided by each task's check command where it has one, otherwise by the run's own outcome.

Command: `kai bench --backend gemini --model gemma-4-26b-a4b-it --yes --no-record --memo --out bench/results/2026-10-13-live/memo-1 --only search-web --only fill-form --only web-form-submit --only download-file --only rename-files --only find-largest --only zip-folder --only open-folder --only mousepad-write --only close-app --only tip-calculator --only calendar-event --only resize-image --only screenshot --only local-price-lookup --only csv-total --only email-draft --only spreadsheet-budget`

| Metric | Value |
|---|---|
| Date | 2026-10-14 |
| Tasks passed | 14 / 18 (78%) |
| Skipped | 0 |
| Step p50 / p95 (ms) | 2998 / 7170 |
| Model p50 / p95 (ms) | 2493 / 6754 |
| Observe p50 / p95 (ms) | 84 / 636 |
| Act p50 / p95 (ms) | 4 / 407 |
| Seconds per task (mean) | 34.65 |
| Steps per task (mean) | 5.56 |
| Model calls per task (mean) | 6.39 |
| Prompt / completion tokens | 263815 / 11960 |
| Cost (US$) | — |
| Failures | gave_up 2 · false_done 1 · malformed 1 |
| False done / gated, never verified | 1 / 0 |
| DONE gate refusals | 0 |
| Total time (s) | 623.7 |
| A/B flags | --memo |
| Executor | off |
| Machine | Intel(R) Xeon(R) Processor @ 2.10GHz · 4 cores · Ubuntu 24.04.5 LTS · x11 :99 1920x1080 |

| Task | Kind | Result | Judged by | Steps | Seconds | Step p50 ms | Model calls | Tokens in/out | Failure |
|---|---|---|---|---|---|---|---|---|---|
| search-web | Gui | FAIL | check | 16 | 72.36 | 2162 | 12 | 28949 / 772 | false_done: said done (I have successfully navigated to the Gemma 4 model card page.) but the check failed |
| fill-form | Gui | pass | check | 4 | 20.50 | 2275 | 4 | 8575 / 297 | — |
| web-form-submit | Gui | pass | check | 12 | 76.20 | 3101 | 13 | 28921 / 1160 | — |
| local-price-lookup | Mixed | pass | check | 3 | 27.14 | 3870 | 4 | 7671 / 356 | — |
| download-file | Mixed | pass | check | 2 | 3.78 | 2459 | 2 | 4584 / 114 | — |
| rename-files | Terminal | pass | check | 5 | 14.71 | 2146 | 6 | 13596 / 496 | — |
| find-largest | Terminal | pass | check | 4 | 38.77 | 3843 | 4 | 10660 / 531 | — |
| zip-folder | Terminal | pass | check | 4 | 9.21 | 2118 | 4 | 9116 / 285 | — |
| open-folder | Gui | pass | check | 3 | 9.97 | 2060 | 4 | 8592 / 321 | — |
| mousepad-write | Gui | pass | check | 24 | 169.33 | 5019 | 29 | 56459 / 2896 | — |
| close-app | Gui | FAIL | check | 1 | 6.41 | 6406 | 1 | 1767 / 89 | gave_up: asked the user: I don't see a text editor window open in the screenshot. There is a browser window with a session restore dialog. I will ask for |
| spreadsheet-budget | Gui | FAIL | check | 6 | 50.76 | 4695 | 13 | 41936 / 1043 | malformed: 3 undecodable replies; last: malformed reply at step 8: action `Type` requires a `target` element number |
| csv-total | Mixed | pass | check | 4 | 12.73 | 2325 | 5 | 11549 / 431 | — |
| tip-calculator | Mixed | pass | check | 3 | 81.47 | 3250 | 5 | 9859 / 2149 | — |
| email-draft | Mixed | FAIL | check | 3 | 7.71 | 2421 | 2 | 4407 / 183 | gave_up: asked the user: The user wants to draft an email, but I am currently on a Gmail landing page, not in a compose window, and I am not |
| calendar-event | Mixed | pass | check | 2 | 12.28 | 9954 | 3 | 8140 / 495 | — |
| resize-image | Mixed | pass | check | 2 | 5.09 | 2581 | 2 | 5219 / 167 | — |
| screenshot | Mixed | pass | check | 2 | 5.23 | 2788 | 2 | 3815 / 175 | — |

Failure categories: gave_up 2 · false_done 1 · malformed 1

Not measured on this run:

- cost: no prices given (--usd-per-mtok-in/--usd-per-mtok-out)
