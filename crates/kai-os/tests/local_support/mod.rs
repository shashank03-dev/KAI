//! A tiny HTTP/1.1 file server for the download tests: range requests,
//! a connection cut part-way through, a server that ignores ranges, and a
//! request log. Plus builders for a fake `llama-server` runtime.

#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[derive(Clone, Default)]
pub struct Route {
    pub body: Vec<u8>,
    /// Close the connection after this many body bytes, on the first GET only.
    pub cut_first: Option<usize>,
    /// Answer 200 with the whole body even when a range is asked for.
    pub ignore_range: bool,
    /// Pause between 64 KiB pieces (lets a test cancel mid-way).
    pub slow: bool,
}

/// (path, Range header) of each request.
pub type Log = Arc<Mutex<Vec<(String, Option<String>)>>>;

pub struct Mock {
    pub base: String,
    pub log: Log,
}

pub async fn serve(routes: HashMap<String, Route>) -> Mock {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let log: Log = Arc::default();
    let routes = Arc::new(Mutex::new(routes));
    let log2 = log.clone();
    tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else {
                return;
            };
            let routes = routes.clone();
            let log = log2.clone();
            tokio::spawn(async move {
                let mut buf = Vec::new();
                let mut tmp = [0u8; 4096];
                while !buf.windows(4).any(|w| w == b"\r\n\r\n") {
                    match sock.read(&mut tmp).await {
                        Ok(0) | Err(_) => return,
                        Ok(n) => buf.extend_from_slice(&tmp[..n]),
                    }
                }
                let head = String::from_utf8_lossy(&buf).to_string();
                let path = head.split_whitespace().nth(1).unwrap_or("/").to_string();
                let range = head
                    .lines()
                    .find(|l| l.to_ascii_lowercase().starts_with("range:"))
                    .map(|l| l.split_once(':').unwrap().1.trim().to_string());
                log.lock().unwrap().push((path.clone(), range.clone()));
                let found = {
                    let mut r = routes.lock().unwrap();
                    r.get_mut(&path).map(|route| {
                        let snapshot = route.clone();
                        route.cut_first = None;
                        snapshot
                    })
                };
                let Some(route) = found else {
                    let _ = sock
                        .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                        .await;
                    return;
                };
                let len = route.body.len();
                let start = range
                    .as_deref()
                    .and_then(|r| r.strip_prefix("bytes="))
                    .and_then(|r| r.trim_end_matches('-').parse::<usize>().ok())
                    .filter(|_| !route.ignore_range);
                let (status, from) = match start {
                    Some(s) if s >= len => {
                        let h = format!(
                            "HTTP/1.1 416 Range Not Satisfiable\r\nContent-Range: bytes */{len}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                        );
                        let _ = sock.write_all(h.as_bytes()).await;
                        return;
                    }
                    Some(s) => (
                        format!(
                            "206 Partial Content\r\nContent-Range: bytes {s}-{}/{len}",
                            len - 1
                        ),
                        s,
                    ),
                    None => ("200 OK".to_string(), 0),
                };
                let h = format!(
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\nConnection: close\r\n\r\n",
                    len - from
                );
                if sock.write_all(h.as_bytes()).await.is_err() {
                    return;
                }
                let end = route.cut_first.map(|c| (from + c).min(len)).unwrap_or(len);
                for piece in route.body[from..end].chunks(64 * 1024) {
                    if sock.write_all(piece).await.is_err() {
                        return;
                    }
                    if route.slow {
                        tokio::time::sleep(Duration::from_millis(15)).await;
                    }
                }
                let _ = sock.flush().await;
                let _ = sock.shutdown().await;
            });
        }
    });
    Mock { base, log }
}

/// Deterministic pseudo-random bytes.
pub fn bytes(n: usize, seed: u8) -> Vec<u8> {
    let mut x: u32 = 0x9e37_79b9 ^ seed as u32;
    (0..n)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            x as u8
        })
        .collect()
}

pub fn sha256(data: &[u8]) -> String {
    use sha2::Digest;
    kai_os::local::download::hex(&sha2::Sha256::digest(data))
}

/// A fake `llama-server`: a shell script that execs a Python HTTP server on
/// `--port`, answering `/health` 503 twice ("loading"), then 200. It records
/// its arguments in `args.txt` next to itself.
pub const FAKE_SERVER_SH: &str = r#"#!/bin/sh
here="$(dirname "$0")"
printf '%s\n' "$@" > "$here/args.txt"
exec python3 -I "$here/fake_server.py" "$@"
"#;

pub const FAKE_SERVER_PY: &str = r#"
import http.server, sys
args = sys.argv[1:]
host = args[args.index("--host") + 1]
port = int(args[args.index("--port") + 1])
hits = {"n": 0}
class H(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path != "/health":
            self.send_response(404); self.end_headers(); return
        hits["n"] += 1
        code = 503 if hits["n"] <= 2 else 200
        body = b'{"status":"ok"}' if code == 200 else b'{"error":{"code":503,"message":"Loading model"}}'
        self.send_response(code)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)
    def log_message(self, *a):
        pass
http.server.HTTPServer((host, port), H).serve_forever()
"#;

/// The fake runtime as a `.tar.gz` with a `llama-test/` folder, like the
/// real Linux release.
pub fn fake_runtime_tgz() -> Vec<u8> {
    let mut out = Vec::new();
    {
        let gz = flate2::write::GzEncoder::new(&mut out, flate2::Compression::fast());
        let mut t = tar::Builder::new(gz);
        for (name, data, mode) in [
            ("llama-test/llama-server", FAKE_SERVER_SH, 0o755),
            ("llama-test/fake_server.py", FAKE_SERVER_PY, 0o644),
        ] {
            let mut h = tar::Header::new_gnu();
            h.set_size(data.len() as u64);
            h.set_mode(mode);
            h.set_cksum();
            t.append_data(&mut h, name, data.as_bytes()).unwrap();
        }
        t.into_inner().unwrap().finish().unwrap();
    }
    out
}

/// A flat `.zip` (like the Windows releases) holding `llama-server.exe`.
pub fn fake_runtime_zip() -> Vec<u8> {
    use std::io::Write;
    let mut cur = std::io::Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut cur);
        let opt = zip::write::SimpleFileOptions::default();
        z.start_file("llama-server.exe", opt).unwrap();
        z.write_all(b"not a real exe").unwrap();
        z.start_file("ggml.dll", opt).unwrap();
        z.write_all(b"dll").unwrap();
        z.finish().unwrap();
    }
    cur.into_inner()
}
