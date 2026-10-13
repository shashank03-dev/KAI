# Local open models

Kai can run its model on your own computer instead of a cloud backend: one
action downloads a runtime and an open model, starts it on `127.0.0.1`, and
Kai talks to it like any other OpenAI-compatible endpoint. Screenshots never
leave the machine.

- Catalog (data, not code): `crates/kai-core/src/models/catalog.json`
- Ranking ("Best for this computer"): `crates/kai-core/src/models/rank.rs`
- Downloads, runtime, server: `crates/kai-os/src/local/`
- CLI: `kai models list | install <id|best> | start <id> | stop | remove <id>`,
  `kai run --backend local --model <id> "<task>"`, `kai doctor`

## Runtime: llama.cpp `llama-server`, pinned to release b11429

Research done 2026-10-10. Decision: **llama.cpp**, release **b11429**
(published 2026-10-05), `llama-server` with a GGUF model plus its `mmproj`
vision projector.

Why:

1. **It runs Gemma 4 with vision.** llama.cpp's own multimodal guide
   (`docs/multimodal.md` at tag b11429) lists `ggml-org/gemma-4-E2B-it-GGUF`,
   `gemma-4-E4B-it-GGUF`, `gemma-4-26B-A4B-it-GGUF` and `gemma-4-31B-it-GGUF`
   as supported multimodal models, loaded with `-m <model> --mmproj <mmproj>`.
   The ggml-org repos ship the matching `mmproj-*.gguf` files.
2. **It speaks the API Kai already uses.** `POST /v1/chat/completions` takes
   `image_url` parts as base64 data URIs (server README, "image_url"), and
   `response_format` with a JSON schema. Kai's existing `OpenAiClient` works
   unchanged; only the base URL, the model name and "no key" differ.
3. **Prebuilt, portable, no installer.** Each release has ready-made archives
   for Linux and Windows on x86_64 and arm64, for CPU, Vulkan (any GPU) and
   CUDA. Kai unpacks one into its own data folder. No admin rights, no system
   service. The Linux binaries use `RUNPATH=$ORIGIN` (checked with `readelf`),
   so the bundled libraries load without `LD_LIBRARY_PATH`.
4. **Safe defaults.** `--host` defaults to `127.0.0.1`, `--offline` blocks
   network access, and `/health` returns 503 while loading and 200 when
   ready, which gives Kai a clean readiness check. MIT licence.

Alternatives considered:

- **Ollama** also runs Gemma 4 with images (Ollama ≥ 0.20, according to
  third-party guides). But it is a system install (a Windows installer, a
  root install script on Linux) running a background service, and it uses its
  own model registry instead of arbitrary GGUF + mmproj files, which would
  make the data-driven catalog harder. One third-party report (BatiAI, on
  Hugging Face) says Ollama 0.20 did not know the Gemma 4 vision projectors
  while llama.cpp master did. The evidence is mixed, but nothing shows Ollama
  beating llama.cpp here, so Kai uses llama.cpp.
- **vLLM** is Linux + CUDA only and heavy. Kai already uses it for the
  DigitalOcean server.
- **LM Studio** is a closed-source GUI app, so Kai cannot bundle it.

Sources (accessed 2026-10-10):

- llama.cpp release list and assets for b11429:
  https://github.com/ggml-org/llama.cpp/releases (latest tag b11429, 2026-10-05)
- Server flags and endpoints at the pinned tag:
  https://raw.githubusercontent.com/ggml-org/llama.cpp/b11429/tools/server/README.md
- Multimodal guide listing Gemma 4:
  https://raw.githubusercontent.com/ggml-org/llama.cpp/b11429/docs/multimodal.md
- Gemma 4 model card (sizes, context, licence):
  https://huggingface.co/google/gemma-4-26B-A4B-it
- GGUF repos: https://huggingface.co/ggml-org/gemma-4-E4B-it-GGUF (and the
  E2B, 26B-A4B and 31B siblings), https://huggingface.co/Qwen/Qwen3-VL-8B-Instruct-GGUF,
  https://huggingface.co/Qwen/Qwen3-VL-4B-Instruct-GGUF,
  https://huggingface.co/mradermacher/UI-TARS-1.5-7B-GGUF
- Hugging Face file listings with LFS SHA-256:
  `https://huggingface.co/api/models/<repo>/tree/main`
