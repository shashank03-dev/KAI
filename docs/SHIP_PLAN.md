# Ship plan: open models, OpenAI-style loop, two days (2026-10-13)

What the owner asked for: Kai should feel as simple as OpenAI's computer
use, run open models fast and accurately, sustain longer sessions, and offer
a store of open models that can be pulled. This is the plan in one place:
what the research says, what is built, and what is left before release, in
order.

## 1. What OpenAI's loop is, and what Kai keeps from it

OpenAI's computer use, as documented today
([guide](https://developers.openai.com/api/docs/guides/tools-computer-use),
accessed 2026-10-13):

1. Send the task with `tools: [{type: "computer"}]`.
2. The model returns one `computer_call` with an ordered `actions` array
   (click, double_click, drag, move, scroll, keypress, type, wait,
   screenshot).
3. Run the actions in order, capture one screenshot, and return it as
   `computer_call_output` (`detail: "original"`).
4. Repeat with `previous_response_id` until no `computer_call` comes back.
   Verify the outcome in the app, not from the model's last message.

For its newest model OpenAI recommends code execution (Playwright or
PyAutoGUI scripts) over the computer tool. Anthropic's current toolsets add
batching, "stop at the first failure", and a separate browser surface with
accessibility refs (`docs/RESEARCH_2026_10_VENDORS.md`).

| OpenAI loop | Kai | Why Kai's form suits open models |
|---|---|---|
| `actions[]` per call | the reply's action plus `then` (≤ 3), each re-checked for target, risk and stop | same round-trip saving, plus a guard the vendor leaves to you |
| pixel coordinates | numbered marks from the accessibility tree; grid fallback | a 4B-active model picks a number reliably and was never trained to count pixels ([ComponentBench](https://arxiv.org/abs/2608.18307): 83.1% vs 48.9% for the same model, tree vs pixels) |
| full history on the server | step window + **run memo** (new, `docs/LONG_RUNS.md`) + optional macro history | constant prompt size, prefix cache hits, fits 16K local context |
| one screenshot per turn | screenshot when needed; tree-only steps when text suffices | prefill dominates small-model latency |
| safety classifiers alongside the model | harness rules before every risky action | open models lack the safety training |
| code execution mode | `shell` with output read back; the browser lane over CDP | the safe, verifiable part of "code mode" |

So the architecture is already OpenAI's loop, made smaller for small models.
The one missing piece was what the server-side history gives a frontier
model: remembering what it read. The run memo is that piece.

## 2. Built in this change (tested; see each doc)

1. **Model store** (`docs/MODEL_STORE.md`): `kai models search` and
   `kai models pull <owner/repo>[:quant]`. Any GGUF vision model on Hugging
   Face becomes a catalog entry pinned to a commit and SHA-256. Gated, split
   and text-only repos are refused.
2. **Curated shelf +7**, every URL checked live (size and SHA-256): Holo4
   35B-A3B, Holo3.1 35B-A3B and 9B, Fara 7B, Gemma 4 12B QAT, GLM-4.6V Flash
   and Qwen3.5 9B. A new `agent_trained` flag marks models trained for
   computer use and breaks ties between untested ones.
3. **Run memo** (`docs/LONG_RUNS.md`): a scratchpad the model keeps across
   steps, behind `--memo` (off by default). Injection-looking memos are
   refused, and with the switch off the prompt is byte-stable.
4. **Task checks** (`docs/VERIFICATION.md`, `--task-checks`, off): the
   DONE gate reads the task's named output (a file, a folder in the file
   manager) with no model call, against the main live failure, false
   "done".
5. **The store in the app:** Models › More from Hugging Face, with search
   and Pull, and a *Made for computer use* badge on model cards.
6. **Release workflow:** a tag now runs the full test suite first,
   publishes SHA-256 files next to the archives, and smoke-tests the
   unpacked CLI on Linux and Windows. "Run workflow" by hand is a dry run
   that does not publish. `kai … | head` no longer panics on the closed
   pipe.
7. **Code health:** the four files over the 500-line rule were split by
   responsibility: prompt tests, human-check phrase tables, Skills page
   tests, and the window's pages and shortcuts (`kai-app/src/nav.rs`).
   There is no behaviour change.

## 3. Left before release, in order (needs keys or a GPU; none were here)

Day 1, measure:

1. **Memo A/B on live Gemma 4 26B** (Gemini key): 14-task bench, two runs
   per side, `--memo` vs off, with `csv-total` and `local-price-lookup`
   added. Turn it on in `Settings::default` only if passes go up and tokens
   per passed task do not.
2. **Holo4 35B-A3B vs Gemma 4 26B-A4B on the Droplet** (vLLM;
   `docs/RESEARCH_2026_10_VENDORS.md` §5.3): `kai bench --backend vllm
   --model Hcompany/Holo4-35B-A3B`, two runs per side. If it wins, it
   becomes the Droplet default. It has the same active size and an
   Apache-2.0 licence.
3. **Local shelf on a consumer GPU** (8–12 GB): Holo3.1 9B, Fara 7B and
   Gemma 4 12B QAT through `kai bench --backend local`. Promote to
   `recommended` whatever matches Gemma 4 E4B's pass rate. This is what
   makes "Best for this computer" point at a model trained for computer use.

Day 2, decide the defaults and package:

4. **Task checks A/B** on live Gemma: `--task-checks` vs off, two runs
   per side (built; see §2).
5. **Release:** push a `v*` tag, or first "Run workflow" on Release for a
   dry run. Then try one real `kai run` and the app's Models page on a
   Windows machine: the Windows code is type-checked here but has never
   run.

What cannot be promised: measured accuracy for any model this session did
not run. Every vendor score above is the vendor's. Kai's own numbers come
only from `kai bench`, and every one of them is in `DECISIONS.md`.
