//! A cheap fallback [`Verifier`]: ask a small, fast text model the same yes/no
//! question JEV would get and read back `{"p": 0.0..1.0}`.

use super::jev::verifier_state;
use super::stream::complete_json_prefix;
use super::{ClientError, Images, ModelClient, ModelRequest, OpenAiClient};
use crate::ports::Verifier;
use async_trait::async_trait;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

const SYSTEM: &str = "You check whether a computer-use agent's last action worked. \
You get the task, what the screen should show now, and the current screen as a \
list of accessibility elements. Estimate the probability that the expectation is \
met. Reply with ONLY a JSON object like {\"p\": 0.85}.";

pub struct LlmVerifier {
    client: Box<dyn ModelClient>,
    off: AtomicBool,
}

impl LlmVerifier {
    pub fn new(client: Box<dyn ModelClient>) -> Self {
        Self {
            client,
            off: AtomicBool::new(false),
        }
    }

    /// A small fast Gateway model (default `openai/gpt-4.1-nano`).
    pub fn gateway(api_key: impl Into<String>, model: Option<&str>) -> Self {
        let c = OpenAiClient::gateway(api_key)
            .with_model(model.unwrap_or("openai/gpt-4.1-nano"))
            .with_early_stop(true)
            .with_timeout(Duration::from_secs(8));
        Self::new(Box::new(c))
    }

    pub fn schema() -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["p"],
            "properties": { "p": { "type": "number" } }
        })
    }

    /// Read `p` out of a reply, clamped to 0..=1.
    pub fn parse_p(raw: &str) -> Option<f32> {
        let start = raw.find('{')?;
        let end = complete_json_prefix(raw)?;
        let v: serde_json::Value = serde_json::from_str(&raw[start..end]).ok()?;
        let p = v["p"]
            .as_f64()
            .or_else(|| v["p"].as_str().and_then(|s| s.parse().ok()))?;
        Some(p.clamp(0.0, 1.0) as f32)
    }
}

#[async_trait]
impl Verifier for LlmVerifier {
    async fn check(&self, task: &str, expect: &str, state: &str) -> Option<f32> {
        if self.off.load(Ordering::Relaxed) || expect.trim().is_empty() {
            return None;
        }
        let images = Images {
            screenshot_jpeg: Vec::new(),
            crop_jpeg: None,
        };
        let schema = Self::schema();
        let user = verifier_state(task, expect, state);
        let req = ModelRequest {
            observation_id: None,
            computer_result: None,
            system: SYSTEM,
            user_text: &user,
            images: &images,
            schema: &schema,
            max_tokens: 16,
            temperature: 0.0,
        };
        match self.client.decide(&req).await {
            Ok(r) => Self::parse_p(&r.raw),
            Err(e @ (ClientError::Unavailable(_) | ClientError::MissingKey(_))) => {
                log::warn!("LLM verifier off: {e}");
                self.off.store(true, Ordering::Relaxed);
                None
            }
            Err(e) => {
                log::debug!("LLM verifier skipped this step: {e}");
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::MockClient;

    #[test]
    fn parses_p() {
        assert_eq!(LlmVerifier::parse_p(r#"{"p":0.9}"#), Some(0.9));
        assert_eq!(
            LlmVerifier::parse_p("```json\n{\"p\": 1.7}\n```"),
            Some(1.0)
        );
        assert_eq!(LlmVerifier::parse_p("yes"), None);
    }

    #[tokio::test]
    async fn verifier_returns_probability() {
        let v = LlmVerifier::new(Box::new(MockClient::always(r#"{"p":0.25}"#)));
        assert_eq!(
            v.check("t", "a dialog opens", "#1 button").await,
            Some(0.25)
        );
        assert_eq!(v.check("t", "", "#1 button").await, None);
    }
}
