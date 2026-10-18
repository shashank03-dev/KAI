//! Start, watch and stop the local `llama-server`. It binds 127.0.0.1 only,
//! runs offline, and is killed when its [`LocalServer`] handle drops (the
//! app's exit). `server.json` in the data folder remembers a server that was
//! detached (`kai models start --detach`) so `stop()` and `status()` work from
//! another process.

use super::LocalManager;
use kai_core::models::{ModelEntry, local_base_url};
use serde::{Deserialize, Serialize};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tokio::process::{Child, Command};

/// What `server.json` records about a running server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerInfo {
    pub id: String,
    pub served_name: String,
    pub port: u16,
    pub pid: u32,
    /// `http://127.0.0.1:<port>/v1`
    pub base_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerStatus {
    pub info: ServerInfo,
    /// `/health` answered 200 (model loaded).
    pub healthy: bool,
}

/// A running server. Dropping it kills the process; [`LocalServer::detach`]
/// leaves it running.
#[derive(Debug)]
pub struct LocalServer {
    pub base_url: String,
    pub pid: u32,
    pub port: u16,
    pub id: String,
    child: Option<Child>,
    state: PathBuf,
}

impl LocalServer {
    fn remove_owned_state(&self) {
        let owned = std::fs::read_to_string(&self.state)
            .ok()
            .and_then(|text| serde_json::from_str::<ServerInfo>(&text).ok())
            .is_some_and(|info| info.pid == self.pid && info.id == self.id);
        if owned {
            let _ = std::fs::remove_file(&self.state);
        }
    }
    pub fn info(&self, served_name: &str) -> ServerInfo {
        ServerInfo {
            id: self.id.clone(),
            served_name: served_name.into(),
            port: self.port,
            pid: self.pid,
            base_url: self.base_url.clone(),
        }
    }

    /// Stop the server and wait for it to exit.
    pub async fn stop(mut self) -> anyhow::Result<()> {
        if let Some(mut c) = self.child.take() {
            let _ = c.kill().await;
        }
        self.remove_owned_state();
        Ok(())
    }

    /// Keep the server running after this handle is gone (CLI `--detach`).
    pub fn detach(mut self) {
        self.child.take();
    }

    /// Wait until the server exits on its own (CLI foreground mode).
    pub async fn wait(&mut self) -> anyhow::Result<()> {
        if let Some(c) = self.child.as_mut() {
            c.wait().await?;
        }
        Ok(())
    }
}

impl Drop for LocalServer {
    fn drop(&mut self) {
        if let Some(c) = self.child.as_mut() {
            let _ = c.start_kill();
            self.remove_owned_state();
        }
    }
}

/// The command line for `llama-server` (flags checked against the pinned
/// release's tools/server/README.md).
pub fn server_args(
    model: &Path,
    mmproj: Option<&Path>,
    port: u16,
    context: u32,
    alias: &str,
) -> Vec<OsString> {
    let mut a: Vec<OsString> = vec!["-m".into(), model.into()];
    if let Some(m) = mmproj {
        a.extend(["--mmproj".into(), m.into()]);
    }
    for s in [
        "--host",
        "127.0.0.1",
        "--port",
        &port.to_string(),
        "-c",
        &context.to_string(),
        "-ngl",
        "auto",
        "--alias",
        alias,
        "--jinja",
        "--no-webui",
        "--offline",
    ] {
        a.push(s.into());
    }
    a.extend(LEAN.iter().map(Into::into));
    a
}

/// How long `start` waits for the port to come free: a server that was
/// just stopped (by this run, or by the previous `kai` run that dropped its
/// handle) takes a moment to exit. Seen: two bench runs back to back.
const PORT_WAIT: Duration = Duration::from_secs(15);

/// Wait until `port` on 127.0.0.1 can be bound, up to `limit`.
pub(crate) async fn wait_for_port(port: u16, limit: Duration) -> anyhow::Result<()> {
    let deadline = Instant::now() + limit;
    loop {
        match std::net::TcpListener::bind(("127.0.0.1", port)) {
            Ok(_) => return Ok(()),
            Err(e) if Instant::now() >= deadline => {
                anyhow::bail!("port {port} on 127.0.0.1 is in use ({e}); pick another port")
            }
            Err(_) => tokio::time::sleep(Duration::from_millis(200)).await,
        }
    }
}

