//! Resumable, verified downloads: `<file>.part` grows with HTTP range
//! requests (after a dropped connection, a cancel, or a restart of Kai), the
//! SHA-256 is computed while streaming (the part already on disk is hashed
//! first), and only a verified file is renamed into place.

use anyhow::Context;
use futures::StreamExt;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Shared cancel flag for an install. Cloning shares the flag.
#[derive(Debug, Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
    pub fn reset(&self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

/// The error an install returns when it was cancelled. The `.part` files stay,
/// so the next install resumes. Check with `err.downcast_ref::<Cancelled>()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cancelled;

impl std::fmt::Display for Cancelled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("download cancelled")
    }
}

impl std::error::Error for Cancelled {}

/// How many times a dropped connection is resumed before giving up.
const MAX_ATTEMPTS: u32 = 6;

/// An HTTP client for big downloads: connect and read timeouts (no total
/// timeout), the system proxy, and extra roots from `SSL_CERT_FILE`.
pub fn download_client() -> reqwest::Client {
    let mut b = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(30))
        .read_timeout(Duration::from_secs(90))
        .user_agent(concat!("kai/", env!("CARGO_PKG_VERSION")));
    if let Some(path) = std::env::var_os("SSL_CERT_FILE")
        && let Ok(pem) = std::fs::read(&path)
        && let Ok(certs) = reqwest::Certificate::from_pem_bundle(&pem)
    {
        for c in certs {
            b = b.add_root_certificate(c);
        }
    }
    b.build().unwrap_or_default()
}

pub fn part_path(dest: &Path) -> PathBuf {
    let mut s = dest.as_os_str().to_owned();
    s.push(".part");
    PathBuf::from(s)
}

/// Bytes already on disk for `dest` (finished file or `.part`).
pub fn bytes_present(dest: &Path) -> u64 {
    if let Ok(m) = std::fs::metadata(dest) {
        return m.len();
    }
    std::fs::metadata(part_path(dest))
        .map(|m| m.len())
        .unwrap_or(0)
}