- Ollama with Gemma 4 (third-party guides): https://getdeploying.com/guides/local-gemma4,
  https://ai.google.dev/gemma/docs/integrations/ollama
- Ollama vs llama.cpp for Gemma 4 projectors: https://huggingface.co/batiai/llamacpp-server-macos

### Which build Kai downloads

| OS | GPU found | Build (b11429 asset) | Size |
|---|---|---|---|
| Windows x64 | NVIDIA | `llama-b11429-bin-win-cuda-12.4-x64.zip` + `cudart-llama-bin-win-cuda-12.4-x64.zip` | 264,478,423 + 391,443,627 B |
| Windows x64 | other dedicated GPU | `llama-b11429-bin-win-vulkan-x64.zip` | 33,337,769 B |
| Windows x64 | none | `llama-b11429-bin-win-cpu-x64.zip` | 19,398,918 B |
| Windows arm64 | any | `llama-b11429-bin-win-cpu-arm64.zip` | 12,229,035 B |
| Linux x64 | dedicated GPU and `libvulkan.so.1` | `llama-b11429-bin-ubuntu-vulkan-x64.tar.gz` | 31,636,673 B |
| Linux x64 | otherwise | `llama-b11429-bin-ubuntu-x64.tar.gz` | 17,693,462 B |
| Linux arm64 | GPU + Vulkan / otherwise | `…-ubuntu-vulkan-arm64.tar.gz` / `…-ubuntu-arm64.tar.gz` | 24,845,090 / 13,682,895 B |

On Linux, NVIDIA cards get the Vulkan build too. The Linux CUDA build needs
either a 567 MB CUDA runtime archive or a system CUDA 12.8. CUDA 12.4 was
picked over 13.4 on Windows because it works with older NVIDIA drivers. Every
archive's SHA-256 is pinned in `runtime.rs` and checked after download.

### Server command line

Checked against the b11429 server README:

```
llama-server -m <model.gguf> --mmproj <mmproj.gguf> --host 127.0.0.1 --port 8098 \
  -c <context> -ngl auto --alias <served_name> --jinja --no-webui --offline
```

`-ngl auto` (also the default) and `--fit on` (default) let llama.cpp choose
how many layers go to the GPU. A mixture-of-experts model that does not fit
spills its experts to RAM. `--alias` sets the model id that the API answers to.
Kai sends that id as `model`.

## Catalog format

`catalog.json` is `{"version": 1, "models": [ModelEntry, …]}`. One entry:

```json
{
  "id": "gemma-4-e4b-it-q4",            // stable; becomes a folder name
  "family": "gemma-4",
  "label": "Gemma 4 E4B",
  "maker": "Google",
  "license": "Apache-2.0",              // SPDX
  "params_b": 8.0,                      // total parameters, billions
  "active_params_b": 4.5,               // per token (MoE / Gemma "effective"); omit for dense
  "quant": "Q4_0",
  "vision": true,                       // Kai needs vision; non-vision models are never offered
  "context": 16384,                     // what Kai asks the server for
  "download_gb": 5.15,                  // all files, 10^9 bytes
  "min_ram_gb": 6.9,                    // files + KV cache at `context` + 1 GB overhead
  "min_vram_gb": 6.9,                   // same, for full GPU offload
  "files": [
    { "url": "https://huggingface.co/<repo>/resolve/<commit>/<file>.gguf",
      "sha256": "<64 hex>", "role": "model", "size_bytes": 4590807392 },
    { "url": "…/mmproj-….gguf", "sha256": "…", "role": "mmproj", "size_bytes": 559874816 }
  ],
  "runtime": "llama.cpp",
  "served_name": "gemma-4-e4b-it",      // --alias, and the `model` Kai sends
  "notes": "Shown on the model card.",
  "computer_use": "experimental"        // recommended | experimental | untested
}
```

Rules, enforced by `ModelEntry::problems()`. An entry that breaks one is
skipped with a log line:

- exactly one `"model"` file, optional `"mmproj"`, https URLs (plain http
  only for `http://127.0.0.1:` mirrors), 64-hex lower-case `sha256` when
  given, ids of letters, digits, `-`, `_` and `.`, `runtime` = `llama.cpp`.

### Adding a new open model

No code changes are needed: add one JSON entry.

