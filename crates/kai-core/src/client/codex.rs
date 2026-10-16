//! A model reached through the Codex CLI (`codex exec`) and the user's own
//! ChatGPT sign-in. Kai never reads the sign-in: it runs the CLI, which owns
//! it, in an empty folder with a read-only sandbox, passes the screenshot as
//! an image and the reply schema as `--output-schema`, and reads only the
//! final message. Each call carries Codex's own instructions (about 15k
//! tokens, measured 2026-10-16), so it suits comparisons, not daily runs.
use super::{Backend, ClientError, ModelClient, ModelReply, ModelRequest};
use async_trait::async_trait;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

/// The binary (`KAI_CODEX_BIN`, default `codex` on PATH) and its reasoning
/// effort (`KAI_CODEX_EFFORT`, default `low`: Kai asks for one step).
const BIN_ENV: &str = "KAI_CODEX_BIN";
const EFFORT_ENV: &str = "KAI_CODEX_EFFORT";
const TIMEOUT: Duration = Duration::from_secs(180);

pub struct CodexClient {
    model: String,
    bin: String,
    effort: String,
    timeout: Duration,
}

impl CodexClient {
    pub fn new(model: &str) -> Self {
        let env = |k: &str, d: &str| {
            std::env::var(k)
                .ok()
                .filter(|v| !v.trim().is_empty())
                .unwrap_or_else(|| d.to_string())
        };
        Self {
            model: model.to_string(),
            bin: env(BIN_ENV, "codex"),
            effort: env(EFFORT_ENV, "low"),
            timeout: TIMEOUT,
        }
    }

    /// The `codex exec` arguments for one call in `dir`.
    fn args(&self, dir: &Path, image: bool) -> Vec<String> {
        let mut a: Vec<String> = [
            "exec",
            "--ephemeral",
            "--skip-git-repo-check",
            "--color",
            "never",
            "-s",
            "read-only",
            "-m",
            &self.model,
            "-c",
            &format!("model_reasoning_effort=\"{}\"", self.effort),
            "--output-schema",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        a.push(dir.join("schema.json").display().to_string());
        a.push("-o".into());
        a.push(dir.join("reply.json").display().to_string());
        if image {
            a.push("-i".into());
            a.push(dir.join("screen.jpg").display().to_string());
        }
        a.push("-".into());
        a
    }
}

/// A fresh, empty working folder for one call, removed when dropped.
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> std::io::Result<Self> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        for _ in 0..16 {
            let dir = std::env::temp_dir().join(format!(
                "kai-codex-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::SeqCst)
            ));
            #[cfg(unix)]
            let created = {
                use std::os::unix::fs::DirBuilderExt;
                std::fs::DirBuilder::new().mode(0o700).create(&dir)
            };
            #[cfg(not(unix))]
            let created = std::fs::create_dir(&dir);
            match created {
                Ok(()) => return Ok(Self(dir)),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e),
            }
        }
        Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "could not create a unique Codex scratch folder",
        ))
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// "tokens used\n15,154" at the end of Codex's log.
fn tokens_used(log: &str) -> Option<u32> {
    let mut lines = log.lines();
    lines.find(|l| l.trim() == "tokens used")?;
    lines.next()?.trim().replace(',', "").parse().ok()
}

/// A usage limit is not a reason to retry or fall back: say so plainly.
fn failure(log: &str) -> ClientError {
    let lower = log.to_ascii_lowercase();
    let last = log
        .lines()
        .rev()
        .find(|l| l.contains("ERROR") || l.contains("error"))
        .unwrap_or("codex exec failed")
        .trim()
        .chars()
        .take(300)
        .collect::<String>();
    if lower.contains("usage limit") || lower.contains("rate limit") {
        ClientError::Unavailable(format!("Codex usage limit reached: {last}"))
    } else if lower.contains("not supported when using codex") {
        ClientError::Unavailable(last)
    } else {
        ClientError::Transport(last)
    }
}

#[async_trait]
impl ModelClient for CodexClient {
    fn backend(&self) -> Backend {
        Backend::Codex
    }

    async fn decide(&self, req: &ModelRequest<'_>) -> Result<ModelReply, ClientError> {
        let started = Instant::now();
        let io = |e: std::io::Error| ClientError::Transport(e.to_string());
        let dir = Scratch::new().map_err(io)?;
        let schema = super::openai::strictify(req.schema);
        std::fs::write(dir.0.join("schema.json"), schema.to_string()).map_err(io)?;
        let image = !req.images.screenshot_jpeg.is_empty();
        if image {
            std::fs::write(dir.0.join("screen.jpg"), &req.images.screenshot_jpeg).map_err(io)?;
        }
        let prompt = format!(
            "{}\n\n{}\n\nDo not run commands or read files: answer only from the screenshot and \
             this message, with the JSON object.",
            req.system, req.user_text
        );
        let mut child = Command::new(&self.bin)
            .args(self.args(&dir.0, image))
            .current_dir(&dir.0)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| ClientError::Transport(format!("cannot run {}: {e}", self.bin)))?;
        let mut stdin = child.stdin.take().expect("stdin is piped");
        stdin.write_all(prompt.as_bytes()).await.map_err(io)?;
        drop(stdin);
        let out = tokio::time::timeout(self.timeout, child.wait_with_output())
            .await
            .map_err(|_| ClientError::Timeout(self.timeout))?
            .map_err(io)?;
        let log = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        let raw = std::fs::read_to_string(dir.0.join("reply.json")).unwrap_or_default();
        if !out.status.success() || raw.trim().is_empty() {
            return Err(failure(&log));
        }
        Ok(ModelReply {
            raw,
            prompt_tokens: tokens_used(&log),
            completion_tokens: None,
            first_token: None,
            total: started.elapsed(),
            backend: Backend::Codex,
            cost_usd: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runs_read_only_ephemeral_and_reads_the_last_message() {
        let c = CodexClient::new("gpt-6-luna");
        let a = c.args(Path::new("/w"), true).join(" ");
        assert!(a.starts_with("exec --ephemeral --skip-git-repo-check"));
        assert!(a.contains("-s read-only"));
        assert!(a.contains("-m gpt-6-luna"));
        assert!(a.contains("--output-schema /w/schema.json"));
        assert!(a.contains("-o /w/reply.json"));
        assert!(a.contains("-i /w/screen.jpg"));
        assert!(a.ends_with(" -"));
        assert!(!c.args(Path::new("/w"), false).join(" ").contains("-i "));
        // Never the flags that would let the model act on this computer.
        assert!(!a.contains("danger") && !a.contains("workspace-write"));
    }

    #[test]
    fn reads_tokens_and_names_limits() {
        assert_eq!(tokens_used("x\ntokens used\n15,154\n"), Some(15154));
        assert!(matches!(
            failure("ERROR: You've hit your usage limit."),
            ClientError::Unavailable(m) if m.contains("usage limit")
        ));
        assert!(matches!(failure("ERROR: boom"), ClientError::Transport(_)));
    }

    #[cfg(unix)]
    #[test]
    fn scratch_screenshots_are_private() {
        use std::os::unix::fs::PermissionsExt;
        let dir = Scratch::new().unwrap();
        let mode = std::fs::metadata(&dir.0).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700);
    }
}
