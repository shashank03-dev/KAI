#!/usr/bin/env python3
"""A tiny localhost proxy between Kai and vLLM for RL rollouts.

Why it exists: Kai asks for `temperature: 0` (deterministic, right for real
use). GRPO needs *different* attempts at the same task, or every group has
zero variance and there is nothing to learn. Kai's Rust code is not changed
for training, so this proxy rewrites the sampling parameters, asks vLLM for
token logprobs, and logs every call (exact system prompt, user turn, images,
sampled reply, logprobs) per episode:

    KAI_ENDPOINT=http://127.0.0.1:8100/ep/<episode-id>/v1   (Kai appends /chat/completions)

The upstream call is made non-streaming; if Kai asked for a stream, the reply
is re-sent as OpenAI-style SSE (`data: {...}` chunks, a usage chunk, then
`data: [DONE]`), which is the wire format Kai's stream reader parses.

Run alone for debugging:
    python train/rollout_proxy.py --upstream http://127.0.0.1:8000/v1 --log-dir /tmp/rollouts
Binds 127.0.0.1 only. API keys pass through in the Authorization header and
are never logged.
"""

from __future__ import annotations

import argparse
import base64
import json
import re
import threading
import time
import urllib.error
import urllib.request
from dataclasses import dataclass
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

EP_PATH = re.compile(r"^/ep/([A-Za-z0-9._-]{1,120})(/v1/.*)$")


@dataclass
class ProxyConfig:
    upstream: str                 # e.g. http://127.0.0.1:8000/v1
    log_dir: Path
    temperature: float = 1.0
    top_p: float = 0.95
    top_k: int = 64
    logprobs: bool = True
    timeout_s: float = 120.0


class _State:
    def __init__(self):
        self.lock = threading.Lock()
        self.counters: dict[str, int] = {}

    def next_call(self, ep: str) -> int:
        with self.lock:
            n = self.counters.get(ep, 0)
            self.counters[ep] = n + 1
            return n


def _split_messages(body: dict, ep_dir: Path, call: int):
    """System text, user text and saved image paths from an OpenAI request."""
    system, user, images = "", "", []
    for m in body.get("messages", []):
        c = m.get("content")
        if m.get("role") == "system":
            system = c if isinstance(c, str) else " ".join(p.get("text", "") for p in c or [])
        elif m.get("role") == "user":
            parts = c if isinstance(c, list) else [{"type": "text", "text": c or ""}]
            for p in parts:
                if p.get("type") == "text":
                    user += p.get("text", "")
                elif p.get("type") == "image_url":
                    url = (p.get("image_url") or {}).get("url", "")
                    if url.startswith("data:") and "," in url:
                        f = ep_dir / f"call-{call:03d}-{len(images)}.jpg"
                        f.write_bytes(base64.b64decode(url.split(",", 1)[1]))
                        images.append(str(f))
    return system, user, images


