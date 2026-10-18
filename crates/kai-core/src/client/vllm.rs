//! vLLM OpenAI-compatible backend (the DigitalOcean Droplet).
//!
//! Uses `response_format: json_schema` so the reply is always schema-valid, one
//! persistent HTTP/2 keep-alive connection, and image parts as data URLs.
//! Streams by default (same SSE wire format as OpenAI) for real TTFT.

use super::stream::{self, StreamOutcome};
use super::{Backend, ClientError, ModelClient, ModelReply, ModelRequest, send_error};
use async_trait::async_trait;
use base64::Engine;
use std::time::{Duration, Instant};

pub struct VllmClient {
    http: reqwest::Client,
    endpoint: String,
    api_key: String,
    model: String,
    timeout: Duration,
    stream: bool,
    early_stop: bool,
}

impl VllmClient {
    /// `endpoint` is the base, e.g. `https://host/v1`.
    pub fn new(endpoint: impl Into<String>, api_key: impl Into<String>) -> Self {
        // The shared client, so corporate roots (`SSL_CERT_FILE`) reach the
        // Droplet too.
        let http = super::http_client();
        Self {
            http,
            endpoint: endpoint.into().trim_end_matches('/').to_string(),
            api_key: api_key.into(),
            model: "gemma-4-26b-a4b-it".to_string(),
            timeout: Duration::from_secs(45),
            stream: true,
            early_stop: false,
        }
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

    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    pub fn with_timeout(mut self, t: Duration) -> Self {
        self.timeout = t;
        self
    }

    fn body(&self, req: &ModelRequest<'_>) -> serde_json::Value {
        let mut content = vec![serde_json::json!({
            "type": "text", "text": req.user_text
        })];
        // Text-only sub-agents send no screenshot.
        if !req.images.screenshot_jpeg.is_empty() {
            content.push(image_part(&req.images.screenshot_jpeg));
        }
        if let Some(crop) = &req.images.crop_jpeg {
            content.push(image_part(crop));
        }
        let mut body = serde_json::json!({
            "model": self.model,
            "max_tokens": req.max_tokens,
            "temperature": req.temperature,
            "messages": [
                { "role": "system", "content": req.system },
                { "role": "user", "content": content }
            ],
            "response_format": if req.schema.is_null() { serde_json::json!({"type":"text"}) } else { serde_json::json!({
                "type": "json_schema",
                "json_schema": { "name": "kai_reply", "schema": req.schema, "strict": true }
            }) }
        });
        if self.stream {
            body["stream"] = true.into();
            body["stream_options"] = serde_json::json!({ "include_usage": true });
        }
        body
    }

    /// Multi-choice generation requires a complete (non-streaming) response:
    /// the existing streaming reader only assembles choices[0].
    fn sample_body(&self, req: &ModelRequest<'_>, n: usize) -> serde_json::Value {
        let mut body = self.body(req);
        body["n"] = serde_json::json!(n);
        body["stream"] = serde_json::json!(false);
        body.as_object_mut().unwrap().remove("stream_options");
        body
    }

    fn parse_samples(
        v: &serde_json::Value,
        elapsed: Duration,
    ) -> Result<Vec<ModelReply>, ClientError> {
        let choices = v["choices"].as_array().ok_or(ClientError::EmptyBody)?;
        if choices.is_empty() {
            return Err(ClientError::EmptyBody);
        }
        let mut ordered = Vec::with_capacity(choices.len());
        for choice in choices {
            let index = choice["index"]
                .as_u64()
                .and_then(|n| usize::try_from(n).ok())
                .filter(|&n| n < choices.len())
                .ok_or(ClientError::EmptyBody)?;
            if choice["finish_reason"] == "length" {
                return Err(ClientError::EmptyBody);
            }
            let content = choice["message"]["content"]
                .as_str()
                .ok_or(ClientError::EmptyBody)?;
            ordered.push((index, content));
        }
        ordered.sort_unstable_by_key(|(index, _)| *index);
        if ordered
            .iter()
            .enumerate()
            .any(|(i, (index, _))| i != *index)
        {
            return Err(ClientError::EmptyBody);
        }
        let mut replies = Vec::with_capacity(choices.len());
        for (i, (_, content)) in ordered.into_iter().enumerate() {
            replies.push(ModelReply {
                raw: content.to_owned(),
                // Request-level totals belong to the batch, not each choice.
                prompt_tokens: (i == 0)
                    .then(|| v["usage"]["prompt_tokens"].as_u64().map(|n| n as u32))
                    .flatten(),
                completion_tokens: (i == 0)
                    .then(|| v["usage"]["completion_tokens"].as_u64().map(|n| n as u32))
                    .flatten(),
                first_token: None,
                total: if i == 0 { elapsed } else { Duration::ZERO },
                backend: Backend::Vllm,
                cost_usd: None,
            });
        }
        Ok(replies)
    }
}

fn image_part(jpeg: &[u8]) -> serde_json::Value {
    let b64 = base64::engine::general_purpose::STANDARD.encode(jpeg);
    serde_json::json!({
        "type": "image_url",
        "image_url": { "url": format!("data:image/jpeg;base64,{b64}") }
    })
}

#[async_trait]
impl ModelClient for VllmClient {
    fn backend(&self) -> Backend {
        Backend::Vllm
    }

