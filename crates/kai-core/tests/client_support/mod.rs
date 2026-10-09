//! A tiny scripted HTTP/1.1 server for client tests: each request pops the next
//! scripted reply and is recorded (path + JSON body).
#![allow(dead_code)]

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

pub enum Reply {
    Json(u16, serde_json::Value),
    /// SSE body sent in chunks, each after a delay.
    Sse(Vec<(Duration, String)>),
}

#[derive(Clone, Default)]
pub struct Server {
    pub replies: Arc<Mutex<VecDeque<Reply>>>,
    pub seen: Arc<Mutex<Vec<(String, serde_json::Value)>>>,
}

impl Server {
    /// Start on 127.0.0.1; returns the base URL (no trailing slash).
    pub async fn start(replies: Vec<Reply>) -> (Self, String) {
        let srv = Server {
            replies: Arc::new(Mutex::new(replies.into())),
            seen: Arc::default(),
        };
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let s = srv.clone();
        tokio::spawn(async move {
            while let Ok((sock, _)) = listener.accept().await {
                let s = s.clone();
                tokio::spawn(async move { s.serve(sock).await });
            }
        });
        (srv, format!("http://{addr}"))
    }

    pub fn bodies(&self) -> Vec<(String, serde_json::Value)> {
        self.seen.lock().unwrap().clone()
    }

    async fn serve(&self, mut sock: TcpStream) {
        let mut buf = Vec::new();
        loop {
            // Read headers.
            let head_end = loop {
                if let Some(p) = find(&buf, b"\r\n\r\n") {
                    break p;
                }
                let mut tmp = [0u8; 8192];
                match sock.read(&mut tmp).await {
                    Ok(0) | Err(_) => return,
                    Ok(n) => buf.extend_from_slice(&tmp[..n]),
                }
            };
            let head = String::from_utf8_lossy(&buf[..head_end]).to_string();
            let path = head.split_whitespace().nth(1).unwrap_or("").to_string();
            let len = head
                .lines()
                .find_map(|l| {
                    let (k, v) = l.split_once(':')?;
                    k.eq_ignore_ascii_case("content-length")
                        .then(|| v.trim().parse::<usize>().ok())?
                })
                .unwrap_or(0);
            let total = head_end + 4 + len;
            while buf.len() < total {
                let mut tmp = [0u8; 65536];
                match sock.read(&mut tmp).await {
                    Ok(0) | Err(_) => return,
                    Ok(n) => buf.extend_from_slice(&tmp[..n]),
                }
            }
            let body: serde_json::Value =
                serde_json::from_slice(&buf[head_end + 4..total]).unwrap_or_default();
            buf.drain(..total);
            self.seen.lock().unwrap().push((path, body));
            let reply = self.replies.lock().unwrap().pop_front();
            let ok = match reply {
                Some(Reply::Json(code, v)) => {
                    let b = v.to_string();
                    let msg = format!(
                        "HTTP/1.1 {code} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{b}",
                        b.len()
                    );
                    sock.write_all(msg.as_bytes()).await.is_ok()
                }
                Some(Reply::Sse(chunks)) => {
                    let mut ok = sock
                        .write_all(
                            b"HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ntransfer-encoding: chunked\r\n\r\n",
                        )
                        .await
                        .is_ok();
                    for (delay, c) in chunks {
                        tokio::time::sleep(delay).await;
                        let part = format!("{:x}\r\n{c}\r\n", c.len());
                        ok = ok && sock.write_all(part.as_bytes()).await.is_ok();
                        let _ = sock.flush().await;
                    }
                    ok && sock.write_all(b"0\r\n\r\n").await.is_ok()
                }
                None => {
                    let msg = "HTTP/1.1 500 X\r\ncontent-length: 0\r\n\r\n";
                    sock.write_all(msg.as_bytes()).await.is_ok()
                }
            };
            if !ok {
                return;
            }
        }
    }
}

fn find(h: &[u8], n: &[u8]) -> Option<usize> {
    h.windows(n.len()).position(|w| w == n)
}

/// One SSE `data:` event for a content delta.
pub fn delta(text: &str) -> String {
    format!(
        "data: {}\n\n",
        serde_json::json!({"choices":[{"index":0,"delta":{"content":text}}]})
    )
}
