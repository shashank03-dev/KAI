#!/usr/bin/env python3
"""Local web fixture for `kai bench`: static pages from ./site plus one POST
endpoint. Nothing leaves the machine.

    python3 bench/fixtures/server.py [--port 8765] [--out ~/kai-bench/server]

GET  /ping    -> "ok" (task setup uses it to see whether the server runs)
GET  /<file>  -> bench/fixtures/site/<file>
POST /submit  -> saves the form fields as JSON to <out>/submission.json
"""

import argparse
import http.server
import json
import os
import urllib.parse

SITE = os.path.join(os.path.dirname(os.path.abspath(__file__)), "site")


def make_handler(out_dir):
    class Handler(http.server.SimpleHTTPRequestHandler):
        def __init__(self, *a, **kw):
            super().__init__(*a, directory=SITE, **kw)

        def log_message(self, *_):
            pass

        def do_GET(self):
            if self.path == "/ping":
                self._reply(200, "text/plain", b"ok")
                return
            super().do_GET()

        def do_POST(self):
            if self.path != "/submit":
                self._reply(404, "text/plain", b"not found")
                return
            n = int(self.headers.get("Content-Length") or 0)
            body = self.rfile.read(n).decode("utf-8", "replace")
            fields = {k: v[-1] for k, v in urllib.parse.parse_qs(body).items()}
            os.makedirs(out_dir, exist_ok=True)
            with open(os.path.join(out_dir, "submission.json"), "w") as f:
                json.dump(fields, f, indent=2)
            page = (
                "<!doctype html><title>Thank you</title>"
                "<h1>Thank you, message received</h1>"
            ).encode()
            self._reply(200, "text/html; charset=utf-8", page)

        def _reply(self, code, ctype, data):
            self.send_response(code)
            self.send_header("Content-Type", ctype)
            self.send_header("Content-Length", str(len(data)))
            self.end_headers()
            self.wfile.write(data)

    return Handler


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--port", type=int, default=8765)
    p.add_argument("--out", default=os.path.expanduser("~/kai-bench/server"))
    a = p.parse_args()
    srv = http.server.ThreadingHTTPServer(("127.0.0.1", a.port), make_handler(a.out))
    srv.serve_forever()


if __name__ == "__main__":
    main()