/// Memory and latency settings for one user and one request at a time
/// (measured with Holo3.1 4B Q4 on a 4-vCPU AMX CPU, docs/LOCAL_MODELS.md):
/// - `--no-repack`: repacking keeps a second copy of the weights next to the
///   mapped file (2.6 GB more for a 3 GB model, no speed gain measured);
///   RSS 6.51 → 4.29 GB.
/// - `-np 1`: Kai sends one request at a time; more slots only hold memory.
/// - `--cache-ram 0`: no extra host-RAM prompt cache (up to 8 GB by default);
///   the slot's own cache still reuses the stable prompt prefix.
/// - `--ctx-checkpoints 2`: hybrid models (Qwen3.5, Holo3.1) save ~50 MB of
///   recurrent state per checkpoint, up to 32 by default.
/// - `--reasoning off`: Kai's replies are one short JSON object; thinking
///   took 64+ tokens per reply and ~9 s per step on that CPU, without it ~1 s.
/// - `--reasoning-budget 0`: `off` only asks the template; Gemma 4 E4B still
///   thought on some steps, spent the whole reply budget on it and answered
///   nothing (2026-10-18). A budget of 0 ends any thinking at once.
const LEAN: &[&str] = &[
    "--no-repack",
    "-np",
    "1",
    "--cache-ram",
    "0",
    "--ctx-checkpoints",
    "2",
    "--reasoning",
    "off",
    "--reasoning-budget",
    "0",
];

fn local_http() -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap_or_default()
}

/// Whether `/health` on this port answers 200.
pub async fn health(port: u16) -> bool {
    let url = format!("http://127.0.0.1:{port}/health");
    matches!(local_http().get(url).send().await, Ok(r) if r.status().as_u16() == 200)
}

fn tail(path: &Path) -> String {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let lines: Vec<&str> = text.lines().collect();
    lines[lines.len().saturating_sub(15)..].join("\n")
}

impl LocalManager {
    fn state_path(&self) -> PathBuf {
        self.root().join("server.json")
    }

    pub fn log_path(&self) -> PathBuf {
        self.root().join("logs").join("llama-server.log")
    }

