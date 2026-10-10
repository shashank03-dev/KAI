# Training and serving LoRA adapters on DigitalOcean

The whole training loop is described in [`train/README.md`](../../train/README.md).
This page covers only the Droplet side.

## Serve an adapter (on the serving Droplet from `up.sh`)

```sh
deploy/digitalocean/serve-lora.sh train/out/sft-26b/adapter       # served as "kai"
deploy/digitalocean/serve-lora.sh train/out/grpo-26b/iter-019 kai2 # any name
deploy/digitalocean/serve-lora.sh --remove                        # base model only
```

The script copies `adapter_config.json` and `adapter_model.safetensors` to
`/root/adapters/<name>`. It refuses if the adapter's base model differs from
the Droplet's `MODEL_ID`. It then adds a systemd drop-in
(`/etc/systemd/system/vllm.service.d/lora.conf`) that restarts the same
`vllm/vllm-openai:v0.25.0` container with the same flags plus:

```
--enable-lora --max-loras 1 --max-lora-rank <adapter r rounded up to 8/16/32/64/128/256>
--lora-modules <name>=/adapters/<name>
```

The base model is still served under its own name (`gemma-4-26b-a4b-it`).
**Kai selects the adapter by model name**, because vLLM routes a request whose
`model` is the adapter name to that adapter:

```sh
source deploy/digitalocean/.secrets/endpoint.env && export KAI_ENDPOINT VLLM_API_KEY
kai run   --backend vllm --model kai "find the 5 biggest files in Downloads"
kai bench --backend vllm --model kai --yes
```

In the app, set the model id to `kai` with the DigitalOcean (vLLM) backend.
Prefix caching works per adapter. The base model and the adapter each warm up
their own cache.

## Training Droplet

```sh
export HF_TOKEN=hf_...  KAI_SSH_KEY=my-laptop
deploy/digitalocean/train-up.sh       # 1x H100, SSH only from your IP
deploy/digitalocean/train-down.sh     # destroy it (bills while it exists)
```

`cloud-init-train.yaml` installs and starts the following:

- the NVIDIA container toolkit;
- Kai cloned from `KAI_REPO_URL`@`KAI_REPO_REF`, `kai` built with cargo, and
  the headless desktop (`scripts/desktop/install.sh`);
- the pinned training venv `train/.venv-gpu` (`train/requirements-gpu.txt`);
- a **localhost-only** vLLM (`vllm-train.service`) with `--enable-lora
  --max-loras 2 --max-lora-rank 64 --enable-sleep-mode`,
  `VLLM_ALLOW_RUNTIME_LORA_UPDATING=True` and `VLLM_SERVER_DEV_MODE=1`, with
  `/root/KAI/train/out` mounted read-only at `/adapters`.

Those dev endpoints (LoRA load/unload, sleep/wake) are what
`train/grpo_gemma.py` uses. They must never be exposed, which is why this
Droplet has no Caddy and a firewall that allows only SSH.
`~/.kai-train.env` exports `KAI_ENDPOINT=http://127.0.0.1:8000/v1` and the
generated API key. Secrets live in `.secrets/` locally (git-ignored) and in
root-only files on the Droplet. Nothing is committed.

`train-down.sh` refuses to touch the serving Droplet (`kai-gemma`). Copy
results off first: `rsync -a root@<ip>:/root/KAI/train/out/ train/out/`.
