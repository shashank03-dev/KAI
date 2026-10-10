//! Any OpenAI-compatible `/v1/chat/completions` endpoint: the Vercel AI
//! Gateway (Gemma 4 and comparison models) or OpenAI itself.
//!
//! - Images go as `image_url` data URIs (screenshot, then the optional crop).
//! - Structured output via `response_format: json_schema` (strict first, then
//!   non-strict, then none plus a schema hint), stepping down once on a 400 that
//!   names the schema and remembering the step for the rest of the process.
//! - GPT reasoning models (`gpt-5*`, `o1/o3/o4*`) get `max_completion_tokens`
//!   and a low `reasoning_effort` (`minimal`, else `low`), and no temperature.
//! - Streaming by default (`stream_options.include_usage`), for real TTFT, with
//!   an optional early stop once the decision object closes.

use super::stream::{self, StreamOutcome};
use super::{Backend, ClientError, ModelClient, ModelReply, ModelRequest, http_client, send_error};
use async_trait::async_trait;
use base64::Engine;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::{Duration, Instant};

pub const GATEWAY_BASE: &str = "https://ai-gateway.vercel.sh/v1";
pub const OPENAI_BASE: &str = "https://api.openai.com/v1";

const SCHEMA_STRICT: u8 = 0;
// 1 = non-strict `json_schema` (between the two).
const SCHEMA_OFF: u8 = 2;

const EFFORT_MINIMAL: u8 = 0;
const EFFORT_LOW: u8 = 1;
const EFFORT_OMIT: u8 = 2;

/// Map a Kai model id to an AI Gateway id: ids with a `/` pass through,
/// `gemma-*`/`gemini-*` get `google/`, GPT and o-series ids get `openai/`.
pub fn gateway_model_id(id: &str) -> String {
    if id.contains('/') {
        return id.to_string();
    }
    if id.starts_with("gemma") || id.starts_with("gemini") {
        format!("google/{id}")
    } else if id.starts_with("gpt-") || is_o_series(id) || id.starts_with("computer-use") {
        format!("openai/{id}")
    } else {
        id.to_string()
    }
}

fn is_o_series(m: &str) -> bool {
    ["o1", "o3", "o4"].iter().any(|p| m.starts_with(p))
}

/// OpenAI reasoning models: no temperature, `max_completion_tokens`.
fn is_reasoning(model: &str) -> bool {
    let m = model.rsplit('/').next().unwrap_or(model);
    m.starts_with("gpt-5") || is_o_series(m)
}

pub struct OpenAiClient {
    http: reqwest::Client,
    base: String,
    api_key: String,
    model: String,
    timeout: Duration,
    stream: bool,
    early_stop: bool,
    /// `None` = automatic: put the schema in the prompt unless the model is
    /// one that enforces `json_schema` (OpenAI's own). The Gateway accepts
    /// `response_format` for Gemma but does not enforce it (measured).
    schema_in_prompt: Option<bool>,
    schema_mode: AtomicU8,
    effort: AtomicU8,
    /// A server on this computer: no key needed, reported as [`Backend::Local`].
    local: bool,
}

impl OpenAiClient {
    /// Vercel AI Gateway; model ids like `google/gemma-4-26b-a4b-it`.
    pub fn gateway(api_key: impl Into<String>) -> Self {
        Self::new(GATEWAY_BASE, api_key).with_model("google/gemma-4-26b-a4b-it")
    }

    /// OpenAI directly; model ids like `gpt-5-mini`.
    pub fn openai(api_key: impl Into<String>) -> Self {
        Self::new(OPENAI_BASE, api_key).with_model("gpt-5-mini")
    }

    /// A local OpenAI-compatible server (`llama-server`), e.g.
    /// `http://127.0.0.1:8098/v1`. No key; a long timeout, since a CPU can
    /// take a minute to read a screenshot.
    pub fn local(base: impl Into<String>, model: impl Into<String>) -> Self {
        let mut c = Self::new("", "")
            .with_base(base)
            .with_model(model)
            .with_timeout(Duration::from_secs(300));
        c.local = true;
        c
    }

