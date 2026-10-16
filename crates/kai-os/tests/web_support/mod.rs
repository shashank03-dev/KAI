//! Shared by the browser lane tests: a tiny local web server and a headless
//! lane. Tests skip (pass with a note) when no Chromium-family browser is
//! found; set `KAI_BROWSER` to point at one.

#![allow(dead_code)]

use kai_core::{Decision, Grounding, Role};
use kai_os::web::{BrowserLane, LaneOptions, find_browser};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Serve `pages` (path without query → HTML). `/echo?…` answers with a page
/// titled with the query string; anything else is a 404. Returns the base
/// URL (`http://127.0.0.1:<port>`).
pub async fn serve(pages: Vec<(&'static str, String)>) -> String {
    let pages: Arc<HashMap<&'static str, String>> = Arc::new(pages.into_iter().collect());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let Ok((mut s, _)) = listener.accept().await else {
                return;
            };
            let pages = pages.clone();
            tokio::spawn(async move {
                let mut buf = vec![0u8; 8192];
                let n = s.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]);
                let target = req.split_whitespace().nth(1).unwrap_or("/").to_string();
                let (path, query) = target.split_once('?').unwrap_or((&target, ""));
                let (status, body, extra) = if path == "/echo" {
                    let q = query.replace('+', " ");
                    (
                        "200 OK",
                        format!("<title>echo {q}</title><p>echo {q}</p>"),
                        "",
                    )
                } else if path == "/file.txt" {
                    (
                        "200 OK",
                        "hello from kai".to_string(),
                        "Content-Disposition: attachment; filename=\"file.txt\"\r\n",
                    )
                } else if let Some(p) = pages.get(path) {
                    ("200 OK", p.clone(), "")
                } else {
                    ("404 Not Found", "<title>Missing</title>".to_string(), "")
                };
                let kind = if path == "/file.txt" {
                    "text/plain"
                } else {
                    "text/html; charset=utf-8"
                };
                let resp = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: {kind}\r\n{extra}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = s.write_all(resp.as_bytes()).await;
            });
        }
    });
    format!("http://{addr}")
}

/// A headless lane with a fresh profile, or `None` (skip) without a browser.
pub async fn lane() -> Option<(BrowserLane, tempfile::TempDir)> {
    lane_with(None).await
}

pub async fn lane_with(
    downloads: Option<std::path::PathBuf>,
) -> Option<(BrowserLane, tempfile::TempDir)> {
    let Some(browser) = find_browser() else {
        // CI sets KAI_REQUIRE_BROWSER so a missing browser fails, not passes.
        assert!(
            std::env::var_os("KAI_REQUIRE_BROWSER").is_none(),
            "KAI_REQUIRE_BROWSER is set but no Chromium-family browser was found"
        );
        eprintln!("no Chromium-family browser found (set KAI_BROWSER); skipping");
        return None;
    };
    let profile = tempfile::tempdir().unwrap();
    let opts = LaneOptions {
        browser,
        profile: profile.path().to_path_buf(),
        headless: true,
        size: (1280, 800),
        downloads,
    };
    Some((BrowserLane::launch(&opts, None).await.unwrap(), profile))
}

/// The mark of the first element with this role whose name contains `name`.
pub fn mark(g: &Grounding, role: Role, name: &str) -> u32 {
    let Grounding::Marks(els) = g else {
        panic!("expected marks, got {g:?}")
    };
    els.iter()
        .find(|e| e.role == role && e.name.contains(name))
        .unwrap_or_else(|| panic!("no {role:?} {name:?} in {}", g.as_text()))
        .mark
}

pub fn decision(json: &str) -> Decision {
    Decision::parse_checked(json).unwrap()
}

pub fn click(mark: u32) -> Decision {
    decision(&format!(
        r#"{{"thought":"","action":"click","target":{mark},"risk":"none"}}"#
    ))
}

pub fn type_into(mark: u32, text: &str) -> Decision {
    decision(&format!(
        r#"{{"thought":"","action":"type","target":{mark},"text":"{text}","risk":"low"}}"#
    ))
}

pub fn key(keys: &str) -> Decision {
    decision(&format!(
        r#"{{"thought":"","action":"key","keys":{keys},"risk":"none"}}"#
    ))
}