1. Find a GGUF repo with the model and its `mmproj` file. Get sizes and
   SHA-256 from `https://huggingface.co/api/models/<repo>/tree/main` (the
   `lfs.oid` field is the SHA-256), and the commit from
   `https://huggingface.co/api/models/<repo>` (`sha`). Pin URLs to that
   commit: `https://huggingface.co/<repo>/resolve/<sha>/<file>`.
2. Check that llama.cpp supports the architecture (its `docs/multimodal.md`).
3. Work out the memory: `min_ram_gb = download_gb + KV(context) + 1.0`,
   where KV ≈ 2 × layers × kv_heads × head_dim × 2 bytes × context.
4. Put the entry either in `catalog.json` (for everyone; the unit test checks
   sizes add up), or in your own `<config>/models.json`
   (`~/.config/kai/models.json`, `%APPDATA%\Kai\models.json`) with the same
   `{"models": […]}` shape. A user entry with an existing `id` replaces it.
5. Use `"computer_use": "untested"` until it has been benchmarked with
   `kai bench`.

## Models in the catalog

All URLs are pinned to a repo commit. **Verified 2026-10-10**: every URL
answered a HEAD request with 200 (after Hugging Face's redirect), its
`Content-Length` / `x-linked-size` equals `size_bytes`, and its
`x-linked-etag` equals the catalog `sha256`. Range requests answer 206
(checked on one HF file and one GitHub asset).

| id | files (model + mmproj) | download | licence | computer use |
|---|---|---|---|---|
| `gemma-4-e2b-it-q4` | ggml-org Q4_0 + mmproj Q8_0 | 3.40 GB | Apache-2.0 | experimental |
| `gemma-4-e4b-it-q4` | ggml-org Q4_0 + mmproj Q8_0 | 5.15 GB | Apache-2.0 | experimental |
| `gemma-4-26b-a4b-it-q4` | ggml-org Q4_0 + mmproj Q8_0 | 15.42 GB | Apache-2.0 | recommended |
| `gemma-4-31b-it-q4` | ggml-org Q4_0 + mmproj Q8_0 | 18.80 GB | Apache-2.0 | recommended |
| `qwen3-vl-4b-instruct-q4` | Qwen official Q4_K_M + mmproj Q8_0 | 2.95 GB | Apache-2.0 | untested |
| `qwen3-vl-8b-instruct-q4` | Qwen official Q4_K_M + mmproj Q8_0 | 5.78 GB | Apache-2.0 | untested |
| `ui-tars-1.5-7b-q4` | mradermacher Q4_K_M + mmproj Q8_0 | 5.54 GB | Apache-2.0 (base model) | untested |
| `holo-3.1-4b-q4` | mradermacher Q4_K_M + mmproj Q8_0 | 3.43 GB | Apache-2.0 | untested |
| `holo-3.1-0.8b-q4` | mradermacher Q4_K_M + mmproj Q8_0 | 0.79 GB | Apache-2.0 | untested |

The ggml-org Gemma 4 GGUFs are converted from Google's QAT (quantization-aware
trained) Q4_0 checkpoints, according to the repo card. That is why Q4_0 was
chosen over K-quants. Gemma 4 sizes come from Google's card: E2B 2.3B
effective (5.1B with embeddings), E4B 4.5B (8B), 26B A4B 25.2B total / 3.8B
active, 31B 30.7B. Context is 128K (E-series) and 256K (26B, 31B); Kai asks
for 16K. UI-TARS uses its own action format, not Kai's JSON replies, so it
probably needs a prompt adapter before it is useful.

### Holo3.1 (H Company), added 2026-10-13