    async fn decide(&self, req: &ModelRequest<'_>) -> Result<ModelReply, ClientError> {
        let started = Instant::now();
        // A transient rate limit must not fail the whole task. After three
        // attempts, return 429 so a configured secondary can take over.
        let mut rate_limited = 0;
        let resp = loop {
            let response = self
                .http
                .post(format!("{}/chat/completions", self.endpoint))
                .bearer_auth(&self.api_key)
                .json(&self.body(req))
                .timeout(self.timeout)
                .send()
                .await
                .map_err(|e| send_error(e, self.timeout))?;
            if response.status().as_u16() != 429 || rate_limited >= 2 {
                break response;
            }
            rate_limited += 1;
            let after = response
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|h| h.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(rate_limited)
                .min(10);
            tokio::time::sleep(Duration::from_secs(after)).await;
        };

        let status = resp.status();
        if !status.is_success() {
            let txt = resp.text().await.unwrap_or_default();
            return Err(ClientError::from_status(status.as_u16(), &txt));
        }
        let mut out = if self.stream {
            stream::read_chat_stream(resp, started, self.early_stop && !req.schema.is_null())
                .await?
        } else {
            let v: serde_json::Value = resp
                .json()
                .await
                .map_err(|e| ClientError::Transport(e.to_string()))?;
            let mut o = StreamOutcome::default();
            o.apply(&v, started);
            o.first_token = None;
            o
        };
        out.require_content()?;
        Ok(ModelReply {
            raw: out.content,
            prompt_tokens: out.prompt_tokens,
            completion_tokens: out.completion_tokens,
            first_token: out.first_token,
            total: started.elapsed(),
            backend: Backend::Vllm,
            cost_usd: None,
        })
    }

