//! Gemini API backend (`generateContent`) for Gemma 4.
//!
//! JSON mode via `responseMimeType`, schema in the prompt (not forced function
//! calling — that hung in testing), `thinkingLevel: minimal` (thinking off for
//! Gemma per Google's docs) and `responseJsonSchema` for the reply schema, each
//! with a retry-and-remember fallback if the model rejects the field.

use super::{Backend, ClientError, ModelClient, ModelReply, ModelRequest};
use async_trait::async_trait;
use base64::Engine;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::{Duration, Instant};

/// Thinking off for Flash, the cheapest for Gemma: one step needs no plan
/// (DECISIONS.md records why it is "minimal", not "low").
const THINKING_LEVEL: &str = "minimal";
const THINKING_UNKNOWN: u8 = 0;
const THINKING_SUPPORTED: u8 = 1;
const THINKING_UNSUPPORTED: u8 = 2;

/// How many 429/5xx replies are retried before giving up.
const MAX_TRANSIENT_TRIES: u32 = 8;
/// The longest single wait on a rate limit (a per-day quota asks for hours;
/// that run fails after its retries instead of hanging).
const MAX_RETRY_WAIT: Duration = Duration::from_secs(60);

/// `"a, b"` → `["a", "b"]`; never empty (an empty key fails at the API).
fn split_keys(raw: &str) -> Vec<String> {
    let keys: Vec<String> = raw
        .split(',')
        .map(str::trim)
        .filter(|k| !k.is_empty())
        .map(String::from)
        .collect();
    if keys.is_empty() {
        vec![String::new()]
    } else {
        keys
    }
}

pub struct GeminiClient {
    http: reqwest::Client,
    base: String,
    /// One or more keys (comma-separated in the setting); used in turn, so
    /// per-key rate limits add up.
    api_keys: Vec<String>,
    next_key: std::sync::atomic::AtomicUsize,
    model: String,
    timeout: Duration,
    /// Remembered across calls: does this model accept thinkingConfig?
    thinking: AtomicU8,
    /// Remembered across calls: does this model accept responseJsonSchema?
    json_schema: AtomicU8,
}

impl GeminiClient {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            http: super::http_client(),
            base: "https://generativelanguage.googleapis.com/v1beta".to_string(),
            api_keys: split_keys(&api_key.into()),
            next_key: std::sync::atomic::AtomicUsize::new(0),
            model: "gemma-4-26b-a4b-it".to_string(),
            timeout: Duration::from_secs(45),
            thinking: AtomicU8::new(THINKING_UNKNOWN),
            json_schema: AtomicU8::new(THINKING_UNKNOWN),
        }
    }

    /// API root, e.g. `https://generativelanguage.googleapis.com/v1beta`.
    pub fn with_base(mut self, base: impl Into<String>) -> Self {
        self.base = base.into().trim_end_matches('/').to_string();
        self
    }

    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    fn body(
        &self,
        req: &ModelRequest<'_>,
        include_thinking: bool,
        include_schema: bool,
    ) -> serde_json::Value {
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
            if include_schema {
                gen_cfg["responseJsonSchema"] = req.schema.clone();
            }
        }
        if include_thinking {
            gen_cfg["thinkingConfig"] = serde_json::json!({ "thinkingLevel": THINKING_LEVEL });
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
        include_schema: bool,
    ) -> Result<(reqwest::StatusCode, serde_json::Value, Duration), ClientError> {
        let url = format!("{}/models/{}:generateContent", self.base, self.model);
        let i = self.next_key.fetch_add(1, Ordering::Relaxed);
        let key = &self.api_keys[i % self.api_keys.len().max(1)];
        let started = Instant::now();
        let resp = self
            .http
            .post(url)
            .header("x-goog-api-key", key)
            .json(&self.body(req, include_thinking, include_schema))
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
        // Each optional field is dropped (and remembered) if the model rejects
        // it; at most one retry per field.
        let mut attempts = 0;
        let (status, v, elapsed) = loop {
            attempts += 1;
            let thinking = self.thinking.load(Ordering::Relaxed) != THINKING_UNSUPPORTED;
            let schema = !req.schema.is_null()
                && self.json_schema.load(Ordering::Relaxed) != THINKING_UNSUPPORTED;
            let (status, v, elapsed) = self.call(req, thinking, schema).await?;
            if status.is_success() {
                if thinking {
                    self.thinking.store(THINKING_SUPPORTED, Ordering::Relaxed);
                }
                if schema {
                    self.json_schema
                        .store(THINKING_SUPPORTED, Ordering::Relaxed);
                }
                break (status, v, elapsed);
            }
            // Rate limit or a transient server error: the next call uses the
            // next key. Waits are not model time (`elapsed` is per call).
            let code = status.as_u16();
            if (code == 429 || code == 500 || code == 503) && attempts < MAX_TRANSIENT_TRIES {
                let all_keys_tried = (attempts as usize).is_multiple_of(self.api_keys.len().max(1));
                // Once every key is limited, wait as long as the API asks
                // (its `RetryInfo`), so retries do not burn more quota.
                let wait = match code {
                    429 if all_keys_tried => retry_delay(&v)
                        .unwrap_or(Duration::ZERO)
                        .clamp(Duration::from_secs(10), MAX_RETRY_WAIT),
                    429 => Duration::ZERO,
                    _ => Duration::from_millis(800),
                };
                log::warn!(
                    "Gemini {code} on {}{}; retrying in {wait:?}",
                    self.model,
                    quota_metric(&v)
                        .map(|m| format!(" ({m})"))
                        .unwrap_or_default()
                );
                tokio::time::sleep(wait).await;
                continue;
            }
            if attempts < 3 && thinking && mentions(&v, "thinking") {
                self.thinking.store(THINKING_UNSUPPORTED, Ordering::Relaxed);
                log::warn!("Gemini rejected thinkingConfig; retrying without it (remembered)");
                continue;
            }
            if attempts < 3 && schema && mentions(&v, "schema") {
                self.json_schema
                    .store(THINKING_UNSUPPORTED, Ordering::Relaxed);
                log::warn!("Gemini rejected responseJsonSchema; plain JSON mode (remembered)");
                continue;
            }
            break (status, v, elapsed);
        };

        if !status.is_success() {
            return Err(ClientError::from_status(status.as_u16(), &v.to_string()));
        }

        // Skip thought parts (if any); join the answer text.
        let raw: String = v["candidates"][0]["content"]["parts"]
            .as_array()
            .ok_or(ClientError::EmptyBody)?
            .iter()
            .filter(|p| p["thought"] != true)
            .filter_map(|p| p["text"].as_str())
            .collect();
        if raw.is_empty() {
            return Err(ClientError::EmptyBody);
        }

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
            cost_usd: None,
        })
    }
}