Computer-use models fine-tuned from Qwen3.5 (H Company's card,
`Hcompany/Holo-3.1-4B`, Apache-2.0). H Company publishes official GGUFs only
for the 35B-A3B (Q4 ≈ 21 GB, too big for a 16 GB machine), so the 4B and 0.8B
entries use mradermacher's community GGUFs, pinned to a commit with their
SHA-256 (checked by Kai's installer on download). llama.cpp b11429 knows the
architecture (`qwen35`). Qwen3.5 is hybrid: only every 4th layer keeps a KV
cache (8 of 32 in the 4B), so `min_ram_gb` counts 0.54 GB of KV at 16K, not
2.1 GB. They stay `untested` (never "Best") until a bench run says
otherwise; see the measurements below.

### Server memory settings (measured)

Holo3.1 4B Q4_K_M, llama.cpp b11429 CPU build, 4 vCPU Xeon with AMX,
16 GB RAM, no GPU; three requests with the same 1280×720 Set-of-Marks
screenshot (~950 prompt tokens), RSS read from `ps`:

| Flags | RSS after 3 image calls | First call (no cache) | Next calls (cached prefix) | Reply |
| --- | --- | --- | --- | --- |
| llama.cpp defaults | 6.51 GB | 42.2 s | 9.6 s | empty: 64 tokens of thinking, no answer |
| `-np 1 --cache-ram 0` | 6.36 GB | 40.1 s | 9.5 s | empty (thinking) |
| + `--ctx-checkpoints 2 --no-repack` | **4.29 GB** | 39.4 s | 9.4 s | empty (thinking) |
| + `--reasoning off` | **4.29 GB** | **29.7 s** | **1.1 s** | `{"target":63}` (correct) |

- **`--no-repack`** is the big one: on CPUs with AMX (and other repack
  targets) llama.cpp keeps a repacked copy of the weights (2.6 GB here) next
  to the memory-mapped file. No speed difference was measured.
- **`--reasoning off`**: Qwen3.5-family models think by default, which spent
  the whole reply budget before any JSON. Kai wants one short JSON object.
- **Image budget**: limiting the image to 512 or 320 tokens cut the first
  call to 14.4 s / 10.0 s but picked the wrong element (#58, #10 instead of
  #63), so Kai keeps the model's default budget.
- **On real tasks** (`kai bench --backend local --model holo-3.1-4b-q4
  --browser --headless`, the four local web tasks, one run): 3 / 4 passed,
  model call p50 46 s, llama-server RSS 4.44 GB peak over the run.
  Too slow for the 10-minute task limit on long tasks on a 4-core CPU;
  details in `bench/results/2026-10-13-browser-ab.md`.
- Kai starts every local server with these flags (`LEAN` in
  `crates/kai-os/src/local/server.rs`); a server already running with older
  flags is reused until it is stopped (`kai models stop`).

## Recommendation rules

`rank(&Catalog, &HardwareSummary)` gives every model a place to run, a speed
tier, a fit and a reason. Thresholds (`rank.rs`):

- **Usable RAM** = RAM − OS reserve, where the reserve is 25 % of RAM,
  at least 2 GB and at most 6 GB. **Usable VRAM** = dedicated VRAM − 0.5 GB.
  Integrated GPUs (under 1 GiB dedicated) count as no GPU.
- **Best** goes to the first fitting model whose `computer_use` is
  `recommended` or `experimental`; an `untested` model is never Best, and if
  no tested model fits, nothing is.
- **GPU**: `min_vram_gb ≤ usable VRAM` → tier 3 ("Runs well").
- **Split** (GPU + RAM): usable VRAM ≥ 30 % of the need and VRAM + RAM ≥
  need. If VRAM + RAM ≥ 1.25 × need and ≤ 5B parameters work per token
  (E2B, E4B, the 26B A4B MoE), it is tier 3. A tighter fit is tier 1 (slow).
  A dense model above 5B split this way is tier 0 (very slow).
- **CPU**: `min_ram_gb ≤ usable RAM`. With ≤ 5B active it is tier 2 (slow but
  usable: tens of seconds per step); bigger dense models are tier 0.
- Otherwise **Too big**. The reason gives the RAM needed
  (smallest RAM that leaves the need after the reserve).
- **Best** = highest tier, then computer-use status (recommended >
  experimental > untested), then more parameters. Exactly one model is Best
  when anything fits. Non-vision models are never offered.

What this gives on the built-in catalog. Each row is a unit test in
`rank_tests.rs`:

| Machine | Best |
|---|---|
| 4 GB RAM, no GPU | nothing fits |
| 8 GB RAM, no GPU | Gemma 4 E2B (CPU) |
| 16 GB RAM, no GPU | Gemma 4 E4B (CPU) |
| 32 GB RAM, no GPU | Gemma 4 26B A4B (CPU; MoE keeps it usable) |
| 16 GB RAM + 4 GB GPU | Gemma 4 E4B (split) |
| 16 GB RAM + 8 GB GPU | Gemma 4 E4B (full offload) |
| 32 GB RAM + 8 GB GPU | Gemma 4 26B A4B (partial offload) |
| 16 GB RAM + 12 GB GPU | Gemma 4 26B A4B (partial offload) |
| 32 GB RAM + 16 GB GPU | Gemma 4 26B A4B |
| 64 GB RAM + 24 GB GPU | Gemma 4 31B (full offload) |
| 128 GB RAM + 48 GB GPU | Gemma 4 31B |

The memory figures are **estimates, not measurements**: files + 1 GB runtime
overhead + a KV-cache allowance at 16K context (0.5 / 0.75 / 1.0 / 2.5 GB for
E2B / E4B / 26B / 31B, generous because Gemma 4's sliding-window layers make
the real cache smaller than a dense estimate; 2.4 GB for Qwen3-VL 4B and 8B
and 0.94 GB for UI-TARS from their layer and head counts). Hardware is read
in GiB (what the OS and `nvidia-smi` report) and file sizes in 10^9 bytes,
which errs on the safe side.

## Data folder and privacy

`$XDG_DATA_HOME/kai/local` (default `~/.local/share/kai/local`) on Linux and
`%LOCALAPPDATA%\Kai\local` on Windows. `KAI_LOCAL_DIR` overrides it. Inside:
`runtime/` (the unpacked build plus `current.json`), `models/<id>/` (GGUFs,
`entry.json`, and a `.complete` marker written last), `downloads/` (runtime
archives while downloading), `server.json` and `logs/llama-server.log`.

- Nothing is downloaded or started without an explicit action (the app's
  button, `kai models install|start`, or `--backend local`, which starts an
  installed model for that run).
- Downloads come only from the pinned URLs above (Hugging Face and GitHub
  releases). Partial files resume with HTTP range requests, the SHA-256 is
  checked before a file is renamed into place, and a mismatch deletes it.
  Free disk space is checked first (needed bytes + 1 GB).
- The server binds `127.0.0.1` only and runs `--offline`. Prompts and
  screenshots stay on this machine. Kai stops the server when the app exits
  (its handle kills the process on drop). `kai models start --detach` leaves
  it running until `kai models stop`.

## What was verified and what was not

Verified here (2026-10-10):

- All 14 model file URLs: HEAD → 200, sizes and LFS SHA-256 equal to the
  catalog. Range requests → 206.
- All 9 runtime asset URLs: HEAD → 200 with the sizes above. These 6 were
  downloaded (not run) and their SHA-256 recomputed, matching GitHub's
  digests: linux x64 CPU, linux x64 Vulkan, linux arm64 CPU, windows x64
  CPU, windows x64 Vulkan, windows arm64 CPU. For the Windows CUDA archive,
  its cudart archive and the linux arm64 Vulkan archive, the SHA-256 comes
  from GitHub's release page digest only.
- Archive layout: the Linux tarball has `llama-b11429/llama-server` plus
  `.so` files, and the Windows zips are flat with `llama-server.exe`. The
  unpacker searches for the binary, so both work.
- Tests with a local mock HTTP server: resume after a dropped connection
  (second request carries `Range: bytes=<n>-`), restart when a server ignores
  ranges, checksum mismatch deletes the file, cancel keeps the `.part` and the
  next install resumes, zip and tar.gz unpacking. Tests with a fake
  `llama-server` (a shell script running a Python `/health` server, 503 then
  200): start, health wait, the exact arguments (`--host 127.0.0.1`,
  `--alias`, `-m`, `--mmproj`), status, kill on drop, detach + stop via
  `server.json`, and a server that dies while loading reports its log.
- Windows code type-checks (`cargo clippy --target x86_64-pc-windows-gnu`).

**Not verified: actually running llama.cpp or any model.** This environment
does not allow running downloaded third-party binaries, and it has no GPU.
These must be checked on a real Linux and a real Windows machine:

- that `llama-server` b11429 starts with these exact flags, loads each
  catalog model with its mmproj, and answers Kai's requests (image parts,
  `response_format`);
- speed per step, and the real memory use against the estimates above;
- Gemma 4 can produce thinking output depending on its chat template. If
  replies come back with reasoning text or run out of tokens, Kai may need to
  pass `chat_template_kwargs` or a larger `max_tokens`;
- Windows: the DXGI VRAM detection, CUDA build selection, `taskkill`-based
  stop, and that the CUDA 12.4 build runs on the user's driver;
- the Linux release binaries' glibc requirement on older distributions.
