# The browser lane

`kai run --browser "…"` (or, in the app, Settings › "Do tasks in Kai's own
browser") does a task in **Kai's own Chromium**, driven over the Chrome
DevTools Protocol (CDP), instead of through the desktop. Why, and what was
rejected: [RESEARCH_2026_10_VENDORS.md](RESEARCH_2026_10_VENDORS.md) §5.1.
Off unless asked for; Linux and macOS for now (Chrome, Chromium, Edge or Brave).

## In the app

The switch in Settings › Behaviour turns it on for every run. The app keeps
one browser for its whole session: it starts with the first run that wants
it and stays open between runs, so what Kai opened (a page, a video) is
still there when the run ends. Close its window any time; the next run
opens it again. Shell commands and app launches in such a run still go to
the desktop (`crates/kai-app/src/browser.rs`).

## Reading a page (`read_page`)

Reading tasks ("what does the Blue Kettle cost?") fail when the answer is
in plain text the element list does not carry: the list holds controls,
and the small screenshot is hard to read. Anthropic's browser tool has
`get_page_text` for this. Kai keeps the model's job small instead of adding
a tool: in the lane the system prompt offers one optional reply field,
`read_page`, like `need_screen`. The next turn then carries the page's
visible text (`innerText`, so no hidden elements and no field values), at
most 4,000 characters, fenced as `<<<page … >>>page` and labelled as data.

- Text with prompt-injection or phishing phrases is withheld, with the same
  phrase lists as screen text (`kai_core::untrusted::suspicious_text`).
- The page cannot close the fence early.
- On the desktop the rule and the schema field are absent, so the desktop
  prompt is byte-for-byte unchanged (`crates/kai-core/tests/read_page_e2e.rs`).
- Not yet measured on a live model.

## What changes and what doesn't

The model sees **the same thing in the same format**: a screenshot with
numbered boxes and a list like `#3 button "Send"`. It answers with the same
12-action JSON. The system prompt is byte-for-byte unchanged, so prefix
caching and any fine-tune keep working. Approvals, risk rules, the outcome
ledger, human checks, procedures and memory all run unchanged, because the
lane is just another `Perceptor` + `Actuator` (`crates/kai-os/src/web/`).

| | Desktop path | Browser lane |
| --- | --- | --- |
| Elements | AT-SPI tree of the focused window (browser chrome + page, when the browser exposes it) | The page's own accessibility tree (`Accessibility.getFullAXTree`) joined with layout boxes (`DOMSnapshot.captureSnapshot`) on the backend node id: 2 calls, no per-node round trips |
| Password fields | AT-SPI `password text` role | DOM `input[type=password]`, forced secure (high risk, never shows a value) |
| Screenshot | Whole screen; Kai's windows masked | Viewport only (`Page.captureScreenshot`), works while the window is covered |
| Click / type / keys | Real pointer and keyboard (`enigo`), or AT-SPI actions | `Input.dispatchMouseEvent` / `insertText` / `dispatchKeyEvent` **to the page**: the user's pointer never moves, no window takes focus |
| Element → point | Element centre | DOM node scrolled into view, then its live content quad (survives layout shifts); grounding box when the marks are stale |
| Back / forward / reload | Browser shortcuts | Navigation history commands (protocol key events reach the page, not the browser) |
| `url_is` probe | Address-bar text from the tree, if exposed | The page's real address |
| `open_url` | The OS default browser | Kai's browser, after the address policy below |
| `shell`, `open_app` | Desktop | Passed to the desktop's hands when there is a desktop; refused in browser-only mode |

## Pages that break naive drivers

Each case has a test against a live headless Chromium
(`crates/kai-os/tests/browser_edges.rs`):

| Case | What goes wrong otherwise | What the lane does |
| --- | --- | --- |
| `alert` / `confirm` / `prompt` | The page freezes: screenshot, tree reads and script calls all hang until the dialog is answered (measured: the click's own release event never returns) | Every protocol call on a page with an open dialog returns at once (`DialogOpen`). The model sees the dialog as marks: `OK (page dialog: "<message>")`, `Cancel`, and an `Answer` field for `prompt()`. The message is in the button's name, so the risk words apply (answering "Delete all files?" is high risk whatever the model says). Enter and Esc answer it; anything else is refused with the reason. A click that opened a dialog counts as done. |
| Leaving a page with unsaved changes (`beforeunload`) | Navigation silently waits | `Leave page and discard changes` (high risk: "discard") or `Stay on page`. |
| Stale dialog marks | Clicking an old `OK` mark after the dialog closed would click the page underneath | Refused: "that page dialog is already closed". |
| Links with `target=_blank`, `window.open` | Kai keeps looking at the old tab | The tab the page opened becomes the working tab; if the working tab closes, the newest remaining one takes over. |
| Native `<select>` | Its popup is drawn by the browser, outside the page: invisible to the screenshot and the tree | `type` into the dropdown chooses an option by label (exact, prefix, then contains) and fires `input`/`change`. A miss lists the options; a disabled option is refused. The Enter the engine sends after typing into a dropdown is skipped. |
| `<input type="file">` | A native file dialog blocks the browser | File pickers are intercepted; the click reports "that opened a file picker … ask the user". |
| Downloads | A headless browser refuses them | Downloads go to the user's Downloads folder (`XDG_DOWNLOAD_DIR` or `~/Downloads`). |
| A crashed page | Every call fails | The page is reloaded once. |
| The browser is closed or killed | Calls wait for their timeout | "Kai's browser was closed" at once. |
| A second Kai browser on the same profile | Chromium hands over to the first and exits; the pipe just closes | "the browser exited at start … is another Kai browser already open with the profile …?" |
| The page navigates while it is read | The tree and the layout describe different documents | Detected (the two addresses differ) and read again. |

## Safety

- **Address policy** (`kai_core::web::normalise_url`, before every
  navigation): only `http`/`https`; a bare host means `https://`;
  `javascript:`, `file:`, `data:`, `chrome:`, `about:`, `view-source:` and
  every other scheme are refused, as are addresses with a user name or
  password and addresses with spaces or control characters. Tests:
  `web::tests` (unit) and `page_marks_typing_submit_policy_and_back`
  (navigating to `file:///etc/passwd` must fail on a live browser).
- **No debugging port.** The browser is started with
  `--remote-debugging-pipe`: the protocol runs over two pipes to the child
  process, so no other program on the machine can connect and drive it (a
  `--remote-debugging-port` can be reached by any local process).
- **Kai's own profile**, in `<config>/kai/browser`, never the user's
  everyday one. The user signs in to a site there once. Chrome 136+ refuses
  remote debugging on the default profile anyway.
- **Running as root** (containers): Chromium refuses to start with its
  sandbox, so the lane adds `--no-sandbox` and logs a warning. Don't run Kai
  as root on a real desktop.
- Page text is screen text: the existing rules (data, not instructions;
  pause on injection or phishing; `docs/SAFETY.md`) apply unchanged.

## Choosing the browser

`KAI_BROWSER=/path/to/chrome` wins; otherwise the first of `google-chrome`,
`google-chrome-stable`, `chromium`, `chromium-browser`, `microsoft-edge`,
`brave-browser` on `PATH`. `--headless` runs it without a window (servers,
CI); the viewport is then 1280×800.

## Measured (2026-10-12, this repo's Linux container, 4 vCPU, no GPU)

Headless Chromium 141, `cargo test -p kai-os --test browser_lane
observe_timings -- --nocapture`, 20 observations each, local fixture pages:

| Page | Marks | Full observe (screenshot + tree) p50 / p95 | Tree-only p50 / p95 |
| --- | --- | --- | --- |
| Contact form | 4 | 115 / 143 ms | **3 / 3 ms** |
| 150 controls | 150 (the mark cap) | 127 / 141 ms | **23 / 28 ms** |

For scale: Kai's desktop path measured an AT-SPI tree read of **136–198 ms**
for an 81-mark GTK window (DECISIONS.md, Phase 2b; a different app on an
earlier container, so this is indicative, not a head-to-head).

**Head-to-head on one page (not achieved).** `desktop_vs_lane_on_one_page`
(ignored by default; needs Xvfb, a window manager and the AT-SPI bus) shows
the same headed Chromium page to both paths. In this container the desktop
path read **no page elements** and fell back to the grid, even with the window
activated through `xdotool`; the lane read all 4. It is likely the
environment (Playwright's Chromium build, no desktop session), not a fair
verdict on the desktop path, so no speed comparison is claimed from it.

**End to end.** `engine_runs_a_web_task_through_the_lane`: the unchanged
engine, with scripted replies, opens the form, types a name, clicks Send
(high risk, approved) and finishes with `done`; the page's address then shows
the submitted form. Passed.

## Live model A/B (2026-10-13)

Gemma 4 26B-A4B through the Gemini API, the bench's four local web tasks,
two runs per side, each task checked by its own shell check. Full method,
every round and the raw files: [bench/results/2026-10-13-browser-ab.md](../bench/results/2026-10-13-browser-ab.md).

| | Desktop | Browser lane |
| --- | --- | --- |
| Tasks passed | 7 / 8 | **8 / 8** |
| Steps | 70 | **38** |
| Model calls | 76 | **42** |
| Prompt tokens | 162k | **85k** |
| Wall time | 409 s | **178 s** |

Eight task runs per side is too few for the pass rate to mean much; the
step, call and token reductions hold on every task. Each lane failure in
earlier rounds became a fix: dropdowns open as option marks, typing
replaces a field's value, and two skill problems that hit both paths.

Run it yourself: `kai bench --backend gemini --model gemma-4-26b-a4b-it
--browser --headless --only fill-form --only web-form-submit --only
local-price-lookup --only download-file --yes`.

## Not built yet

- **Windows and Firefox.** Windows needs the pipe over inherited handles (or
  a port plus a token); Firefox speaks WebDriver BiDi, not CDP.
- **File uploads.** Detected and handed to the user, not performed.
- **Iframes.** Only the main document is marked; same-origin iframes are
  separate documents in the snapshot and cross-origin ones separate targets.
  Use the grid (coordinates) there for now.
