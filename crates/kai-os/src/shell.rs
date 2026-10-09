//! Run a terminal command for the model and hand back what it printed.
//! PowerShell on Windows, `sh` elsewhere. Bounded in time and output, and
//! killed the moment the kill switch trips.

use crate::kill::KillSwitch;
use std::process::Stdio;
use std::time::{Duration, Instant};
use tokio::io::AsyncReadExt;
use tokio::process::Command;

/// Most bytes of output kept per stream; the rest is summarised.
pub const MAX_OUTPUT: usize = 12_000;

pub struct Shell {
    pub timeout: Duration,
    pub kill: KillSwitch,
}

impl Shell {
    pub fn new(kill: KillSwitch) -> Self {
        Self {
            timeout: Duration::from_secs(180),
            kill,
        }
    }

    pub async fn run(&self, cmd: &str) -> anyhow::Result<String> {
        let mut child = command(cmd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .current_dir(home())
            .spawn()?;
        let mut out = child.stdout.take().expect("piped");
        let mut err = child.stderr.take().expect("piped");
        let read_out = tokio::spawn(async move { read_capped(&mut out).await });
        let read_err = tokio::spawn(async move { read_capped(&mut err).await });

        let started = Instant::now();
        let status = loop {
            if let Some(s) = child.try_wait()? {
                break Some(s);
            }
            if self.kill.is_tripped() || started.elapsed() > self.timeout {
                kill_tree(&mut child).await;
                break None;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        };

        let (stdout, out_more) = read_out.await?;
        let (stderr, err_more) = read_err.await?;
        let mut text = stdout;
        if out_more {
            text.push_str("\n…(output truncated)");
        }
        if !stderr.trim().is_empty() {
            text.push_str("\n[stderr]\n");
            text.push_str(&stderr);
            if err_more {
                text.push_str("\n…(stderr truncated)");
            }
        }
        let footer = match status {
            Some(s) => format!(
                "\n[exit {}]",
                s.code()
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| "signal".into())
            ),
            None if self.kill.is_tripped() => "\n[stopped by the kill switch]".to_string(),
            None => format!(
                "\n[timed out after {}s and was stopped]",
                self.timeout.as_secs()
            ),
        };
        text.push_str(&footer);
        Ok(text)
    }
}

/// Stop the command and everything it started, so no grandchild keeps the
/// output pipes (or the work) alive.
#[cfg(unix)]
async fn kill_tree(child: &mut tokio::process::Child) {
    if let Some(pid) = child.id() {
        // The child leads its own process group (see `command`).
        // SAFETY: kill(2) with a negative pid signals that group; no memory is touched.
        unsafe {
            libc::kill(-(pid as i32), libc::SIGKILL);
        }
    }
    let _ = child.kill().await;
}

#[cfg(windows)]
async fn kill_tree(child: &mut tokio::process::Child) {
    if let Some(pid) = child.id() {
        let _ = Command::new("taskkill")
            .args(["/T", "/F", "/PID", &pid.to_string()])
            .creation_flags(0x0800_0000)
            .status()
            .await;
    }
    let _ = child.kill().await;
}

#[cfg(target_os = "windows")]
fn command(cmd: &str) -> Command {
    let mut c = Command::new("powershell");
    c.args([
        "-NoProfile",
        "-NonInteractive",
        "-ExecutionPolicy",
        "Bypass",
        "-Command",
        cmd,
    ]);
    c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    c
}

#[cfg(not(target_os = "windows"))]
fn command(cmd: &str) -> Command {
    let mut c = Command::new("sh");
    c.args(["-c", cmd]);
    c.process_group(0);
    c
}

fn home() -> std::path::PathBuf {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .map(Into::into)
        .unwrap_or_else(std::env::temp_dir)
}

/// Read a stream fully but keep only the first `MAX_OUTPUT` bytes.
async fn read_capped<R: tokio::io::AsyncRead + Unpin>(r: &mut R) -> (String, bool) {
    let mut kept = Vec::new();
    let mut more = false;
    let mut buf = [0u8; 8192];
    while let Ok(n) = r.read(&mut buf).await {
        if n == 0 {
            break;
        }
        let room = MAX_OUTPUT.saturating_sub(kept.len());
        kept.extend_from_slice(&buf[..n.min(room)]);
        more |= n > room;
    }
    (String::from_utf8_lossy(&kept).into_owned(), more)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[tokio::test]
    async fn captures_stdout_stderr_and_exit() {
        let sh = Shell::new(KillSwitch::new());
        let out = sh.run("echo hello; echo oops 1>&2; exit 3").await.unwrap();
        assert!(out.starts_with("hello"));
        assert!(out.contains("[stderr]\noops"));
        assert!(out.ends_with("[exit 3]"));
    }

    #[tokio::test]
    async fn output_is_capped() {
        let sh = Shell::new(KillSwitch::new());
        let out = sh
            .run("head -c 50000 /dev/zero | tr '\\0' a")
            .await
            .unwrap();
        assert!(out.contains("(output truncated)"));
        assert!(out.len() < MAX_OUTPUT + 200);
    }

    #[tokio::test]
    async fn times_out() {
        let mut sh = Shell::new(KillSwitch::new());
        sh.timeout = Duration::from_millis(200);
        let out = sh.run("sleep 5").await.unwrap();
        assert!(out.contains("timed out"));
    }

    #[tokio::test]
    async fn kill_switch_stops_a_running_command() {
        let kill = KillSwitch::new();
        let sh = Shell::new(kill.clone());
        let k2 = kill.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(150)).await;
            k2.trip("test");
        });
        let started = Instant::now();
        let out = sh.run("sleep 10").await.unwrap();
        assert!(out.contains("stopped by the kill switch"));
        assert!(started.elapsed() < Duration::from_secs(2));
    }
}
