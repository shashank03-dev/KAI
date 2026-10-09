//! vLLM OpenAI-compatible backend (the DigitalOcean Droplet).
//!
//! Uses `response_format: json_schema` so the reply is always schema-valid, one
//! persistent HTTP/2 keep-alive connection, and image parts as data URLs.

use super::{Backend, ClientError, ModelClient, ModelReply, ModelRequest};
use async_trait::async_trait;
use base64::Engine;
use std::time::{Duration, Instant};

pub struct VllmClient {
    http: reqwest::Client,
    endpoint: String,
    api_key: String,
    model: String,
    timeout: Duration,
}

impl VllmClient {
    /// `endpoint` is the base, e.g. `https://host/v1`.
    pub fn new(endpoint: impl Into<String>, api_key: impl Into<String>) -> Self {
        let http = reqwest::Client::builder()
            .pool_max_idle_per_host(2)
            .pool_idle_timeout(Duration::from_secs(90))
            .tcp_keepalive(Duration::from_secs(30))
            .build()
            .expect("reqwest client");
        Self {
            http,
            endpoint: endpoint.into().trim_end_matches('/').to_string(),
            api_key: api_key.into(),
            model: "gemma-4-26b-a4b-it".to_string(),
            timeout: Duration::from_secs(45),
        }
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
        serde_json::json!({
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
        })
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
        let resp = self
            .http
            .post(format!("{}/chat/completions", self.endpoint))
            .bearer_auth(&self.api_key)
            .json(&self.body(req))
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
        if !status.is_success() {
            let txt = resp.text().await.unwrap_or_default();
            return Err(ClientError::Status(status.as_u16(), truncate(&txt)));
        }
        let v: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| ClientError::Transport(e.to_string()))?;

        let raw = v["choices"][0]["message"]["content"]
            .as_str()
            .ok_or(ClientError::EmptyBody)?
            .to_string();

        Ok(ModelReply {
            raw,
            prompt_tokens: v["usage"]["prompt_tokens"].as_u64().map(|n| n as u32),
            completion_tokens: v["usage"]["completion_tokens"].as_u64().map(|n| n as u32),
            first_token: None,
            total: started.elapsed(),
            backend: Backend::Vllm,
        })
    }
}

fn truncate(s: &str) -> String {
    s.chars().take(300).collect()
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
}
