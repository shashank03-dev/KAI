"""Offline test of train/rollout_proxy.py against a fake vLLM upstream.

    python train/tests/test_proxy.py
Sends a Kai-shaped streaming request (system + text + JPEG, temperature 0,
json_schema response_format) and checks that the upstream saw the sampling
overrides, Kai gets valid SSE back, and calls.jsonl holds the exact prompt,
image file, reply and logprobs.
"""

from __future__ import annotations

import base64
import io
import json
import sys
import tempfile
import threading
import urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from rollout_proxy import ProxyConfig, RolloutProxy, read_calls  # noqa: E402

SEEN = {}
REPLY = '{"thought":"open it","action":"open_app","text":"mousepad","risk":"none"}'


class FakeVllm(BaseHTTPRequestHandler):
    def log_message(self, *a):
        pass

    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        SEEN.update(body=body, path=self.path, auth=self.headers.get("Authorization"))
        resp = {"id": "x", "model": body["model"],
                "choices": [{"index": 0, "finish_reason": "stop",
                             "message": {"role": "assistant", "content": REPLY},
                             "logprobs": {"content": [{"token": "{", "logprob": -0.01},
                                                      {"token": "\"", "logprob": -0.2}]}}],
                "usage": {"prompt_tokens": 1900, "completion_tokens": 30}}
        data = json.dumps(resp).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)


def main():
    from PIL import Image
    up = ThreadingHTTPServer(("127.0.0.1", 0), FakeVllm)
    threading.Thread(target=up.serve_forever, daemon=True).start()
    buf = io.BytesIO()
    Image.new("RGB", (1070, 602), (200, 200, 200)).save(buf, "JPEG")
    b64 = base64.b64encode(buf.getvalue()).decode()
    with tempfile.TemporaryDirectory() as tmp:
        cfg = ProxyConfig(f"http://127.0.0.1:{up.server_port}/v1", Path(tmp), temperature=0.9)
        with RolloutProxy(cfg, port=0) as proxy:
            req = {"model": "kai", "max_tokens": 192, "temperature": 0.0, "stream": True,
                   "stream_options": {"include_usage": True},
                   "messages": [{"role": "system", "content": "You are Kai."},
                                {"role": "user", "content": [
                                    {"type": "text", "text": "Elements:\n#1 button \"OK\"\n\nTask: t"},
                                    {"type": "image_url",
                                     "image_url": {"url": f"data:image/jpeg;base64,{b64}"}}]}],
                   "response_format": {"type": "json_schema", "json_schema": {"name": "kai_reply",
                                                                               "schema": {}, "strict": True}}}
            r = urllib.request.urlopen(urllib.request.Request(
                proxy.endpoint("it000-write-note-3") + "/chat/completions",
                data=json.dumps(req).encode(), method="POST",
                headers={"Content-Type": "application/json", "Authorization": "Bearer local"}))
            sse = r.read().decode()
            assert r.headers["Content-Type"] == "text/event-stream", r.headers
        # what Kai's stream reader would accumulate
        content = ""
        for line in sse.splitlines():
            if line.startswith("data:") and line[5:].strip() != "[DONE]":
                for ch in json.loads(line[5:]).get("choices", []):
                    content += ch.get("delta", {}).get("content") or ""
        assert content == REPLY, content
        assert sse.rstrip().endswith("data: [DONE]")
        b = SEEN["body"]
        assert SEEN["path"] == "/v1/chat/completions", SEEN["path"]
        assert b["temperature"] == 0.9 and b["top_p"] == 0.95 and b["top_k"] == 64
        assert b["logprobs"] is True and "stream" not in b
        assert b["response_format"]["type"] == "json_schema"
        assert SEEN["auth"] == "Bearer local"
        calls = read_calls(Path(tmp), "it000-write-note-3")
        assert len(calls) == 1
        c = calls[0]
        assert c["system"] == "You are Kai." and c["user"].endswith("Task: t")
        assert c["reply"] == REPLY and c["logprobs"] == [-0.01, -0.2]
        assert Image.open(c["images"][0]).size == (1070, 602)
        assert "local" not in json.dumps(c)  # the API key is never logged
    up.shutdown()
    print("proxy test OK: SSE re-stream, sampling overrides, call log with image + logprobs")


if __name__ == "__main__":
    main()