    fn new(base: &str, api_key: impl Into<String>) -> Self {
        Self {
            http: http_client(),
            base: base.to_string(),
            api_key: api_key.into(),
            model: String::new(),
            timeout: Duration::from_secs(45),
            stream: true,
            early_stop: false,
            schema_in_prompt: None,
            schema_mode: AtomicU8::new(SCHEMA_STRICT),
            effort: AtomicU8::new(EFFORT_MINIMAL),
            local: false,
        }
    }

    pub fn with_base(mut self, base: impl Into<String>) -> Self {
        self.base = base.into().trim_end_matches('/').to_string();
        self
    }

    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        let m = self.model.rsplit('/').next().unwrap_or(&self.model);
        // gpt-5 / gpt-5-mini / gpt-5-nano take `minimal`; later ones and the
        // o-series start at `low` (o4-mini rejects `minimal`, measured).
        let start =
            if matches!(m, "gpt-5" | "gpt-5-mini" | "gpt-5-nano") || m.starts_with("gpt-5-20") {
                EFFORT_MINIMAL
            } else {
                EFFORT_LOW
            };
        self.effort = AtomicU8::new(start);
        self
    }

    pub fn with_timeout(mut self, t: Duration) -> Self {
        self.timeout = t;
        self
    }

    /// Streaming on (default) or off.
    pub fn with_streaming(mut self, on: bool) -> Self {
        self.stream = on;
        self
    }

    /// Stop reading the stream once the first JSON object closes.
    pub fn with_early_stop(mut self, on: bool) -> Self {
        self.early_stop = on;
        self
    }

    /// Force the schema hint in the prompt on or off.
    pub fn with_schema_in_prompt(mut self, on: bool) -> Self {
        self.schema_in_prompt = Some(on);
        self
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    fn enforces_schema(&self) -> bool {
        self.base == OPENAI_BASE || self.model.starts_with("openai/")
    }

    pub(crate) fn body(&self, req: &ModelRequest<'_>, mode: u8, effort: u8) -> serde_json::Value {
        let mut content = vec![serde_json::json!({ "type": "text", "text": req.user_text })];
        if !req.images.screenshot_jpeg.is_empty() {
            content.push(image_part(&req.images.screenshot_jpeg));
        }
        if let Some(crop) = &req.images.crop_jpeg {
            content.push(image_part(crop));
        }
        let has_schema = !req.schema.is_null();
        let hint = has_schema
            && (mode == SCHEMA_OFF || self.schema_in_prompt.unwrap_or(!self.enforces_schema()));
        // The hint goes at the end of the system prompt: stable across steps,
        // so prefix caching still hits.
        let system = if hint {
            format!(
                "{}\n\nReply with ONLY a JSON object matching this JSON schema (no prose, no \
                 markdown fences):\n{}",
                req.system,
                serde_json::to_string(req.schema).unwrap_or_default()
            )
        } else {
            req.system.to_string()
        };
        let mut body = serde_json::json!({
            "model": self.model,
            "messages": [
                { "role": "system", "content": system },
                { "role": "user", "content": content }
            ],
        });
        if is_reasoning(&self.model) {
            // Reasoning tokens count against the limit; leave room for them.
            let room = if effort == EFFORT_MINIMAL { 256 } else { 2048 };
            body["max_completion_tokens"] = (req.max_tokens + room).into();
            if effort != EFFORT_OMIT {
                let e = if effort == EFFORT_MINIMAL {
                    "minimal"
                } else {
                    "low"
                };
                body["reasoning_effort"] = e.into();
            }
        } else {
            body["max_tokens"] = req.max_tokens.into();
            body["temperature"] = req.temperature.into();
        }
        if has_schema && mode != SCHEMA_OFF {
            let strict = mode == SCHEMA_STRICT;
            let schema = if strict {
                strictify(req.schema)
            } else {
                req.schema.clone()
            };
            body["response_format"] = serde_json::json!({
                "type": "json_schema",
                "json_schema": { "name": "kai_reply", "schema": schema, "strict": strict }
            });
        }
        if self.stream {
            body["stream"] = true.into();
            body["stream_options"] = serde_json::json!({ "include_usage": true });
        }
        body
    }

    async fn send(&self, body: &serde_json::Value) -> Result<reqwest::Response, ClientError> {
        let mut req = self.http.post(format!("{}/chat/completions", self.base));
        if !self.api_key.is_empty() {
            req = req.bearer_auth(&self.api_key);
        }
        req.json(body)
            .timeout(self.timeout)
            .send()
            .await
            .map_err(|e| send_error(e, self.timeout))
    }
}