/// Download `url` to `dest`. `on_bytes(done, total)` reports this file's
/// bytes. A finished `dest` of the expected size is kept as is.
pub async fn fetch(
    http: &reqwest::Client,
    url: &str,
    dest: &Path,
    sha256: Option<&str>,
    size: Option<u64>,
    cancel: &CancelToken,
    on_bytes: &mut (dyn FnMut(u64, Option<u64>) + Send),
) -> anyhow::Result<()> {
    if let Ok(m) = std::fs::metadata(dest)
        && size.is_none_or(|s| s == m.len())
    {
        on_bytes(m.len(), Some(m.len()));
        return Ok(());
    }
    if let Some(dir) = dest.parent() {
        tokio::fs::create_dir_all(dir).await?;
    }
    let part = part_path(dest);
    let mut have = std::fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
    if size.is_some_and(|s| have > s) {
        have = 0;
    }
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&part)
        .await
        .with_context(|| format!("cannot write {}", part.display()))?;
    file.set_len(have).await?;
    let mut hasher = Sha256::new();
    if have > 0 {
        hash_prefix(&part, have, &mut hasher, cancel).await?;
    }
    on_bytes(have, size);

    let mut attempt = 0;
    let mut total = size;
    'outer: loop {
        if cancel.is_cancelled() {
            return Err(Cancelled.into());
        }
        if total.is_some_and(|t| have >= t) {
            break;
        }
        attempt += 1;
        let mut req = http.get(url);
        if have > 0 {
            req = req.header(reqwest::header::RANGE, format!("bytes={have}-"));
        }
        let resp = match req.send().await {
            Ok(r) => r,
            Err(e) if attempt < MAX_ATTEMPTS => {
                log::warn!("download of {url} failed ({e}); retrying");
                tokio::time::sleep(Duration::from_millis(500 * attempt as u64)).await;
                continue;
            }
            Err(e) => return Err(anyhow::Error::new(e).context(format!("downloading {url}"))),
        };
        let status = resp.status().as_u16();
        match status {
            206 => {}
            200 => {
                // The server ignored the range: start over.
                if have > 0 {
                    file.set_len(0).await?;
                    have = 0;
                    hasher = Sha256::new();
                }
            }
            416 if have > 0 => {
                // Asked past the end: what we have may already be complete;
                // otherwise start over once.
                if total.is_none() || total == Some(have) {
                    total = Some(have);
                    break;
                }
                file.set_len(0).await?;
                have = 0;
                hasher = Sha256::new();
                continue;
            }
            _ => anyhow::bail!("downloading {url}: server answered HTTP {status}"),
        }
        if total.is_none() {
            total = resp.content_length().map(|n| n + have);
        }
        let mut stream = resp.bytes_stream();
        let mut since_report = 0u64;
        let start_of_attempt = have;
        while let Some(chunk) = stream.next().await {
            if cancel.is_cancelled() {
                file.flush().await?;
                return Err(Cancelled.into());
            }
            let chunk = match chunk {
                Ok(c) => c,
                Err(e) if attempt < MAX_ATTEMPTS => {
                    log::warn!("download of {url} dropped at {have} bytes ({e}); resuming");
                    if have - start_of_attempt >= 1 << 20 {
                        attempt = 0; // it was making progress: a fresh budget
                    }
                    file.flush().await?;
                    tokio::time::sleep(Duration::from_millis(500 * attempt as u64)).await;
                    continue 'outer;
                }
                Err(e) => return Err(anyhow::Error::new(e).context(format!("downloading {url}"))),
            };
            file.write_all(&chunk).await?;
            hasher.update(&chunk);
            have += chunk.len() as u64;
            since_report += chunk.len() as u64;
            if since_report >= 1 << 20 {
                since_report = 0;
                on_bytes(have, total);
            }
        }
        if total.is_none_or(|t| have >= t) {
            break;
        }
        // The body ended early without an error: resume.
        if attempt >= MAX_ATTEMPTS {
            anyhow::bail!("downloading {url}: connection kept closing early at {have} bytes");
        }
    }
    file.flush().await?;
    file.sync_all().await?;
    drop(file);
    on_bytes(have, total);

    if let Some(s) = size
        && s != have
    {
        let _ = std::fs::remove_file(&part);
        anyhow::bail!("downloading {url}: expected {s} bytes, got {have}");
    }
    if let Some(want) = sha256 {
        let got = hex(&hasher.finalize());
        if !got.eq_ignore_ascii_case(want) {
            let _ = std::fs::remove_file(&part);
            anyhow::bail!(
                "checksum mismatch for {}: expected sha256 {want}, got {got}; the file was \
                 deleted, try again",
                dest.file_name().unwrap_or_default().to_string_lossy()
            );
        }
    }
    tokio::fs::rename(&part, dest)
        .await
        .with_context(|| format!("cannot move {} into place", part.display()))?;
    Ok(())
}

async fn hash_prefix(
    path: &Path,
    len: u64,
    hasher: &mut Sha256,
    cancel: &CancelToken,
) -> anyhow::Result<()> {
    let mut f = tokio::fs::File::open(path).await?;
    let mut buf = vec![0u8; 1 << 20];
    let mut left = len;
    while left > 0 {
        if cancel.is_cancelled() {
            return Err(Cancelled.into());
        }
        let want = buf.len().min(left as usize);
        let n = f.read(&mut buf[..want]).await?;
        anyhow::ensure!(n > 0, "{} is shorter than expected", path.display());
        hasher.update(&buf[..n]);
        left -= n as u64;
    }
    Ok(())
}

/// SHA-256 of a whole file, lower-case hex.
pub async fn sha256_file(path: &Path) -> anyhow::Result<String> {
    let len = tokio::fs::metadata(path).await?.len();
    let mut h = Sha256::new();
    hash_prefix(path, len, &mut h, &CancelToken::new()).await?;
    Ok(hex(&h.finalize()))
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
