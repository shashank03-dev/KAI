//! A deterministic backend that replays recorded or scripted replies. Powers
//! the end-to-end loop test and offline UI work. No network.

use super::{Backend, ClientError, ModelClient, ModelReply, ModelRequest};
use async_trait::async_trait;
use std::sync::Mutex;
use std::time::Duration;

/// Replays a fixed list of raw replies in order, then errors (or repeats the
/// last one if `loop_last` is set).
pub struct MockClient {
    replies: Mutex<std::collections::VecDeque<String>>,
    loop_last: bool,
    last: Mutex<Option<String>>,
    /// Simulated per-call latency, for realistic trace timings in demos.
    latency: Duration,
}

impl MockClient {
    pub fn new(replies: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            replies: Mutex::new(replies.into_iter().map(Into::into).collect()),
            loop_last: false,
            last: Mutex::new(None),
            latency: Duration::from_millis(0),
        }
    }

    /// Convenience: one reply, repeated forever.
    pub fn always(reply: impl Into<String>) -> Self {
        let mut c = Self::new(std::iter::once(reply.into()));
        c.loop_last = true;
        c
    }

    pub fn repeating(mut self) -> Self {
        self.loop_last = true;
        self
    }

    pub fn with_latency(mut self, d: Duration) -> Self {
        self.latency = d;
        self
    }
}

#[async_trait]
impl ModelClient for MockClient {
    fn backend(&self) -> Backend {
        Backend::Mock
    }

    async fn decide(&self, _req: &ModelRequest<'_>) -> Result<ModelReply, ClientError> {
        if !self.latency.is_zero() {
            tokio::time::sleep(self.latency).await;
        }
        let raw = {
            let mut q = self.replies.lock().unwrap();
            match q.pop_front() {
                Some(r) => {
                    *self.last.lock().unwrap() = Some(r.clone());
                    r
                }
                None if self.loop_last => self
                    .last
                    .lock()
                    .unwrap()
                    .clone()
                    .ok_or(ClientError::EmptyBody)?,
                None => return Err(ClientError::EmptyBody),
            }
        };
        Ok(ModelReply {
            raw,
            prompt_tokens: Some(100),
            completion_tokens: Some(20),
            first_token: Some(self.latency / 2),
            total: self.latency,
            backend: Backend::Mock,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::Images;

    fn empty_req<'a>(schema: &'a serde_json::Value, imgs: &'a Images) -> ModelRequest<'a> {
        ModelRequest {
            system: "s",
            user_text: "u",
            images: imgs,
            schema,
            max_tokens: 1,
            temperature: 0.0,
        }
    }

    #[tokio::test]
    async fn replays_in_order_then_errors() {
        let m = MockClient::new(["a", "b"]);
        let schema = crate::schema::Decision::json_schema();
        let imgs = Images {
            screenshot_jpeg: vec![],
            crop_jpeg: None,
        };
        assert_eq!(m.decide(&empty_req(&schema, &imgs)).await.unwrap().raw, "a");
        assert_eq!(m.decide(&empty_req(&schema, &imgs)).await.unwrap().raw, "b");
        assert!(m.decide(&empty_req(&schema, &imgs)).await.is_err());
    }

    #[tokio::test]
    async fn always_repeats() {
        let m = MockClient::always("x");
        let schema = crate::schema::Decision::json_schema();
        let imgs = Images {
            screenshot_jpeg: vec![],
            crop_jpeg: None,
        };
        for _ in 0..3 {
            assert_eq!(m.decide(&empty_req(&schema, &imgs)).await.unwrap().raw, "x");
        }
    }
}