    /// Start `entry` on `port` and wait until it has loaded the model.
    /// Existing servers must be stopped explicitly by their owner.
    pub async fn start(&self, entry: &ModelEntry, port: u16) -> anyhow::Result<LocalServer> {
        anyhow::ensure!(
            self.is_installed(&entry.id),
            "{} is not installed; run `kai models install {}`",
            entry.label,
            entry.id
        );
        let server = self
            .server_path()
            .ok_or_else(|| anyhow::anyhow!("the llama.cpp runtime is not installed"))?;
        if let Some(info) = self.running() {
            anyhow::bail!(
                "a server for {} is already running on port {}; stop it first",
                info.id,
                info.port
            );
        }
        wait_for_port(port, PORT_WAIT).await?;
        let dir = self.model_dir(&entry.id);
        let model = dir.join(
            entry
                .model_file()
                .ok_or_else(|| anyhow::anyhow!("{} has no model file", entry.id))?
                .file_name(),
        );
        let mmproj = entry.mmproj_file().map(|f| dir.join(f.file_name()));
        let log = self.log_path();
        std::fs::create_dir_all(log.parent().unwrap_or(self.root()))?;
        let out = std::fs::File::create(&log)?;
        let mut cmd = Command::new(&server);
        cmd.args(server_args(
            &model,
            mmproj.as_deref(),
            port,
            entry.context,
            &entry.served_name,
        ))
        .current_dir(server.parent().unwrap_or(self.root()))
        .stdin(std::process::Stdio::null())
        .stdout(out.try_clone()?)
        .stderr(out)
        .kill_on_drop(false);
        #[cfg(windows)]
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        let child = cmd
            .spawn()
            .map_err(|e| anyhow::anyhow!("cannot start {}: {e}", server.display()))?;
        let pid = child.id().unwrap_or(0);
        let mut handle = LocalServer {
            base_url: local_base_url(port),
            pid,
            port,
            id: entry.id.clone(),
            child: Some(child),
            state: self.state_path(),
        };
        std::fs::write(
            &handle.state,
            serde_json::to_string_pretty(&handle.info(&entry.served_name))?,
        )?;
        let deadline = Instant::now() + self.health_timeout();
        loop {
            if let Some(c) = handle.child.as_mut()
                && let Some(st) = c.try_wait()?
            {
                handle.child = None;
                handle.remove_owned_state();
                anyhow::bail!(
                    "llama-server stopped while loading ({st}). Last lines of {}:\n{}",
                    log.display(),
                    tail(&log)
                );
            }
            if health(port).await {
                return Ok(handle);
            }
            if Instant::now() > deadline {
                anyhow::bail!(
                    "llama-server did not get ready within {:?}; see {}",
                    self.health_timeout(),
                    log.display()
                );
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }

    /// The server Kai started, if it is still running.
    pub fn running(&self) -> Option<ServerInfo> {
        let info: ServerInfo =
            serde_json::from_str(&std::fs::read_to_string(self.state_path()).ok()?).ok()?;
        if pid_is_server(&info) {
            Some(info)
        } else {
            let _ = std::fs::remove_file(self.state_path());
            None
        }
    }

    /// The running server and whether its model is loaded.
    pub async fn status(&self) -> Option<ServerStatus> {
        let info = self.running()?;
        let healthy = health(info.port).await;
        Some(ServerStatus { info, healthy })
    }

    /// Stop the server Kai started (also one from another process). Returns
    /// whether one was running.
    pub fn stop(&self) -> anyhow::Result<bool> {
        let Some(info) = self.running() else {
            return Ok(false);
        };
        kill_pid(info.pid);
        let _ = std::fs::remove_file(self.state_path());
        Ok(true)
    }
}

/// Guard against a recycled pid: the process must still carry our flags.
#[cfg(target_os = "linux")]
fn pid_is_server(info: &ServerInfo) -> bool {
    let Ok(raw) = std::fs::read(format!("/proc/{}/cmdline", info.pid)) else {
        return false;
    };
    let args: Vec<&[u8]> = raw.split(|b| *b == 0).collect();
    let has = |flag: &str, val: &str| {
        args.windows(2)
            .any(|w| w[0] == flag.as_bytes() && w[1] == val.as_bytes())
    };
    has("--alias", &info.served_name) && has("--port", &info.port.to_string())
}

/// macOS has no `/proc`: read the command line from `ps`.
#[cfg(target_os = "macos")]
fn pid_is_server(info: &ServerInfo) -> bool {
    let Ok(out) = std::process::Command::new("ps")
        .args(["-p", &info.pid.to_string(), "-o", "command="])
        .output()
    else {
        return false;
    };
    let cmd = String::from_utf8_lossy(&out.stdout);
    cmd.contains(&format!("--alias {}", info.served_name))
        && cmd.contains(&format!("--port {}", info.port))
}

#[cfg(unix)]
fn kill_pid(pid: u32) {
    let pid = pid as libc::pid_t;
    // SAFETY: plain signal sends to a pid we verified carries our flags.
    unsafe { libc::kill(pid, libc::SIGTERM) };
    for _ in 0..50 {
        std::thread::sleep(Duration::from_millis(100));
        if unsafe { libc::kill(pid, 0) } != 0 {
            return;
        }
    }
    unsafe { libc::kill(pid, libc::SIGKILL) };
}

#[cfg(windows)]
fn pid_is_server(info: &ServerInfo) -> bool {
    let out = std::process::Command::new("tasklist")
        .args(["/FI", &format!("PID eq {}", info.pid), "/FO", "CSV", "/NH"])
        .creation_flags_no_window()
        .output();
    out.map(|o| String::from_utf8_lossy(&o.stdout).contains("llama-server"))
        .unwrap_or(false)
}

#[cfg(windows)]
fn kill_pid(pid: u32) {
    let _ = std::process::Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .creation_flags_no_window()
        .output();
}

#[cfg(windows)]
trait NoWindow {
    fn creation_flags_no_window(&mut self) -> &mut Self;
}

#[cfg(windows)]
impl NoWindow for std::process::Command {
    fn creation_flags_no_window(&mut self) -> &mut Self {
        use std::os::windows::process::CommandExt;
        self.creation_flags(0x0800_0000)
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
fn pid_is_server(_: &ServerInfo) -> bool {
    false
}

#[cfg(not(any(unix, windows)))]
fn kill_pid(_: u32) {}

#[cfg(test)]
mod tests {

    #[tokio::test]
    async fn start_waits_for_a_port_that_is_being_freed() {
        let held = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = held.local_addr().unwrap().port();
        let release = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(400)).await;
            drop(held);
        });
        wait_for_port(port, Duration::from_secs(5)).await.unwrap();
        release.await.unwrap();
    }

    #[tokio::test]
    async fn a_port_that_stays_busy_still_fails_with_the_reason() {
        let held = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = held.local_addr().unwrap().port();
        let e = wait_for_port(port, Duration::from_millis(300))
            .await
            .unwrap_err();
        assert!(e.to_string().contains("is in use"), "{e}");
    }
    use super::*;

    #[test]
    fn binds_loopback_only_with_the_alias() {
        let a = server_args(
            Path::new("/m/model.gguf"),
            Some(Path::new("/m/mmproj.gguf")),
            8098,
            16384,
            "gemma-4-e4b-it",
        );
        let s: Vec<String> = a.iter().map(|x| x.to_string_lossy().into()).collect();
        let after = |f: &str| s[s.iter().position(|x| x == f).unwrap() + 1].clone();
        assert_eq!(after("--host"), "127.0.0.1");
        assert_eq!(after("--port"), "8098");
        assert_eq!(after("--mmproj"), "/m/mmproj.gguf");
        assert_eq!(after("--alias"), "gemma-4-e4b-it");
        assert_eq!(after("-c"), "16384");
        assert!(s.contains(&"--jinja".to_string()) && s.contains(&"--offline".to_string()));
        assert_eq!(after("-np"), "1");
        assert_eq!(after("--cache-ram"), "0");
        assert_eq!(after("--reasoning"), "off");
        assert_eq!(after("--reasoning-budget"), "0");
        assert!(s.contains(&"--no-repack".to_string()));
        assert!(!s.iter().any(|x| x == "0.0.0.0"));
    }
}