def make_handler(cfg: ProxyConfig, state: _State):
    class Handler(BaseHTTPRequestHandler):
        protocol_version = "HTTP/1.1"

        def log_message(self, *a):  # quiet
            pass

        def _route(self):
            m = EP_PATH.match(self.path)
            return (m.group(1), m.group(2)) if m else ("default", self.path)

        def _upstream(self, rest: str) -> str:
            # rest starts with /v1/...; cfg.upstream ends with /v1
            return cfg.upstream.rstrip("/") + rest[len("/v1"):]

        def _send(self, code: int, data: bytes, ctype="application/json"):
            self.send_response(code)
            self.send_header("Content-Type", ctype)
            self.send_header("Content-Length", str(len(data)))
            self.end_headers()
            self.wfile.write(data)

        def do_GET(self):
            _, rest = self._route()
            req = urllib.request.Request(self._upstream(rest), headers=self._auth())
            try:
                with urllib.request.urlopen(req, timeout=cfg.timeout_s) as r:
                    self._send(r.status, r.read())
            except urllib.error.HTTPError as e:
                self._send(e.code, e.read())

        def _auth(self):
            h = {"Content-Type": "application/json"}
            if self.headers.get("Authorization"):
                h["Authorization"] = self.headers["Authorization"]
            return h

        def do_POST(self):
            ep, rest = self._route()
            raw = self.rfile.read(int(self.headers.get("Content-Length", "0")))
            body = json.loads(raw or b"{}")
            want_stream = bool(body.pop("stream", False))
            body.pop("stream_options", None)
            body.update(temperature=cfg.temperature, top_p=cfg.top_p, top_k=cfg.top_k)
            if cfg.logprobs:
                body["logprobs"] = True
            ep_dir = cfg.log_dir / ep
            ep_dir.mkdir(parents=True, exist_ok=True)
            call = state.next_call(ep)
            system, user, images = _split_messages(body, ep_dir, call)
            t0 = time.time()
            req = urllib.request.Request(self._upstream(rest), data=json.dumps(body).encode(),
                                         headers=self._auth(), method="POST")
            try:
                with urllib.request.urlopen(req, timeout=cfg.timeout_s) as r:
                    resp = json.loads(r.read())
            except urllib.error.HTTPError as e:
                self._send(e.code, e.read())
                return
            except (urllib.error.URLError, TimeoutError) as e:
                self._send(502, json.dumps({"error": {"message": f"proxy: {e}"}}).encode())
                return
            choice = (resp.get("choices") or [{}])[0]
            content = (choice.get("message") or {}).get("content") or ""
            lp = [t.get("logprob") for t in ((choice.get("logprobs") or {}).get("content") or [])]
            rec = {"call": call, "model": body.get("model"), "system": system, "user": user,
                   "images": images, "reply": content, "logprobs": lp or None,
                   "finish_reason": choice.get("finish_reason"), "usage": resp.get("usage"),
                   "ms": int((time.time() - t0) * 1000), "temperature": cfg.temperature}
            with open(ep_dir / "calls.jsonl", "a", encoding="utf-8") as f:
                f.write(json.dumps(rec, ensure_ascii=False) + "\n")
            if not want_stream:
                self._send(200, json.dumps(resp).encode())
                return
            chunks = [
                {"id": resp.get("id"), "object": "chat.completion.chunk", "model": resp.get("model"),
                 "choices": [{"index": 0, "delta": {"role": "assistant", "content": content},
                              "finish_reason": choice.get("finish_reason")}]},
                {"id": resp.get("id"), "object": "chat.completion.chunk", "choices": [],
                 "usage": resp.get("usage")},
            ]
            data = "".join(f"data: {json.dumps(c)}\n\n" for c in chunks) + "data: [DONE]\n\n"
            self._send(200, data.encode(), "text/event-stream")

    return Handler


class RolloutProxy:
    """Start/stop the proxy in a background thread."""

    def __init__(self, cfg: ProxyConfig, port: int = 8100):
        self.cfg, self.port = cfg, port
        self.server = ThreadingHTTPServer(("127.0.0.1", port), make_handler(cfg, _State()))
        self.port = self.server.server_port  # the real one when port=0
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)

    def endpoint(self, episode: str) -> str:
        return f"http://127.0.0.1:{self.port}/ep/{episode}/v1"

    def __enter__(self):
        self.thread.start()
        return self

    def __exit__(self, *exc):
        self.server.shutdown()
        self.server.server_close()


def read_calls(log_dir: Path, episode: str) -> list[dict]:
    p = Path(log_dir) / episode / "calls.jsonl"
    if not p.exists():
        return []
    return [json.loads(line) for line in p.read_text(encoding="utf-8").splitlines() if line.strip()]


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--upstream", required=True)
    ap.add_argument("--log-dir", required=True)
    ap.add_argument("--port", type=int, default=8100)
    ap.add_argument("--temperature", type=float, default=1.0)
    ap.add_argument("--top-p", type=float, default=0.95)
    ap.add_argument("--top-k", type=int, default=64)
    a = ap.parse_args()
    cfg = ProxyConfig(a.upstream, Path(a.log_dir), a.temperature, a.top_p, a.top_k)
    with RolloutProxy(cfg, a.port) as p:
        print(f"proxy on http://127.0.0.1:{a.port} -> {a.upstream}; episode endpoint e.g. "
              f"{p.endpoint('test')}")
        try:
            p.thread.join()
        except KeyboardInterrupt:
            pass


if __name__ == "__main__":
    main()
