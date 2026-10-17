//! Provider-owned authentication and catalog discovery. No inference or token reads.
use crate::settings::{BackendChoice, Keys, Settings};
use anyhow::{Context, bail, ensure};
use serde_json::{Value, json};
use std::{path::PathBuf, process::Stdio, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout, Command},
};

use crate::catalog::ModelOrigin;
pub use crate::catalog::RemoteModel;

/// Finder-launched apps do not inherit an interactive shell's PATH.
pub fn provider_binary(name: &str) -> PathBuf {
    let env = format!("KAI_{}_BIN", name.to_ascii_uppercase());
    if let Some(bin) = std::env::var_os(env).filter(|v| !v.is_empty()) {
        return bin.into();
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(contents) = exe.parent().and_then(|p| p.parent())
    {
        let bundled = contents.join("Resources/providers").join(name);
        if bundled.is_file() {
            return bundled;
        }
    }
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        dirs.extend([
            home.join(".local/bin"),
            home.join(".npm-global/bin"),
            home.join(".cargo/bin"),
        ]);
    }
    dirs.extend([
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
        PathBuf::from("/Applications/Codex.app/Contents/Resources"),
    ]);
    let file = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_owned()
    };
    dirs.into_iter()
        .map(|d| d.join(&file))
        .find(|p| p.is_file())
        .unwrap_or_else(|| file.into())
}

/// Why a subscription backend cannot start on this computer, if it cannot.
/// Only looks at the file system: no process is started.
pub fn missing_dependency(backend: BackendChoice) -> Option<String> {
    let (name, product) = match backend {
        BackendChoice::Codex => ("codex", "Codex"),
        BackendChoice::Claude => ("claude", "Claude Code"),
        _ => return None,
    };
    let bin = provider_binary(name);
    (!bin.is_file()).then(|| {
        format!(
            "{product} is not installed on this computer (looked for `{name}` in the app bundle, PATH, Homebrew and ~/.local/bin). Install it with its setup guide, then refresh."
        )
    })
}

struct Rpc {
    _child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    next: u64,
}
impl Rpc {
    async fn start() -> anyhow::Result<Self> {
        let mut child = Command::new(provider_binary("codex"))
            .args(["app-server", "--listen", "stdio://"])
            .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null())
            .kill_on_drop(true).spawn()
            .context("Codex is not installed or could not start. Install Codex, then reconnect in Settings.")?;
        let input = child.stdin.take().context("Codex stdin unavailable")?;
        let output = BufReader::new(child.stdout.take().context("Codex stdout unavailable")?);
        let mut rpc = Self {
            _child: child,
            input,
            output,
            next: 0,
        };
        rpc.call(
            "initialize",
            json!({"clientInfo":{"name":"kai","title":"Kai","version":env!("CARGO_PKG_VERSION")}}),
        )
        .await?;
        rpc.send(json!({"method":"initialized","params":{}}))
            .await?;
        Ok(rpc)
    }
    async fn send(&mut self, value: Value) -> anyhow::Result<()> {
        self.input
            .write_all(format!("{value}\n").as_bytes())
            .await?;
        self.input.flush().await?;
        Ok(())
    }
    async fn read(&mut self) -> anyhow::Result<Value> {
        // Bounded lines: reject oversized messages rather than accumulating indefinitely.
        let mut bytes = Vec::new();
        loop {
            let buf = self.output.fill_buf().await?;
            ensure!(!buf.is_empty(), "Codex disconnected; reconnect in Settings");
            let n = buf
                .iter()
                .position(|b| *b == b'\n')
                .map(|i| i + 1)
                .unwrap_or(buf.len());
            ensure!(
                bytes.len() + n <= 8 * 1024 * 1024,
                "Codex message exceeded 8 MiB"
            );
            let done = buf[n - 1] == b'\n';
            bytes.extend_from_slice(&buf[..n]);
            self.output.consume(n);
            if done {
                return Ok(serde_json::from_slice(&bytes)?);
            }
        }
    }
    async fn call(&mut self, method: &str, params: Value) -> anyhow::Result<Value> {
        self.next += 1;
        let id = self.next;
        self.send(json!({"id":id,"method":method,"params":params}))
            .await?;
        tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                let value = self.read().await?;
                if value.get("id").and_then(Value::as_u64) == Some(id) {
                    ensure!(
                        value.get("error").is_none(),
                        "Codex rejected {method}; update Codex or reconnect your account"
                    );
                    return Ok(value.get("result").cloned().unwrap_or(Value::Null));
                }
            }
        })
        .await
        .context("Codex did not respond within 30 seconds")?
    }
}

