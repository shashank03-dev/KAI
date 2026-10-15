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
        let out = child.stdout.take().expect("piped");
        let err = child.stderr.take().expect("piped");
        let out_buf = Captured::default();
        let err_buf = Captured::default();
        let read_out = tokio::spawn(read_capped(out, out_buf.clone()));
        let read_err = tokio::spawn(read_capped(err, err_buf.clone()));

        let started = Instant::now();
        // Return the moment the command exits; check the kill switch and the
        // timeout every 50 ms meanwhile.
        let status = loop {
            tokio::select! {
                s = child.wait() => break Some(s?),
                _ = tokio::time::sleep(Duration::from_millis(50)) => {
                    if self.kill.is_tripped() || started.elapsed() > self.timeout {
                        kill_tree(&mut child).await;
                        break None;
                    }
                }
            }
        };

        // An app the command started in the background (e.g. `gio open`) can
        // inherit the pipes and keep them open forever: after the command
        // exits, give the readers a moment, then use what was read.
        let grace = tokio::time::sleep(OUTPUT_GRACE);
        tokio::pin!(grace);
        let mut detached = false;
        for reader in [read_out, read_err] {
            tokio::select! {
                _ = reader => {}
                _ = &mut grace => detached = true,
            }
        }
        let (stdout, out_more) = out_buf.take();
        let (stderr, err_more) = err_buf.take();
        let mut text = stdout;
        if detached {
            text.push_str("\n(a program it started is still running in the background)");
        }
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
    #[cfg(target_os = "macos")]
    c.env(
        "PATH",
        with_mac_tools(&std::env::var("PATH").unwrap_or_default()),
    );
    c
}

/// Where Mac command-line tools live that an app opened from the Finder
/// does not see (its `PATH` is only `/usr/bin:/bin:/usr/sbin:/sbin`):
/// Homebrew on Apple Silicon, then Homebrew on Intel and manual installs.
#[cfg(any(target_os = "macos", all(test, unix)))]
const MAC_TOOL_DIRS: &[&str] = &["/opt/homebrew/bin", "/opt/homebrew/sbin", "/usr/local/bin"];

/// `path` plus each of [`MAC_TOOL_DIRS`] it is missing, appended so the
/// person's own order still wins.
#[cfg(any(target_os = "macos", all(test, unix)))]
fn with_mac_tools(path: &str) -> String {
    let have: Vec<&str> = path.split(':').filter(|d| !d.is_empty()).collect();
    let missing = MAC_TOOL_DIRS.iter().filter(|d| !have.contains(d));
    have.iter()
        .copied()
        .chain(missing.copied())
        .collect::<Vec<_>>()
        .join(":")
}

fn home() -> std::path::PathBuf {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .map(Into::into)
        .unwrap_or_else(std::env::temp_dir)
}

/// Read a stream fully but keep only the first `MAX_OUTPUT` bytes.
/// How long readers may run after the command itself has exited.
const OUTPUT_GRACE: Duration = Duration::from_millis(500);

/// Output read so far, shared with the reader task, so a reader that never
/// sees EOF still leaves its text behind.
#[derive(Clone, Default)]
struct Captured(std::sync::Arc<std::sync::Mutex<(Vec<u8>, bool)>>);

impl Captured {
    fn take(&self) -> (String, bool) {
        let g = self.0.lock().unwrap_or_else(|e| e.into_inner());
        (String::from_utf8_lossy(&g.0).into_owned(), g.1)
    }
}

async fn read_capped<R: tokio::io::AsyncRead + Unpin>(mut r: R, into: Captured) {
    let mut buf = [0u8; 8192];
    while let Ok(n) = r.read(&mut buf).await {
        if n == 0 {
            break;
        }
        let mut g = into.0.lock().unwrap_or_else(|e| e.into_inner());
        let room = MAX_OUTPUT.saturating_sub(g.0.len());
        g.0.extend_from_slice(&buf[..n.min(room)]);
        g.1 |= n > room;
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_background_child_holding_the_pipe_does_not_hang() {
        let sh = Shell::new(KillSwitch::new());
        let t = std::time::Instant::now();
        let out = sh.run("echo started; (sleep 30 &)").await.unwrap();
        assert!(
            t.elapsed() < Duration::from_secs(5),
            "took {:?}",
            t.elapsed()
        );
        assert!(out.starts_with("started"));
        assert!(out.ends_with("[exit 0]"));
    }

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

    #[test]
    fn finder_path_gains_homebrew_without_reordering() {
        assert_eq!(
            with_mac_tools("/usr/bin:/bin:/usr/sbin:/sbin"),
            "/usr/bin:/bin:/usr/sbin:/sbin:/opt/homebrew/bin:/opt/homebrew/sbin:/usr/local/bin"
        );
        let mine = "/Users/me/bin:/opt/homebrew/bin:/usr/bin";
        assert_eq!(
            with_mac_tools(mine),
            "/Users/me/bin:/opt/homebrew/bin:/usr/bin:/opt/homebrew/sbin:/usr/local/bin"
        );
        assert_eq!(with_mac_tools(""), MAC_TOOL_DIRS.join(":"));
    }
}