/// How many times a 429 is waited out before giving up.
const MAX_RATE_LIMIT_TRIES: u32 = 8;

/// `Retry-After` in seconds, capped at a minute.
fn retry_after(resp: &reqwest::Response) -> Option<Duration> {
    let secs: f64 = resp
        .headers()
        .get(reqwest::header::RETRY_AFTER)?
        .to_str()
        .ok()?
        .trim()
        .parse()
        .ok()?;
    Some(Duration::from_secs_f64(secs.clamp(1.0, 60.0)))
}

/// OpenAI strict mode wants every property listed in `required` and no
/// additional properties. Optional fields in Kai's schemas are already
/// nullable (or arrays/strings that may be empty), so this keeps meaning.
pub(crate) fn strictify(schema: &serde_json::Value) -> serde_json::Value {
    let mut s = schema.clone();
    if let Some(props) = s.get_mut("properties").and_then(|p| p.as_object_mut()) {
        let keys: Vec<serde_json::Value> = props.keys().map(|k| k.clone().into()).collect();
        for v in props.values_mut() {
            *v = strictify(v);
        }
        s["required"] = keys.into();
        s["additionalProperties"] = false.into();
    }
    if let Some(items) = s.get("items").cloned() {
        s["items"] = strictify(&items);
    }
    for key in ["anyOf", "oneOf", "allOf"] {
        if let Some(arr) = s.get(key).and_then(|a| a.as_array()).cloned() {
            s[key] = arr.iter().map(strictify).collect::<Vec<_>>().into();
        }
    }
    s
}

fn image_part(jpeg: &[u8]) -> serde_json::Value {
    let b64 = base64::engine::general_purpose::STANDARD.encode(jpeg);
    serde_json::json!({
        "type": "image_url",
        "image_url": { "url": format!("data:image/jpeg;base64,{b64}") }
    })
}

fn mentions_schema(body: &str) -> bool {
    let l = body.to_ascii_lowercase();
    l.contains("response_format") || l.contains("json_schema") || l.contains("schema")
}

fn mentions_reasoning(body: &str) -> bool {
    body.to_ascii_lowercase().contains("reasoning")
}

#[async_trait]
impl ModelClient for OpenAiClient {
    fn backend(&self) -> Backend {
        if self.local {
            Backend::Local
        } else {
            Backend::OpenAi
        }
    }