pub enum AccountAction {
    Refresh,
    Connect,
    Disconnect,
}

/// `open` receives only the authorization URL; tokens remain inside Codex.
pub async fn codex_account(
    action: AccountAction,
    open: impl Fn(String),
) -> anyhow::Result<(String, Vec<RemoteModel>)> {
    let mut rpc = Rpc::start().await?;
    match action {
        AccountAction::Connect => {
            let result = rpc
                .call("account/login/start", json!({"type":"chatgpt"}))
                .await?;
            let url = result
                .get("authUrl")
                .and_then(Value::as_str)
                .context("Codex did not return a sign-in URL")?;
            let parsed = url::Url::parse(url)?;
            ensure!(
                parsed.scheme() == "https"
                    && matches!(
                        parsed.host_str(),
                        Some("auth.openai.com" | "auth0.openai.com" | "chatgpt.com")
                    ),
                "Unexpected Codex authorization address"
            );
            open(url.to_owned());
            tokio::time::timeout(Duration::from_secs(300), async {
                loop {
                    let event = rpc.read().await?;
                    if event["method"] == "account/login/completed" {
                        ensure!(
                            event["params"]["success"] == true,
                            "Sign-in was canceled or failed; reconnect to try again"
                        );
                        return Ok::<_, anyhow::Error>(());
                    }
                }
            })
            .await
            .context("Sign-in expired after five minutes; reconnect to try again")??;
        }
        AccountAction::Disconnect => {
            rpc.call("account/logout", json!({})).await?;
            return Ok(("Signed out of Codex on this computer".into(), vec![]));
        }
        AccountAction::Refresh => {}
    }
    let account = rpc
        .call("account/read", json!({"refreshToken":false}))
        .await?;
    ensure!(
        account["account"].is_object(),
        "Not signed in. Press Connect ChatGPT / Codex."
    );
    ensure!(
        account["account"]["type"] == "chatgpt",
        "Codex is using API billing. Connect ChatGPT to use a subscription."
    );
    let plan = account["account"]["planType"].as_str().unwrap_or("ChatGPT");
    let label = format!("Connected through Codex · {plan} · model access not tested");
    let mut models = Vec::new();
    let mut cursor = Value::Null;
    for _ in 0..100 {
        let result = rpc
            .call("model/list", json!({"cursor":cursor,"limit":100}))
            .await?;
        for m in result["data"].as_array().into_iter().flatten() {
            if let Some(model) = codex_model(m)
                && !models
                    .iter()
                    .any(|known: &RemoteModel| known.id == model.id)
            {
                models.push(model);
            }
        }
        let next = result.get("nextCursor").cloned().unwrap_or(Value::Null);
        if next.is_null() {
            return Ok((label, models));
        }
        ensure!(next != cursor, "Codex repeated its catalog cursor");
        cursor = next;
    }
    bail!("Codex catalog exceeded 100 pages")
}

/// One `model/list` entry: the exact slug Codex accepts as `--model`.
/// Hidden models are left out, as Codex's own picker does.
pub fn codex_model(m: &Value) -> Option<RemoteModel> {
    if m["hidden"] == true {
        return None;
    }
    let id = m["model"].as_str().or_else(|| m["id"].as_str())?;
    let image_input = m["inputModalities"]
        .as_array()
        .map(|all| all.iter().any(|x| x == "image"));
    Some(RemoteModel {
        id: id.into(),
        label: m["displayName"].as_str().unwrap_or(id).into(),
        origin: ModelOrigin::Discovered,
        image_input,
    })
}