    async fn decide_with_tool(
        &self,
        req: &ModelRequest<'_>,
        tool: &serde_json::Value,
    ) -> Result<ModelReply, ClientError> {
        let started = Instant::now();
        let body = super::tool_call::request_body(self.body(req), tool);
        let resp = self
            .http
            .post(format!("{}/chat/completions", self.endpoint))
            .bearer_auth(&self.api_key)
            .json(&body)
            .timeout(self.timeout)
            .send()
            .await
            .map_err(|e| send_error(e, self.timeout))?;
        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let txt = resp.text().await.unwrap_or_default();
            return Err(ClientError::from_status(status, &txt));
        }
        let payload: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| ClientError::Transport(e.to_string()))?;
        super::tool_call::parse_reply(&payload, self.backend(), started.elapsed())
    }

    async fn sample(
        &self,
        req: &ModelRequest<'_>,
        n: usize,
    ) -> Vec<Result<ModelReply, ClientError>> {
        if n <= 1 {
            return vec![self.decide(req).await];
        }
        let started = Instant::now();
        let result = self
            .http
            .post(format!("{}/chat/completions", self.endpoint))
            .bearer_auth(&self.api_key)
            .json(&self.sample_body(req, n))
            .timeout(self.timeout)
            .send()
            .await;
        let response = match result {
            Ok(r) => r,
            Err(e) => return vec![Err(send_error(e, self.timeout))],
        };
        if !response.status().is_success() {
            let status = response.status().as_u16();
            let message = response.text().await.unwrap_or_default();
            // Older OpenAI-compatible vLLM endpoints can reject `n`. Only
            // then use the standard K independent calls.
            if matches!(status, 400 | 422)
                && message.to_ascii_lowercase().contains("n")
                && (message.contains("unsupported")
                    || message.contains("must be")
                    || message.contains("not support"))
            {
                return futures::future::join_all((0..n).map(|_| self.decide(req))).await;
            }
            return vec![Err(ClientError::from_status(status, &message))];
        }
        let parsed = response
            .json::<serde_json::Value>()
            .await
            .map_err(|e| ClientError::Transport(e.to_string()))
            .and_then(|v| Self::parse_samples(&v, started.elapsed()));
        match parsed {
            Ok(replies) if replies.len() == n => replies.into_iter().map(Ok).collect(),
            Ok(_) => vec![Err(ClientError::EmptyBody)],
            Err(e) => vec![Err(e)],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::Images;

    #[test]
    fn body_has_system_first_then_image() {
        let c = VllmClient::new("https://x/v1", "k");
        let imgs = Images {
            screenshot_jpeg: vec![1, 2, 3],
            crop_jpeg: None,
        };
        let schema = crate::schema::Decision::json_schema();
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
        let body = c.body(&req);
        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(body["messages"][0]["content"], "SYS");
        // Structured outputs requested.
        assert_eq!(body["response_format"]["type"], "json_schema");
        // First user part is text, second is the image.
        assert_eq!(body["messages"][1]["content"][0]["type"], "text");
        assert_eq!(body["messages"][1]["content"][1]["type"], "image_url");
    }

    #[test]
    fn crop_adds_a_second_image() {
        let c = VllmClient::new("https://x/v1", "k");
        let imgs = Images {
            screenshot_jpeg: vec![1],
            crop_jpeg: Some(vec![2]),
        };
        let schema = crate::schema::Decision::json_schema();
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
        let body = c.body(&req);
        assert_eq!(body["messages"][1]["content"].as_array().unwrap().len(), 3);
    }

    #[test]
    fn sampling_sets_n_and_decodes_all_choices_without_double_counting_tokens() {
        let c = VllmClient::new("https://x/v1", "k");
        let images = Images {
            screenshot_jpeg: vec![],
            crop_jpeg: None,
        };
        let schema = serde_json::json!({"type":"object"});
        let req = ModelRequest {
            observation_id: None,
            computer_result: None,
            system: "system",
            user_text: "task",
            images: &images,
            schema: &schema,
            max_tokens: 128,
            temperature: 0.7,
        };
        let body = c.sample_body(&req, 4);
        assert_eq!(body["n"], 4);
        assert_eq!(body["stream"], false);
        assert!(body.get("stream_options").is_none());
        let payload = serde_json::json!({"choices": [
            {"index":0,"message":{"content":"one"}}, {"index":1,"message":{"content":"two"}},
            {"index":2,"message":{"content":"three"}}, {"index":3,"message":{"content":"four"}}
        ],"usage":{"prompt_tokens":12,"completion_tokens":40}});
        let replies = VllmClient::parse_samples(&payload, Duration::from_millis(200)).unwrap();
        assert_eq!(replies.len(), 4);
        assert_eq!(replies[3].raw, "four");
        assert_eq!(
            replies
                .iter()
                .map(|r| r.prompt_tokens.unwrap_or(0))
                .sum::<u32>(),
            12
        );
        assert_eq!(
            replies
                .iter()
                .map(|r| r.completion_tokens.unwrap_or(0))
                .sum::<u32>(),
            40
        );
    }

    #[test]
    fn sampling_uses_choice_indices_and_rejects_missing_or_duplicate_choices() {
        let elapsed = Duration::from_millis(150);
        let reordered = serde_json::json!({"choices": [
            {"index":2,"message":{"content":"third"}},
            {"index":0,"message":{"content":"first"}},
            {"index":1,"message":{"content":"second"}}
        ],"usage":{"prompt_tokens":5,"completion_tokens":9}});
        let replies = VllmClient::parse_samples(&reordered, elapsed).unwrap();
        assert_eq!(
            replies.iter().map(|r| r.raw.as_str()).collect::<Vec<_>>(),
            vec!["first", "second", "third"]
        );
        assert_eq!(
            replies.iter().filter_map(|r| r.prompt_tokens).sum::<u32>(),
            5
        );
        let duplicate = serde_json::json!({"choices": [
            {"index":0,"message":{"content":"a"}},
            {"index":0,"message":{"content":"b"}}
        ]});
        assert!(VllmClient::parse_samples(&duplicate, elapsed).is_err());
        let missing = serde_json::json!({"choices": [
            {"index":0,"message":{"content":"a"}},
            {"index":2,"message":{"content":"b"}}
        ]});
        assert!(VllmClient::parse_samples(&missing, elapsed).is_err());
    }
}
