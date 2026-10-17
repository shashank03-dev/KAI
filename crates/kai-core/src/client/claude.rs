//! Claude Code owns sign-in. Tool-disabled calls return decisions to Kai.
use super::{Backend, ClientError, ModelClient, ModelReply, ModelRequest};
use async_trait::async_trait;
use base64::Engine;
use serde_json::{Value, json};
use std::{
    process::Stdio,
    time::{Duration, Instant},
};
use tokio::{io::AsyncWriteExt, process::Command};

pub struct ClaudeClient {
    model: String,
}
impl ClaudeClient {
    pub fn new(model: &str) -> Self {
        Self {
            model: model.into(),
        }
    }
}

#[async_trait]
impl ModelClient for ClaudeClient {
    fn backend(&self) -> Backend {
        Backend::Claude
    }
    async fn decide(&self, req: &ModelRequest<'_>) -> Result<ModelReply, ClientError> {
        let started = Instant::now();
        let timeout = Duration::from_secs(180);
        let dir =
            super::codex::Scratch::new().map_err(|e| ClientError::Transport(e.to_string()))?;
        let mut content = vec![
            json!({"type":"text","text":format!("{}\n\nReturn only one decision using the supplied schema. Do not execute any actions yourself.",req.user_text)}),
        ];
        for image in std::iter::once(req.images.screenshot_jpeg.as_slice())
            .chain(req.images.crop_jpeg.as_deref())
            .filter(|image| !image.is_empty())
        {
            content.push(json!({"type":"image","source":{"type":"base64","media_type":"image/jpeg","data":base64::engine::general_purpose::STANDARD.encode(image)}}));
        }
        let message = json!({"type":"user","message":{"role":"user","content":content}});
        let mut child = Command::new(crate::connections::provider_binary("claude"))
            .args([
                "-p",
                "--restricted",
                "--no-chrome",
                "--permission-prompts",
                "none",
                "--input-format",
                "stream-json",
                "--output-format",
                "json",
                "--model",
                &self.model,
                "--tools",
                "",
                "--strict-mcp-config",
                "--mcp-config",
                "{\"mcpServers\":{}}",
                "--setting-sources",
                "",
                "--settings",
                "{\"disableAllHooks\":true}",
                "--no-session-persistence",
                "--system-prompt",
                req.system,
                "--json-schema",
                &req.schema.to_string(),
            ])
            .current_dir(&dir.0)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| ClientError::Transport(format!("Cannot start Claude Code: {e}")))?;
        let mut stdin = child.stdin.take().ok_or(ClientError::EmptyBody)?;
        stdin
            .write_all(format!("{message}\n").as_bytes())
            .await
            .map_err(|e| ClientError::Transport(e.to_string()))?;
        drop(stdin);
        let out = tokio::time::timeout(timeout, child.wait_with_output())
            .await
            .map_err(|_| ClientError::Timeout(timeout))?
            .map_err(|e| ClientError::Transport(e.to_string()))?;
        if !out.status.success() {
            return Err(ClientError::Unavailable(
                "Claude Code could not answer; check sign-in, model access and usage in Settings"
                    .into(),
            ));
        }
        let value: Value =
            serde_json::from_slice(&out.stdout).map_err(|_| ClientError::EmptyBody)?;
        if value["is_error"] == true {
            return Err(ClientError::Unavailable(
                "Claude Code reported a model or account error".into(),
            ));
        }
        let raw = value
            .get("structured_output")
            .filter(|v| v.is_object())
            .map(Value::to_string)
            .or_else(|| value["result"].as_str().map(str::to_owned))
            .ok_or(ClientError::EmptyBody)?;
        Ok(ModelReply {
            raw,
            prompt_tokens: value["usage"]["input_tokens"]
                .as_u64()
                .and_then(|v| v.try_into().ok()),
            completion_tokens: value["usage"]["output_tokens"]
                .as_u64()
                .and_then(|v| v.try_into().ok()),
            first_token: None,
            total: started.elapsed(),
            backend: Backend::Claude,
            cost_usd: value["total_cost_usd"].as_f64(),
        })
    }
}
