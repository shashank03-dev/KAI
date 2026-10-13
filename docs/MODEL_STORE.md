# The model store

Kai's local models come from one store with two shelves:

- **The curated shelf**: `crates/kai-core/src/models/catalog.json`, entries
  checked by hand. Every URL is pinned to a commit and every file carries its
  SHA-256 and size. `kai models list` and the app's Models page show it,
  ranked for this computer (`docs/LOCAL_MODELS.md`).
- **The open shelf**: any GGUF vision model on Hugging Face.
  `kai models search <name>` finds one, and `kai models pull <owner/repo>`
  adds it to your own catalog (`<config>/models.json`), pinned the same way.
  After that it installs, starts and runs like a built-in model.

```sh
kai models search holo
kai models pull Hcompany/Holo4-35B-A3B-GGUF          # Q4_K_M by default
kai models pull bartowski/microsoft_Fara-7B-GGUF:Q8_0 --install
kai models install holo4-35b-a3b-q4_k_m
kai bench --backend local --model holo4-35b-a3b-q4_k_m --out target/bench-holo4
```

## Design note (2026-10-13)

**What.** Pulling an open model is one command and needs no code change.
The curated shelf gained the open computer-use models worth trying, each
verified the same way as the first ten.

**Why.** The owner wants "a store of open models that can be pulled and
used". Before this, adding a model meant hand-writing a JSON entry with
hashes (`docs/LOCAL_MODELS.md`, "Adding a new open model"). The Hub already
serves everything that entry needs: the commit (`/api/models/<repo>`, `sha`)
and the files with their LFS SHA-256 and size (`/api/models/<repo>/tree/<sha>`).
So `pull` builds the same entry the manual steps would.

**How it works.**

- `kai_core::models::hub` (OS-free, offline-tested) parses the reference
  (`owner/repo`, `owner/repo:Q8_0`, `hf:…` or a link), reads the Hub's JSON,
  and builds the entry:
  - **Weights:** the asked quantisation, else the first of `Q4_K_M`, `Q4_0`,
    `Q4_K_S`, `IQ4_XS`, `IQ4_NL`, `Q5_K_M`, `Q6_K`, `Q8_0`. A tag counts only
    as a whole token, so `Q4_K` never matches `Q4_K_M` and `Q4_K_M` never
    matches `IQ4_K_M`. Two matching files is an error that asks you to name
    one.
  - **Vision projector:** `Q8_0`, else `F16`, `BF16`, `F32`. **No projector
    means no pull**: Kai has to see the screen.
  - **Refused:** gated repos (they need a token and an accepted licence),
    and split GGUFs (`-00001-of-0000N`), which the installer does not join.
  - **Estimated, and labelled so in the entry's notes:** parameters (from the
    name, `35b-a3b` → 35 total, 3 active; else from the file size), and
    `min_ram_gb` = files + 1 GB runtime + a KV allowance of 10% of the
    weights, between 1 and 3 GB.
  - Always `"computer_use": "untested"`, so a pulled model is never "Best"
    until a bench says so.
  - The licence comes from the repo's card, else from its base model's card
    (GGUF repos often leave it out). Weights already in the catalog (same
    SHA-256) are not added again; `pull` names the existing id instead.
- `kai_os::local::hub` makes the two or three GET requests, through the same
  client as downloads (system proxy, `SSL_CERT_FILE`). `HF_ENDPOINT`, the
  Hub's own variable, points at a mirror.
- `Catalog::save_user_entry` merges the entry into `<config>/models.json`
  through a temporary file and a rename. The same id replaces an earlier
  pull, and your other entries stay.

**The `agent_trained` flag.** New, optional, default `false`. It marks
models whose maker trained them to operate computers (Holo, Fara, UI-TARS).
`kai models list` shows "made for computer use", and the ranking uses the
flag only to break ties between models that are equally tested. It is not a
Kai measurement; `computer_use` stays the only field that says how a model
did in Kai's harness.

**Rejected.**

- *Ollama's registry as the store:* a system service and its own model
  format. `docs/LOCAL_MODELS.md` explains why the runtime is llama.cpp.
- *Following `main` instead of pinning a commit:* a repo can change under
  the user, and the hash check would then fail or, worse, pass on new
  weights nobody checked.
- *Making a vendor-trained model "Best" on the vendor's numbers:* every Holo
  and Fara figure is vendor-run. "Best" stays reserved for models Kai has
  measured.
- *Downloading with a Hub token for gated repos:* that puts a secret in the
  download path for little gain. Every model worth shipping here is open.

**Tests.** `crates/kai-core/src/models/hub_tests.rs` runs on Hub JSON shapes
copied from the real API on 2026-10-13. It covers reference parsing
(including `../` and malformed names), pinned URLs, sizes and hashes,
quantisation choice and whole-token matching, and refusal of text-only,
gated and split repos. It also checks that saving merges, replaces and
leaves no temporary file, that search output is parsed and its query
cleaned, and that `agent_trained` breaks ties.

**Live check.** `kai models search holo` and `kai models pull
Hcompany/Holo4-35B-A3B-GGUF` were run against the real Hub on 2026-10-13
(see `DECISIONS.md`). Nothing was installed. The curated entry's 14 file
URLs (7 models × weights + projector) each answered `200`, with
`x-linked-size` equal to `size_bytes` and `x-linked-etag` equal to `sha256`.

## Added to the curated shelf (2026-10-13)

All untested with Kai. The scores are the vendors' own. Memory follows
`docs/LOCAL_MODELS.md`: files + KV at 16K (from each model's `config.json`:
layers, KV heads, head size, and which layers are full-attention) + 1 GB.

| id | what | made for computer use | download | min RAM | licence |
|---|---|---|---|---|---|
| `holo4-35b-a3b-q4` | H Company, official GGUF, Qwen3.6-35B-A3B base. Reports 80.8% OSWorld | yes | 22.2 GB | 23.6 GB | Apache-2.0 |
| `holo-3.1-35b-a3b-q4` | H Company, official GGUF. Reports 79.3% AndroidWorld | yes | 22.2 GB | 23.6 GB | Apache-2.0 |
| `holo-3.1-9b-q4` | H Company via mradermacher. Reports 72% AndroidWorld | yes | 6.25 GB | 7.8 GB | Apache-2.0 |
| `fara-7b-q4` | Microsoft via bartowski, Qwen2.5-VL base. Reports 73.5% WebVoyager | yes | 6.04 GB | 8.0 GB | MIT |
| `gemma-4-12b-it-qat-q4` | Google's own QAT Q4_0 | no | 7.15 GB | 8.9 GB | Apache-2.0 |
| `glm-4.6v-flash-q4` | Z.ai, llama.cpp's own GGUF | no | 7.15 GB | 8.9 GB | MIT |
| `qwen3.5-9b-q4` | Qwen via unsloth, the base of Holo3.1 9B | no | 6.6 GB | 8.2 GB | Apache-2.0 |

`holo-3.1-4b`, `holo-3.1-0.8b` and `ui-tars-1.5-7b` are now also marked
`agent_trained`.

**Not added.** Holo4 27B: CC BY-NC (non-commercial), which does not fit an
MIT app that people may use at work. OpenCUA: no GGUF found. EvoCUA: only
GGUFs from unknown single uploaders. Qwen3.5 35B-A3B: Holo3.1/Holo4 at the
same size are the same base trained for this job.

**What it takes to promote one to `recommended`.** Run `kai bench --backend
local --model <id>` on the same machine as a Gemma baseline, at least twice
per side, with `--out` set per run. Promote only if the pass rate is at
least as good. Record the numbers in `DECISIONS.md`.
