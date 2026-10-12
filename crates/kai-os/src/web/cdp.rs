//! A minimal Chrome DevTools Protocol client over `--remote-debugging-pipe`:
//! the browser reads commands on its fd 3 and writes replies on its fd 4,
//! each message JSON ending in a NUL byte. A pipe, unlike a debugging port,
//! cannot be reached by any other process on the machine.

use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::Child;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::oneshot;

type Pending = Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value, String>>>>>;

/// Longest one protocol call may take (navigation waits separately).
pub const CALL_TIMEOUT: Duration = Duration::from_secs(15);

/// One browser process and the pipe to it. Dropping it closes the browser.
pub struct Cdp {
    writer: Mutex<std::fs::File>,
    pending: Pending,
    next: AtomicU64,
    child: Mutex<Child>,
}

impl Cdp {
    /// Start `browser` with `args` and the debugging pipe attached.
    #[cfg(target_os = "linux")]
    pub fn launch(browser: &Path, args: &[String]) -> anyhow::Result<Self> {
        use std::os::fd::{FromRawFd, OwnedFd};
        use std::os::unix::process::CommandExt;
        use std::process::{Command, Stdio};

        // `to_child` carries commands (the child's fd 3), `from_child` the
        // replies (the child's fd 4).
        let (child_reads, we_write) = pipe()?;
        let (we_read, child_writes) = pipe()?;
        let (r, w) = (child_reads.raw(), child_writes.raw());
        let mut cmd = Command::new(browser);
        cmd.args(args)
            .arg("--remote-debugging-pipe")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        // SAFETY: only async-signal-safe calls (fcntl, dup2) between fork
        // and exec. The fds are first moved above 4 so dup2 onto 3/4 can
        // never clobber the other one; dup2 clears close-on-exec.
        unsafe {
            cmd.pre_exec(move || {
                let r2 = libc::fcntl(r, libc::F_DUPFD, 10);
                let w2 = libc::fcntl(w, libc::F_DUPFD, 10);
                if r2 < 0 || w2 < 0 || libc::dup2(r2, 3) < 0 || libc::dup2(w2, 4) < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let child = cmd.spawn()?;
        drop((child_reads, child_writes));
        // SAFETY: both fds came from pipe2 above and are owned only here.
        let writer = unsafe { std::fs::File::from(OwnedFd::from_raw_fd(we_write.into_raw())) };
        let reader = unsafe { std::fs::File::from(OwnedFd::from_raw_fd(we_read.into_raw())) };

        let pending: Pending = Arc::default();
        let p = pending.clone();
        std::thread::Builder::new()
            .name("kai-cdp-read".into())
            .spawn(move || read_loop(reader, p))?;
        Ok(Self {
            writer: Mutex::new(writer),
            pending,
            next: AtomicU64::new(1),
            child: Mutex::new(child),
        })
    }

    #[cfg(not(target_os = "linux"))]
    pub fn launch(_browser: &Path, _args: &[String]) -> anyhow::Result<Self> {
        anyhow::bail!("Kai's browser lane runs on Linux for now (docs/BROWSER.md)")
    }

    /// Send one command (to the browser, or to an attached page `session`)
    /// and wait for its result.
    pub async fn call(
        &self,
        method: &str,
        params: Value,
        session: Option<&str>,
    ) -> anyhow::Result<Value> {
        self.call_within(method, params, session, CALL_TIMEOUT)
            .await
    }

    pub async fn call_within(
        &self,
        method: &str,
        params: Value,
        session: Option<&str>,
        limit: Duration,
    ) -> anyhow::Result<Value> {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let mut msg = json!({ "id": id, "method": method, "params": params });
        if let Some(s) = session {
            msg["sessionId"] = json!(s);
        }
        let (tx, rx) = oneshot::channel();
        self.lock_pending().insert(id, tx);
        let mut bytes = serde_json::to_vec(&msg)?;
        bytes.push(0);
        let sent = self
            .writer
            .lock()
            .map_err(|_| anyhow::anyhow!("browser pipe lock poisoned"))
            .and_then(|mut w| w.write_all(&bytes).map_err(Into::into));
        if let Err(e) = sent {
            self.lock_pending().remove(&id);
            anyhow::bail!("browser pipe closed: {e}");
        }
        match tokio::time::timeout(limit, rx).await {
            Ok(Ok(Ok(v))) => Ok(v),
            Ok(Ok(Err(e))) => anyhow::bail!("{method}: {e}"),
            Ok(Err(_)) => anyhow::bail!("{method}: the browser closed"),
            Err(_) => {
                self.lock_pending().remove(&id);
                anyhow::bail!("{method}: no reply within {limit:?}")
            }
        }
    }

    /// Whether the browser process is still running.
    pub fn alive(&self) -> bool {
        self.child
            .lock()
            .map(|mut c| matches!(c.try_wait(), Ok(None)))
            .unwrap_or(false)
    }

    fn lock_pending(
        &self,
    ) -> std::sync::MutexGuard<'_, HashMap<u64, oneshot::Sender<Result<Value, String>>>> {
        self.pending.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl Drop for Cdp {
    fn drop(&mut self) {
        if let Ok(mut c) = self.child.lock() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

/// Route each NUL-terminated reply to its waiting caller. Events (no `id`)
/// are not used: the lane reads state by asking, which keeps it simple and
/// race-free. On EOF every waiting call fails.
fn read_loop(reader: std::fs::File, pending: Pending) {
    let mut r = BufReader::new(reader);
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match r.read_until(0, &mut buf) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        if buf.last() == Some(&0) {
            buf.pop();
        }
        let Ok(v) = serde_json::from_slice::<Value>(&buf) else {
            continue;
        };
        let Some(id) = v.get("id").and_then(Value::as_u64) else {
            continue;
        };
        let reply = match v.get("error") {
            Some(e) => Err(e
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("protocol error")
                .to_string()),
            None => Ok(v.get("result").cloned().unwrap_or(Value::Null)),
        };
        let tx = pending.lock().ok().and_then(|mut p| p.remove(&id));
        if let Some(tx) = tx {
            let _ = tx.send(reply);
        }
    }
    if let Ok(mut p) = pending.lock() {
        for (_, tx) in p.drain() {
            let _ = tx.send(Err("the browser closed".into()));
        }
    }
}

/// A pipe end that closes itself unless handed on.
#[cfg(target_os = "linux")]
struct Fd(i32);

#[cfg(target_os = "linux")]
impl Fd {
    fn raw(&self) -> i32 {
        self.0
    }
    fn into_raw(self) -> i32 {
        let fd = self.0;
        std::mem::forget(self);
        fd
    }
}

#[cfg(target_os = "linux")]
impl Drop for Fd {
    fn drop(&mut self) {
        // SAFETY: we own this fd and close it once.
        unsafe { libc::close(self.0) };
    }
}

/// `(read end, write end)`, both close-on-exec so no other child inherits
/// them.
#[cfg(target_os = "linux")]
fn pipe() -> anyhow::Result<(Fd, Fd)> {
    let mut fds = [0i32; 2];
    // SAFETY: `fds` has room for the two descriptors pipe2 writes.
    if unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) } != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok((Fd(fds[0]), Fd(fds[1])))
}
