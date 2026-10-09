//! Gemini API backend (`generateContent`) for Gemma 4.
//!
//! JSON mode via `responseMimeType`, schema in the prompt (not forced function
//! calling — that hung in testing), `thinkingLevel: low` with a
//! retry-and-remember fallback if the model rejects the thinking field.

use super::{Backend, ClientError, ModelClient, ModelReply, ModelRequest};
use async_trait::async_trait;
use base64::Engine;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::{Duration, Instant};

const THINKING_UNKNOWN: u8 = 0;
const THINKING_SUPPORTED: u8 = 1;
const THINKING_UNSUPPORTED: u8 = 2;

pub struct GeminiClient {
    http: reqwest::Client,
    base: String,
    api_key: String,
    model: String,
    timeout: Duration,
    /// Remembered across calls: does this model accept thinkingConfig?
    thinking: AtomicU8,
}

impl GeminiClient {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            http: reqwest::Client::builder()
                .pool_idle_timeout(Duration::from_secs(90))
                .build()
                .expect("reqwest client"),
            base: "https://generativelanguage.googleapis.com/v1beta".to_string(),
            api_key: api_key.into(),
            model: "gemma-4-26b-a4b-it".to_string(),
            timeout: Duration::from_secs(45),
            thinking: AtomicU8::new(THINKING_UNKNOWN),
        }
    }

    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    fn body(&self, req: &ModelRequest<'_>, include_thinking: bool) -> serde_json::Value {
        let mut parts = vec![serde_json::json!({ "text": req.user_text })];
        if !req.images.screenshot_jpeg.is_empty() {
            let b64 = base64::engine::general_purpose::STANDARD.encode(&req.images.screenshot_jpeg);
            parts.push(
                serde_json::json!({ "inlineData": { "mimeType": "image/jpeg", "data": b64 } }),
            );
        }
        if let Some(crop) = &req.images.crop_jpeg {
            let cb = base64::engine::general_purpose::STANDARD.encode(crop);
            parts.push(
                serde_json::json!({ "inlineData": { "mimeType": "image/jpeg", "data": cb } }),
            );
        }

        // Gemma over the Gemini API has no systemInstruction channel in some
        // builds, so fold the schema + system prompt into the prompt text and
        // ask for JSON. We still send systemInstruction; harmless if ignored.
        let schema_hint = if req.schema.is_null() {
            String::new()
        } else {
            format!(
                "Reply with ONLY a JSON object matching this schema (no prose, no markdown):\n{}",
                serde_json::to_string(req.schema).unwrap_or_default()
            )
        };

        let mut gen_cfg = serde_json::json!({
            "temperature": req.temperature,
            "maxOutputTokens": req.max_tokens,
        });
        if !req.schema.is_null() {
            gen_cfg["responseMimeType"] = serde_json::json!("application/json");
        }
        if include_thinking {
            gen_cfg["thinkingConfig"] = serde_json::json!({ "thinkingLevel": "low" });
        }

        let mut all_parts = Vec::new();
        if !schema_hint.is_empty() {
            all_parts.push(serde_json::json!({ "text": schema_hint }));
        }
        all_parts.extend(parts);
        serde_json::json!({
            "systemInstruction": { "parts": [{ "text": req.system }] },
            "contents": [{
                "role": "user",
                "parts": all_parts
            }],
            "generationConfig": gen_cfg
        })
    }

    async fn call(
        &self,
        req: &ModelRequest<'_>,
        include_thinking: bool,
    ) -> Result<(reqwest::StatusCode, serde_json::Value, Duration), ClientError> {
        let url = format!("{}/models/{}:generateContent", self.base, self.model);
        let started = Instant::now();
        let resp = self
            .http
            .post(url)
            .header("x-goog-api-key", &self.api_key)
            .json(&self.body(req, include_thinking))
            .timeout(self.timeout)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    ClientError::Timeout(self.timeout)
                } else {
                    ClientError::Transport(e.to_string())
                }
            })?;
        let status = resp.status();
        let v: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| ClientError::Transport(e.to_string()))?;
        Ok((status, v, started.elapsed()))
    }
}

#[async_trait]
impl ModelClient for GeminiClient {
    fn backend(&self) -> Backend {
        Backend::Gemini
    }

    async fn decide(&self, req: &ModelRequest<'_>) -> Result<ModelReply, ClientError> {
        let include = self.thinking.load(Ordering::Relaxed) != THINKING_UNSUPPORTED;
        let (status, v, elapsed) = self.call(req, include).await?;

        // If the thinking field was the problem, retry once without it and
        // remember for the rest of the process.
        let (status, v, elapsed) = if !status.is_success() && include && mentions_thinking(&v) {
            self.thinking.store(THINKING_UNSUPPORTED, Ordering::Relaxed);
            log::warn!("Gemini rejected thinkingConfig; retrying without it (remembered)");
            self.call(req, false).await?
        } else {
            if include && status.is_success() {
                self.thinking.store(THINKING_SUPPORTED, Ordering::Relaxed);
            }
            (status, v, elapsed)
        };

        if !status.is_success() {
            return Err(ClientError::Status(
                status.as_u16(),
                truncate(&v.to_string()),
            ));
        }

        let raw = v["candidates"][0]["content"]["parts"][0]["text"]
            .as_str()
            .ok_or(ClientError::EmptyBody)?
            .to_string();

        Ok(ModelReply {
            raw,
            prompt_tokens: v["usageMetadata"]["promptTokenCount"]
                .as_u64()
                .map(|n| n as u32),
            completion_tokens: v["usageMetadata"]["candidatesTokenCount"]
                .as_u64()
                .map(|n| n as u32),
            first_token: None,
            total: elapsed,
            backend: Backend::Gemini,
        })
    }
}

fn mentions_thinking(v: &serde_json::Value) -> bool {
    v.to_string().to_ascii_lowercase().contains("thinking")
}

fn truncate(s: &str) -> String {
    s.chars().take(300).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::Images;

    fn req_fixture() -> (Images, serde_json::Value) {
        (
            Images {
                screenshot_jpeg: vec![9, 9, 9],
                crop_jpeg: None,
            },
            crate::schema::Decision::json_schema(),
        )
    }

    #[test]
    fn body_asks_for_json_and_includes_thinking_by_default() {
        let c = GeminiClient::new("key");
        let (imgs, schema) = req_fixture();
        let req = ModelRequest {
            system: "SYS",
            user_text: "USER",
            images: &imgs,
            schema: &schema,
            max_tokens: 64,
            temperature: 0.0,
        };
        let body = c.body(&req, true);
        assert_eq!(
            body["generationConfig"]["responseMimeType"],
            "application/json"
        );
        assert_eq!(
            body["generationConfig"]["thinkingConfig"]["thinkingLevel"],
            "low"
        );
        assert_eq!(body["systemInstruction"]["parts"][0]["text"], "SYS");
        // inlineData image present.
        let parts = body["contents"][0]["parts"].as_array().unwrap();
        assert!(parts.iter().any(|p| p.get("inlineData").is_some()));
    }

    #[test]
    fn body_can_drop_thinking() {
        let c = GeminiClient::new("key");
        let (imgs, schema) = req_fixture();
        let req = ModelRequest {
            system: "s",
            user_text: "u",
            images: &imgs,
            schema: &schema,
            max_tokens: 1,
            temperature: 0.0,
        };
        let body = c.body(&req, false);
        assert!(body["generationConfig"].get("thinkingConfig").is_none());
    }
}
