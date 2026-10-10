# Executor A/B — Gemma 4 26B alone vs Gemma 4 26B + jevos-v4 (2026-10-10)

Same 14 tasks, same build (bbd8e2e), same container (4 vCPU, Xvfb), Gemini API free tier (two keys), jevos-v4 on 127.0.0.1:8017 (CPU). One run each. Console summaries copied verbatim from `kai bench` (the JSON files were overwritten by a script error; per-task recordings are in bench/runs/{exec,base}, not committed).

## With --executor

```
task                   result    steps     secs   p50 ms   p95 ms  calls tokens in/out
search-web             pass         10    65.35     3458     8031      8     20628/567
fill-form              pass          5    26.86     3841    10217      4      8314/427
web-form-submit        FAIL         11    92.17     2396     5568     11    23379/1112
download-file          pass          2    10.73     8976     8976      3      6418/317
rename-files           FAIL          1   110.48     8172     8172      9    17256/1434
find-largest           pass          3    17.27     2474     3764      5      8951/529
zip-folder             pass          3    49.78     2323     4793      5     10533/630
open-folder            pass          8    65.50     2594     5328      9    19376/1078
mousepad-write         FAIL          8   131.87     3214     5577     15    25639/1745
close-app              pass         38   261.16     1795     3182     40    66773/1723
tip-calculator         pass          3     8.06     2124     3132      3      5482/249
calendar-event         pass          3    58.67     5326     8672      6      9402/614
resize-image           pass          3     7.34     1881     3300      3      5496/223
screenshot             pass          2     3.78     2122     2122      2       3524/97

11 / 14 (79%) passed, 0 skipped · step p50 2145 ms · p95 7436 ms · model p50 2031 ms · 909.0 s total · gemini · gemma-4-26b-a4b-it
failures: rate_limited 2 · false_done 1
  web-form-submit        false_done: said done (The contact form has been submitted successfully, as indicated by the 'Thank you, message received' text on the screen.) but the check failed
failures: rate_limited 2 · false_done 1
  web-form-submit        false_done: said done (The contact form has been submitted successfully, as indicated by the 'Thank you, message received' text on the screen.) but the check failed
```

## Without (baseline)

```
task                   result    steps     secs   p50 ms   p95 ms  calls tokens in/out
search-web             pass         12   112.08     3350     8504     12    26235/1349
fill-form              pass          4    12.96     3619     4258      4      7147/282
web-form-submit        pass         10   107.17     3624     5303     13     22503/865
download-file          pass          2     9.11     7840     7840      3      5809/281
rename-files           pass          5    69.87     1643    14613      8     13233/480
find-largest           pass          2    34.97     2291     2291      2      3162/121
zip-folder             pass          3     9.72     2380     5395      4      7549/328
open-folder            pass          6   108.80     4554     8204     12    24056/1747
mousepad-write         pass         18   197.01     4874     9954     32    53283/3651
close-app              FAIL          0    44.87        0        0      6      9274/880
tip-calculator         pass          3    15.81     4839     7960      5      8125/490
calendar-event         pass          4    39.98     5040     8720      6     10126/562
resize-image           pass          3    12.28     3283     6066      4      6519/329
screenshot             pass         39   299.97     3639    10966     42    67340/4175

13 / 14 (93%) passed, 0 skipped · step p50 3619 ms · p95 10966 ms · model p50 3030 ms · 1074.6 s total · gemini · gemma-4-26b-a4b-it
failures: malformed 1
  close-app              malformed: 3 undecodable replies; last: malformed reply at step 2: action `Key` requires at least one key in `keys`
failures: malformed 1
  close-app              malformed: 3 undecodable replies; last: malformed reply at step 2: action `Key` requires at least one key in `keys`
```
