//! The real OS probes for the outcome ledger (`docs/VERIFICATION.md`):
//! `file_exists`, `file_contains` and `shell_ro`. Nothing here writes.
//! `shell_ro` only accepts a [`ReadOnlyCmd`], which only the allowlist in
//! `kai_core::shell_ro` can build; POSIX stages run directly (no shell),
//! piped stage to stage with stdin closed and output discarded, and every
//! child is killed when the engine's timeout drops the future.

use async_trait::async_trait;
use kai_core::probe::{ProbeSpec, ProbeVerdict};
use kai_core::shell_ro::{Flavor, ReadOnlyCmd};
use std::path::PathBuf;
use std::process::Stdio;
use tokio::io::AsyncReadExt;
use tokio::process::{Child, Command};

/// Most bytes `file_contains` reads.
pub const MAX_READ: u64 = 4 * 1024 * 1024;

/// Read-only checks on this machine.
#[derive(Debug, Default, Clone, Copy)]
pub struct DesktopProbe;

fn home() -> PathBuf {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .map(Into::into)
        .unwrap_or_else(std::env::temp_dir)
}

/// `~` and `~/…` become the home folder; nothing else is expanded.
fn expand(word: &str) -> String {
    if word == "~" {
        return home().display().to_string();
    }
    match word.strip_prefix("~/").or_else(|| word.strip_prefix("~\\")) {
        Some(rest) => home().join(rest).display().to_string(),
        None => word.to_string(),
    }
}

#[async_trait]
impl kai_core::Probe for DesktopProbe {
    async fn check(&self, probe: &ProbeSpec) -> anyhow::Result<ProbeVerdict> {
        Ok(match probe {
            ProbeSpec::FileExists { path } => match tokio::fs::metadata(expand(path)).await {
                Ok(_) => ProbeVerdict::pass("exists"),
                Err(_) => ProbeVerdict::fail("not found"),
            },
            ProbeSpec::FileContains { path, text } => file_contains(&expand(path), text).await,
            ProbeSpec::ShellRo { cmd } => run_read_only(cmd).await,
            _ => anyhow::bail!("not an OS probe"),
        })
    }
}

async fn file_contains(path: &str, text: &str) -> ProbeVerdict {
    let Ok(f) = tokio::fs::File::open(path).await else {
        return ProbeVerdict::fail("not found");
    };
    let mut buf = Vec::new();
    if f.take(MAX_READ).read_to_end(&mut buf).await.is_err() {
        return ProbeVerdict::fail("not readable");
    }
    let hay = String::from_utf8_lossy(&buf).to_lowercase();
    if hay.contains(&text.to_lowercase()) {
        ProbeVerdict::pass("text found")
    } else {
        ProbeVerdict::fail("text not in the file")
    }
}

/// Run a checked command. Only the exit status is kept, never the output.
pub async fn run_read_only(cmd: &ReadOnlyCmd) -> ProbeVerdict {
    let status = match cmd.flavor() {
        Flavor::Posix => posix(cmd.stages()).await,
        Flavor::PowerShell => powershell(&cmd.powershell_line()).await,
    };
    match status {
        Ok(0) => ProbeVerdict::pass("exit 0"),
        Ok(code) => ProbeVerdict::fail(format!("exit {code}")),
        Err(e) => ProbeVerdict::fail(e),
    }
}

fn quiet(c: &mut Command) {
    c.stderr(Stdio::null())
        .kill_on_drop(true)
        .current_dir(home());
    #[cfg(target_os = "windows")]
    c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
}

