//! Model backends behind one trait. The harness talks only to [`ModelClient`];
//! swapping vLLM for Gemini for a mock changes nothing above this line.

mod failover;
mod gemini;
mod mock;
mod vllm;

pub use failover::FailoverClient;
pub use gemini::GeminiClient;
pub use mock::MockClient;
pub use vllm::VllmClient;

use async_trait::async_trait;
use std::time::Duration;

/// One screenshot plus an optional crop, already JPEG-encoded.
#[derive(Clone)]
pub struct Images {
    pub screenshot_jpeg: Vec<u8>,
    pub crop_jpeg: Option<Vec<u8>>,
}

/// Everything a single `decide` call needs. Laid out stable-first by
/// [`crate::prompt`] so vLLM prefix caching hits.
pub struct ModelRequest<'a> {
    /// Stable system prompt (cached prefix).
    pub system: &'a str,
    /// The changing user content: element list + last-3 history + task.
    pub user_text: &'a str,
    pub images: &'a Images,
    /// JSON schema for structured outputs (vLLM); Gemini puts it in the prompt.
    pub schema: &'a serde_json::Value,
    pub max_tokens: u32,
    pub temperature: f32,
}

/// What came back, with timings for the trace.
#[derive(Debug, Clone)]
pub struct ModelReply {
    pub raw: String,
    pub prompt_tokens: Option<u32>,
    pub completion_tokens: Option<u32>,
    /// Time to first token, when the backend was called in streaming mode.
    pub first_token: Option<Duration>,
    /// Wall time for the whole call.
    pub total: Duration,
    /// Which backend actually served this (for the status bar).
    pub backend: Backend,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    Vllm,
    Gemini,
    Mock,
}

impl Backend {
    pub fn label(self) -> &'static str {
        match self {
            Backend::Vllm => "DigitalOcean (Gemma 4)",
            Backend::Gemini => "Gemini API (Gemma 4)",
            Backend::Mock => "Mock",
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("network/transport error: {0}")]
    Transport(String),
    #[error("backend returned status {0}: {1}")]
    Status(u16, String),
    #[error("backend timed out after {0:?}")]
    Timeout(Duration),
    #[error("empty or malformed response body")]
    EmptyBody,
}

impl ClientError {
    /// Whether the fallback should kick in for this error (Droplet unreachable
    /// or slow → try Gemini).
    pub fn is_failover(&self) -> bool {
        matches!(
            self,
            ClientError::Transport(_) | ClientError::Timeout(_) | ClientError::Status(500..=599, _)
        )
    }
}

/// A model that can choose the next action from a screenshot + element list.
#[async_trait]
pub trait ModelClient: Send + Sync {
    fn backend(&self) -> Backend;

    /// One decision call. Returns the raw reply; the caller validates it.
    async fn decide(&self, req: &ModelRequest<'_>) -> Result<ModelReply, ClientError>;

    /// A cheap warm-up (used when the app opens / onboarding "Test" button).
    async fn warm_up(&self, images: &Images) -> Result<ModelReply, ClientError> {
        let schema = crate::schema::Decision::json_schema();
        let req = ModelRequest {
            system: "You are Kai. Reply with a `done` action.",
            user_text: "warm up",
            images,
            schema: &schema,
            max_tokens: 16,
            temperature: 0.0,
        };
        self.decide(&req).await
    }
}