/// `RetryInfo.retryDelay` (`"23s"`, `"1.5s"`) from a 429 error body.
fn retry_delay(v: &serde_json::Value) -> Option<Duration> {
    v["error"]["details"]
        .as_array()?
        .iter()
        .find_map(|d| d["retryDelay"].as_str())
        .and_then(|s| s.trim().strip_suffix('s')?.parse::<f64>().ok())
        // `try_`: a huge value ("1e30s") must not panic the run.
        .and_then(|s| Duration::try_from_secs_f64(s).ok())
}

/// Which quota a 429 hit (`QuotaFailure.violations[].quotaMetric`), for the
/// log: per-minute tokens and requests need different pacing.
fn quota_metric(v: &serde_json::Value) -> Option<String> {
    let m = v["error"]["details"]
        .as_array()?
        .iter()
        .filter_map(|d| d["violations"].as_array())
        .flatten()
        .filter_map(|x| x["quotaId"].as_str().or(x["quotaMetric"].as_str()))
        .collect::<Vec<_>>()
        .join(", ");
    (!m.is_empty()).then_some(m)
}

fn mentions(v: &serde_json::Value, what: &str) -> bool {
    v.to_string().to_ascii_lowercase().contains(what)
}

#[cfg(test)]
mod tests {

    #[test]
    fn several_keys_split_on_commas() {
        assert_eq!(
            split_keys(" a , b,"),
            vec!["a".to_string(), "b".to_string()]
        );
        assert_eq!(split_keys(""), vec![String::new()]);
    }

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
            observation_id: None,
            computer_result: None,
            system: "SYS",
            user_text: "USER",
            images: &imgs,
            schema: &schema,
            max_tokens: 64,
            temperature: 0.0,
        };
        let body = c.body(&req, true, true);
        assert_eq!(
            body["generationConfig"]["responseMimeType"],
            "application/json"
        );
        assert_eq!(
            body["generationConfig"]["thinkingConfig"]["thinkingLevel"],
            "minimal"
        );
        assert_eq!(body["generationConfig"]["responseJsonSchema"], schema);
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
            observation_id: None,
            computer_result: None,
            system: "s",
            user_text: "u",
            images: &imgs,
            schema: &schema,
            max_tokens: 1,
            temperature: 0.0,
        };
        let body = c.body(&req, false, false);
        assert!(body["generationConfig"].get("thinkingConfig").is_none());
        assert!(body["generationConfig"].get("responseJsonSchema").is_none());
    }

    #[test]
    fn a_rate_limit_says_how_long_to_wait_and_which_quota() {
        let v = serde_json::json!({"error": {"code": 429, "details": [
            {"@type": "type.googleapis.com/google.rpc.QuotaFailure", "violations": [
                {"quotaMetric": "generativelanguage.googleapis.com/generate_content_free_tier_input_token_count",
                 "quotaId": "GenerateContentInputTokensPerModelPerMinute-FreeTier"}]},
            {"@type": "type.googleapis.com/google.rpc.RetryInfo", "retryDelay": "23.5s"}
        ]}});
        assert_eq!(retry_delay(&v), Some(Duration::from_millis(23_500)));
        assert_eq!(
            quota_metric(&v).as_deref(),
            Some("GenerateContentInputTokensPerModelPerMinute-FreeTier")
        );
        let none = serde_json::json!({"error": {"code": 429, "message": "slow down"}});
        assert_eq!(retry_delay(&none), None);
        assert_eq!(quota_metric(&none), None);
        for bad in ["-1s", "NaNs", "infs", "1e30s", "s", "12"] {
            let v = serde_json::json!({"error": {"details": [{"retryDelay": bad}]}});
            assert_eq!(retry_delay(&v), None, "{bad}");
        }
    }
}