/// Stage to stage through pipes; the last stage's exit code decides.
async fn posix(stages: &[Vec<String>]) -> Result<i32, String> {
    let mut children: Vec<Child> = Vec::new();
    let mut input: Option<Stdio> = None;
    for (i, words) in stages.iter().enumerate() {
        let last = i + 1 == stages.len();
        let mut c = Command::new(&words[0]);
        c.args(words[1..].iter().map(|w| expand(w)));
        c.stdin(input.take().unwrap_or_else(Stdio::null));
        c.stdout(if last { Stdio::null() } else { Stdio::piped() });
        quiet(&mut c);
        let mut child = c
            .spawn()
            .map_err(|_| format!("could not start {}", words[0]))?;
        if !last {
            let out = child.stdout.take().ok_or("no pipe")?;
            input = Some(out.try_into().map_err(|_| "no pipe".to_string())?);
        }
        children.push(child);
    }
    let mut code = -1;
    for (i, mut child) in children.into_iter().enumerate().rev() {
        let status = child.wait().await.map_err(|_| "wait failed".to_string())?;
        if i + 1 == stages.len() {
            code = status.code().unwrap_or(-1);
        }
    }
    Ok(code)
}

async fn powershell(line: &str) -> Result<i32, String> {
    let exe = if cfg!(windows) { "powershell" } else { "pwsh" };
    let mut c = Command::new(exe);
    // A Get-* that writes nothing still "succeeds"; `$?`-style exit codes
    // are what PowerShell gives: 0 when the last command succeeded.
    c.args(["-NoProfile", "-NonInteractive", "-Command", line])
        .stdin(Stdio::null())
        .stdout(Stdio::null());
    quiet(&mut c);
    let status = c
        .status()
        .await
        .map_err(|_| format!("could not start {exe}"))?;
    Ok(status.code().unwrap_or(-1))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use kai_core::Probe;
    use kai_core::probe::RawProbe;
    use std::time::Duration;

    fn spec(json: serde_json::Value) -> ProbeSpec {
        let raw: RawProbe = serde_json::from_value(json).unwrap();
        ProbeSpec::parse(&raw).unwrap()
    }

    #[tokio::test]
    async fn file_probes_read_and_never_write() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("note.txt");
        std::fs::write(&f, "Hello from Kai\n").unwrap();
        let path = f.display().to_string();
        let p = DesktopProbe;
        let v = |j| async { p.check(&spec(j)).await.unwrap() };
        assert!(
            v(serde_json::json!({"kind":"file_exists","path":path}))
                .await
                .ok
        );
        assert!(
            v(serde_json::json!({"kind":"file_contains","path":path,"text":"hello FROM"}))
                .await
                .ok
        );
        let miss = v(serde_json::json!({"kind":"file_contains","path":path,"text":"bye"})).await;
        assert_eq!(miss.reason, "text not in the file");
        let gone = dir.path().join("gone.txt").display().to_string();
        assert_eq!(
            v(serde_json::json!({"kind":"file_exists","path":gone}))
                .await
                .reason,
            "not found"
        );
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "Hello from Kai\n");
    }

    #[tokio::test]
    async fn shell_ro_runs_without_a_shell_and_keeps_only_the_exit_code() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("sub.json");
        std::fs::write(&f, "{\"topic\": \"billing\"}").unwrap();
        let path = f.display().to_string();
        let run = |c: String| async move { run_read_only(&ReadOnlyCmd::parse(&c).unwrap()).await };
        assert!(run(format!("grep -q billing {path}")).await.ok);
        assert_eq!(run(format!("grep -q refund {path}")).await.reason, "exit 1");
        assert!(run(format!("cat {path} | grep -q topic")).await.ok);
        assert!(run(format!("test -f {path}")).await.ok);
        // `*` is not a glob and `;` never gets this far: no shell runs.
        assert!(!run(format!("ls {}/*.json", dir.path().display())).await.ok);
        assert!(ReadOnlyCmd::parse(&format!("cat {path}; rm {path}")).is_err());
        assert!(f.exists());
    }

    #[tokio::test]
    async fn a_dropped_probe_kills_its_children() {
        // `cat /dev/zero | wc -c` never ends by itself.
        let cmd = ReadOnlyCmd::parse("cat /dev/zero | wc -c").unwrap();
        let t = std::time::Instant::now();
        let r = tokio::time::timeout(Duration::from_millis(300), run_read_only(&cmd)).await;
        assert!(r.is_err(), "timed out");
        assert!(t.elapsed() < Duration::from_secs(2));
    }
}