    async fn decide(&self, req: &ModelRequest<'_>) -> Result<ModelReply, ClientError> {
        if self.api_key.is_empty() && !self.local {
            return Err(ClientError::MissingKey(
                "AI_GATEWAY_API_KEY or OPENAI_API_KEY",
            ));
        }
        let mut attempts = 0;
        loop {
            attempts += 1;
            let mode = self.schema_mode.load(Ordering::Relaxed);
            let effort = self.effort.load(Ordering::Relaxed);
            let started = Instant::now();
            let resp = self.send(&self.body(req, mode, effort)).await?;
            let status = resp.status();
            if status.as_u16() == 429 && attempts < MAX_RATE_LIMIT_TRIES {
                // Free tiers allow a few requests a minute: wait and retry.
                // The wait is not model time (`started` resets next loop).
                let wait = retry_after(&resp).unwrap_or(Duration::from_secs(15));
                log::warn!("{} rate-limited; retrying in {wait:?}", self.model);
                tokio::time::sleep(wait).await;
                continue;
            }
            if !status.is_success() {
                let txt = resp.text().await.unwrap_or_default();
                if status.as_u16() == 400 && attempts < 4 {
                    if is_reasoning(&self.model) && effort < EFFORT_OMIT && mentions_reasoning(&txt)
                    {
                        log::warn!("{} rejected reasoning_effort; stepping down", self.model);
                        self.effort.store(effort + 1, Ordering::Relaxed);
                        continue;
                    }
                    if !req.schema.is_null() && mode < SCHEMA_OFF && mentions_schema(&txt) {
                        log::warn!("{} rejected response_format; stepping down", self.model);
                        self.schema_mode.store(mode + 1, Ordering::Relaxed);
                        continue;
                    }
                }
                return Err(ClientError::from_status(status.as_u16(), &txt));
            }
            let out = if self.stream {
                stream::read_chat_stream(resp, started, self.early_stop).await?
            } else {
                let v: serde_json::Value = resp
                    .json()
                    .await
                    .map_err(|e| ClientError::Transport(e.to_string()))?;
                let mut o = StreamOutcome::default();
                o.apply(&v, started);
                o.first_token = None; // not measurable without streaming
                o
            };
            if out.content.trim().is_empty() {
                return Err(ClientError::EmptyBody);
            }
            return Ok(ModelReply {
                raw: out.content,
                prompt_tokens: out.prompt_tokens,
                completion_tokens: out.completion_tokens,
                first_token: out.first_token,
                total: started.elapsed(),
                backend: self.backend(),
                cost_usd: out.cost_usd,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::Images;

    fn with_req(f: impl FnOnce(&ModelRequest<'_>)) {
        let imgs = Images {
            screenshot_jpeg: vec![1, 2, 3],
            crop_jpeg: Some(vec![4]),
        };
        let schema = crate::schema::Decision::json_schema();
        f(&ModelRequest {
            system: "SYS",
            user_text: "USER",
            images: &imgs,
            schema: &schema,
            max_tokens: 192,
            temperature: 0.0,
        });
    }

    #[test]
    fn gateway_ids_are_mapped() {
        assert_eq!(
            gateway_model_id("gemma-4-26b-a4b-it"),
            "google/gemma-4-26b-a4b-it"
        );
        assert_eq!(gateway_model_id("openai/gpt-5-mini"), "openai/gpt-5-mini");
        assert_eq!(gateway_model_id("gpt-4.1-mini"), "openai/gpt-4.1-mini");
        assert_eq!(gateway_model_id("o4-mini"), "openai/o4-mini");
    }

    #[test]
    fn gemma_body_has_images_schema_hint_and_stream() {
        let c = OpenAiClient::gateway("k");
        with_req(|r| {
            let b = c.body(r, SCHEMA_STRICT, EFFORT_MINIMAL);
            assert_eq!(b["model"], "google/gemma-4-26b-a4b-it");
            assert_eq!(b["max_tokens"], 192);
            assert!(b.get("reasoning_effort").is_none());
            let parts = b["messages"][1]["content"].as_array().unwrap();
            assert_eq!(parts.len(), 3);
            assert!(
                parts[1]["image_url"]["url"]
                    .as_str()
                    .unwrap()
                    .starts_with("data:image/jpeg;base64,")
            );
            assert!(
                b["messages"][0]["content"]
                    .as_str()
                    .unwrap()
                    .contains("JSON schema")
            );
            assert_eq!(b["response_format"]["json_schema"]["strict"], true);
            assert_eq!(b["stream_options"]["include_usage"], true);
        });
    }

    #[test]
    fn reasoning_models_get_completion_tokens_and_effort() {
        let c = OpenAiClient::gateway("k")
            .with_model("openai/gpt-5-mini")
            .with_streaming(false);
        with_req(|r| {
            let b = c.body(r, SCHEMA_STRICT, EFFORT_MINIMAL);
            assert_eq!(b["max_completion_tokens"], 192 + 256);
            assert_eq!(b["reasoning_effort"], "minimal");
            assert!(b.get("temperature").is_none());
            assert!(b.get("stream").is_none());
            // OpenAI enforces the schema, so no prompt hint.
            assert_eq!(b["messages"][0]["content"], "SYS");
            let off = c.body(r, SCHEMA_OFF, EFFORT_OMIT);
            assert!(off.get("response_format").is_none());
            assert!(off.get("reasoning_effort").is_none());
            assert!(
                off["messages"][0]["content"]
                    .as_str()
                    .unwrap()
                    .contains("JSON schema")
            );
        });
        let o4 = OpenAiClient::openai("k").with_model("o4-mini");
        assert_eq!(o4.effort.load(Ordering::Relaxed), EFFORT_LOW);
    }

    #[test]
    fn strictify_requires_every_property() {
        let s = strictify(&crate::schema::Decision::json_schema());
        let req = s["required"].as_array().unwrap();
        assert_eq!(req.len(), s["properties"].as_object().unwrap().len());
        assert_eq!(s["additionalProperties"], false);
    }
}