pub async fn discover(settings: &Settings, keys: &Keys) -> anyhow::Result<Vec<RemoteModel>> {
    let (base, key) = match settings.backend {
        BackendChoice::Vllm => (
            settings.vllm_endpoint.clone(),
            compatible_key(settings, keys)?,
        ),
        BackendChoice::OpenAi => (crate::client::OPENAI_BASE.into(), keys.openai.as_deref()),
        BackendChoice::Gateway => (crate::client::GATEWAY_BASE.into(), keys.gateway.as_deref()),
        BackendChoice::Local => (format!("http://127.0.0.1:{}/v1", settings.local_port), None),
        _ => bail!(
            "Select Codex, OpenAI, AI Gateway, a compatible server, or a local server to discover models"
        ),
    };
    let parsed = url::Url::parse(&base).context("Enter a server URL ending in /v1")?;
    ensure!(
        parsed.username().is_empty() && parsed.password().is_none(),
        "Keep credentials in the key field, not the URL"
    );
    let loopback = matches!(
        parsed.host_str(),
        Some("localhost" | "127.0.0.1" | "[::1]" | "::1")
    );
    ensure!(
        parsed.scheme() == "https" || (parsed.scheme() == "http" && loopback),
        "Cloud endpoints require HTTPS; HTTP is allowed for loopback only"
    );
    let mut req = crate::client::http_client()
        .get(format!("{}/models", base.trim_end_matches('/')))
        .timeout(Duration::from_secs(30));
    if let Some(key) = key.filter(|k| !k.is_empty()) {
        req = req.bearer_auth(key);
    }
    let response = req
        .send()
        .await
        .context("Could not reach the model catalog")?;
    ensure!(
        response.status().is_success(),
        "Catalog returned HTTP {}; check the endpoint and account",
        response.status().as_u16()
    );
    let value: Value = response
        .json()
        .await
        .context("The server did not return a JSON model catalog")?;
    let data = value["data"]
        .as_array()
        .context("The model catalog has no data array")?;
    let mut models: Vec<_> = data
        .iter()
        .filter_map(|m| {
            let id = m["id"].as_str()?;
            Some(RemoteModel {
                id: id.into(),
                label: m["name"].as_str().unwrap_or(id).into(),
                origin: ModelOrigin::Discovered,
                image_input: None,
            })
        })
        .collect();
    models.sort_by(|a, b| a.id.cmp(&b.id));
    models.dedup_by(|a, b| a.id == b.id);
    Ok(models)
}

/// Claude Code owns its browser flow and credential store as well.
pub async fn claude_account(action: AccountAction) -> anyhow::Result<(String, Vec<RemoteModel>)> {
    let bin = provider_binary("claude");
    match action {
        AccountAction::Connect | AccountAction::Disconnect => {
            let verb = if matches!(action, AccountAction::Connect) {
                "login"
            } else {
                "logout"
            };
            let mut child = Command::new(&bin).args(["auth",verb]).stdin(Stdio::null())
                .stdout(Stdio::null()).stderr(Stdio::null()).kill_on_drop(true).spawn()
                .context("Claude Code is not installed. Open its setup guide, install it, then reconnect.")?;
            let status = tokio::time::timeout(Duration::from_secs(300), child.wait())
                .await
                .context("Claude sign-in expired; reconnect to try again")??;
            ensure!(
                status.success(),
                "Claude Code {verb} did not complete; retry or use the provider's sign-in application"
            );
            if verb == "logout" {
                return Ok(("Signed out of Claude Code on this computer".into(), vec![]));
            }
        }
        AccountAction::Refresh => {}
    }
    let output = tokio::time::timeout(
        Duration::from_secs(30),
        Command::new(&bin)
            .args(["auth", "status"])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .output(),
    )
    .await
    .context("Claude Code account status timed out")?
    .context("Could not start Claude Code; install it first")?;
    ensure!(
        output.status.success(),
        "Claude Code is not signed in. Connect Claude Code first."
    );
    let status: Value = serde_json::from_slice(&output.stdout)
        .context("Update Claude Code to a version supporting JSON account status")?;
    ensure!(status["loggedIn"] == true, "Claude Code is not signed in");
    Ok((
        "Claude Code connected · Claude Code has no model catalog, so these are its documented aliases; account access and vision not tested".into(),
        crate::catalog::documented(BackendChoice::Claude),
    ))
}

/// Never reuse a stored server key after a GUI endpoint-origin change.
/// Explicit environment keys retain CLI compatibility and are operator-owned.
pub fn compatible_key<'a>(settings: &Settings, keys: &'a Keys) -> anyhow::Result<Option<&'a str>> {
    let endpoint =
        url::Url::parse(&settings.vllm_endpoint).context("Enter a compatible server URL first")?;
    let origin = endpoint.origin().ascii_serialization();
    let loopback = matches!(
        endpoint.host_str(),
        Some("localhost" | "127.0.0.1" | "[::1]" | "::1")
    );
    let key = keys.vllm.as_deref().filter(|k| !k.is_empty());
    let explicit =
        key.is_some_and(|key| std::env::var("VLLM_API_KEY").ok().as_deref() == Some(key));
    if settings.compatible_key_origin == origin || explicit {
        return Ok(key);
    }
    if loopback {
        return Ok(None);
    }
    bail!(
        "This server origin has no authorized key. Enter its API key and save it before connecting. An existing provider's key will not be reused."
    )
}

pub fn endpoint_origin(endpoint: &str) -> Option<String> {
    url::Url::parse(endpoint)
        .ok()
        .map(|u| u.origin().ascii_serialization())
}
