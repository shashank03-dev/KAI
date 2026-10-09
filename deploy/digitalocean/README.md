# Serving Gemma 4 on DigitalOcean

One command brings up a GPU Droplet running **Gemma 4 (26B-A4B)** on vLLM, with
an OpenAI-compatible API behind Caddy TLS. Nothing is clicked in the console.

## Prerequisites

1. **doctl**, authenticated: `doctl auth init` (needs a DigitalOcean API token).
2. **Hugging Face token** with Gemma access. Accept the Gemma license on the
   model page first, then: `export HF_TOKEN=hf_...` — **never commit it.**
3. An **SSH key already uploaded** to your DO account
   (`doctl compute ssh-key list`); pass its name as `KAI_SSH_KEY`.

## Bring it up

```sh
export HF_TOKEN=hf_xxx
export KAI_SSH_KEY="my-laptop"
# optional: export KAI_REGION=nyc2   KAI_HOSTNAME_MODE=sslip
deploy/digitalocean/up.sh
```

`up.sh`:

- confirms the region (**H100 is in `nyc2`, `ams3`, `tor1` only**) and the GPU
  size against the live API, and refuses to proceed otherwise;
- creates a cloud firewall (SSH from your IP only, 443 open), tags the Droplet
  `kai`, and passes `cloud-init.yaml`;
- generates a vLLM API key into `.secrets/` (git-ignored);
- prints the **hourly cost** and the endpoint `https://<host>/v1`.

`cloud-init.yaml` installs the NVIDIA container toolkit, pulls
`vllm/vllm-openai:v0.25.0`, downloads `google/gemma-4-26B-A4B-it`, and runs vLLM
as a systemd service bound to localhost with prefix caching and image input
(2 per prompt). Caddy terminates TLS and rate-limits `/v1/*`; `/health` returns
`ready` only after the model answers a warm-up request **with an image**.

```sh
curl -s https://<host>/health     # -> ready, once the model is loaded
```

## Smoke test (latency)

```sh
deploy/digitalocean/smoke.sh 20    # 20 real image requests; prints p50/p95
```

Record the numbers in `DECISIONS.md`. (First-token latency needs a streaming
client — Kai's own `smoke` subcommand, `kai bench --backend vllm`, measures it.)

## Tear it down — this is the one that stops billing

```sh
deploy/digitalocean/down.sh
```

A GPU Droplet is **~US$3.39/hour and bills while it exists, even when idle or
powered off.** Stopping it does nothing; only destroying it (this script) ends
charges. Run it the moment you are done.

## Notes

- To use a real domain instead of `sslip.io`, set `KAI_HOSTNAME_MODE=your.domain`
  and point an A record at the Droplet IP so Caddy can issue a public cert
  (swap `tls internal` for your email in the Caddyfile).
- Everything in `.secrets/` (API key, rendered cloud-init, endpoint env) is
  git-ignored. Rotate the key by deleting `.secrets/vllm_api_key` and re-running.
